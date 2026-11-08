// Control-flow statements: `while`, `loop`, `for`, and `break`, with the loop
// labels each may carry. `if` chains live in `conditionals.rs`.
//
// One of the statement-shape parsers; each adds methods to the same
// `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;
use ast_types::{Expr, LoopAdapter, LoopAdapterKind, Stmt};

use super::Parser;

impl Parser {
    /// Parse a while statement, optionally prefixed with a loop `label`.
    pub(crate) fn parse_while_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();
        if self.check(&TokenKind::Val) {
            self.advance(); // consume 'val'
            return self.parse_while_val(start_span, label);
        }

        let condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = self.previous_span(condition.span());

        Ok(Stmt::While {
            label,
            condition,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a loop statement: `loop { ... }`, optionally `label`ed.
    ///
    /// Statement position yields `Stmt::Expr(Expr::Loop { .. })` rather than a
    /// statement-only node, so a trailing `loop` is recognised as a value-producing
    /// tail by the same test that recognises every other trailing expression.
    pub(crate) fn parse_loop_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = self.previous_span(start_span);

        Ok(Stmt::Expr(Expr::Loop {
            label,
            body,
            span: start_span.merge(end_span),
        }))
    }

    /// Parse a for statement: `for <head> in <iterable> { ... }`, optionally
    /// prefixed with a loop `label`.
    ///
    /// `<head>` is one loop variable, or the pair `(index, value)` that
    /// `.enumerate()` and `.char_indices()` bind. `<iterable>` is a numeric range, an expression
    /// yielding a sequence, or either of those wearing a chain of `.map(f)` /
    /// `.filter(p)` adapters and an outermost `.enumerate()`: the adapters are
    /// recognised here rather than type-checked as methods because a range is not a
    /// first-class value, so `(0..n).map(f)` has no receiver to resolve a method
    /// against.
    pub(crate) fn parse_for_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        let (index, iterator) = self.parse_for_head()?;

        self.skip_newlines();
        self.consume(TokenKind::In, "'in'")?;
        self.skip_newlines();

        // The iterable expression must not be a struct literal or `{` would be
        // consumed. Parse it at `Range` precedence so a `..` / `..=` separator is
        // not swallowed as a range expression; the operator (if any)
        // then distinguishes a numeric range from an array iterable.
        let start = self.guarded_header(|p| p.parse_expr(Precedence::Range))?;
        self.skip_newlines();

        let inclusive = if self.check(&TokenKind::DotDotEqual) {
            self.advance();
            true
        } else if self.check(&TokenKind::DotDot) {
            self.advance();
            false
        } else {
            // No range operator follows, so the whole iterable is already parsed:
            // either a sequence expression or a parenthesised range, each possibly
            // wearing an adapter chain.
            let (iterable, adapters, enumerated) = strip_adapters(start)?;
            let (iterable, step) = strip_step(iterable)?;
            let (iterable, reversed) = strip_rev(iterable)?;
            // `.char_indices()` binds its own position, so a chain around one would have
            // two sources for a single binding, and an adapter that drops or replaces
            // elements would leave the offsets naming text no longer being yielded.
            let char_indices = is_char_indices_head(&iterable);
            if char_indices && (enumerated || !adapters.is_empty()) {
                return Err(ParseError::CharIndicesHeadDecorated {
                    span: iterable.span(),
                });
            }
            let pair_head = match (enumerated, char_indices) {
                (true, _) => Some(ENUMERATE_METHOD),
                (_, true) => Some(CHAR_INDICES_METHOD),
                _ => None,
            };
            // A pair over a head that yields no position destructures the item, which the
            // checker requires to be a two-element tuple. Only a range is known here to
            // yield something else.
            let pair = index.is_some() && pair_head.is_none();
            if !pair {
                Self::check_head_agrees(&index, pair_head, iterable.span())?;
            }
            let mut body = self.parse_block()?;
            let end_span = self.previous_span(iterable.span());
            let span = start_span.merge(end_span);

            // A parenthesised range is the only way to write `(0..n).enumerate()`,
            // so unwrap it here rather than leaving a range value no other pass
            // knows how to consume.
            return Ok(match unwrap_paren(iterable) {
                Expr::Range { span, .. } if pair => {
                    return Err(ParseError::PairWithoutEnumerate { span });
                }
                Expr::Range {
                    start,
                    end,
                    inclusive,
                    ..
                } => Stmt::ForRange {
                    label,
                    index,
                    iterator,
                    start: *start,
                    end: *end,
                    inclusive,
                    reversed,
                    step,
                    adapters,
                    body,
                    span,
                },
                iterable => {
                    let (index, iterator) = match (pair, index) {
                        (true, Some(first)) => {
                            (None, self.bind_pair_item(first, iterator, &mut body))
                        }
                        (_, index) => (index, iterator),
                    };
                    Stmt::ForEach {
                        label,
                        index,
                        iterator,
                        iterable,
                        adapters,
                        pair,
                        body,
                        span,
                    }
                }
            });
        };

        self.skip_newlines();

        let end = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        // An unparenthesised range binds looser than a method call, so
        // `for i in 0..n.enumerate()` would enumerate `n`, not the range. Reject the
        // pair head here and point at the spelling that works.
        Self::check_head_agrees(&index, None, end.span())?;

        let body = self.parse_block()?;

        let end_span = self.previous_span(end.span());

        Ok(Stmt::ForRange {
            label,
            index: None,
            iterator,
            start,
            end,
            inclusive,
            reversed: false,
            step: None,
            adapters: Vec::new(),
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse the binding half of a `for` head: one loop variable, or the
    /// `(index, value)` pair an enumerated loop binds. Returns the index binding
    /// (`None` for the single-variable form) and the value binding.
    fn parse_for_head(&mut self) -> ParseResult<(Option<Identifier>, Identifier)> {
        if !self.check(&TokenKind::LeftParen) {
            return Ok((None, self.parse_loop_variable()?));
        }

        self.advance(); // consume '('
        self.skip_newlines();
        let index = self.parse_loop_variable()?;
        self.skip_newlines();
        self.consume(TokenKind::Comma, "',' between the index and value bindings")?;
        self.skip_newlines();
        let value = self.parse_loop_variable()?;
        self.skip_newlines();
        self.consume(TokenKind::RightParen, "')' to close the loop bindings")?;
        Ok((Some(index), value))
    }

    /// Bind a pair head's item whole under a parser-made name and open `body` by
    /// destructuring it, as `val (first, second) = item` would. A `_` binds nothing.
    fn bind_pair_item(
        &mut self,
        first: Identifier,
        second: Identifier,
        body: &mut Vec<Stmt>,
    ) -> Identifier {
        let item = Identifier {
            name: format!("__destructure_{}", self.next_destructure_id()),
            span: first.span.merge(second.span),
        };
        let fields = [first, second]
            .into_iter()
            .enumerate()
            .filter(|(_, name)| name.name != "_")
            .map(|(index, name)| Stmt::VarDecl {
                init: Some(Expr::TupleIndex {
                    object: Box::new(Expr::Identifier(item.clone())),
                    index,
                    span: name.span,
                }),
                span: name.span,
                name,
                ty: None,
                mutable: false,
            });
        body.splice(0..0, fields);
        item
    }

    /// Consume one identifier as a loop binding.
    fn parse_loop_variable(&mut self) -> ParseResult<Identifier> {
        let token = self.consume(TokenKind::Identifier(String::new()), "loop variable")?;
        match token.kind {
            TokenKind::Identifier(name) => Ok(Identifier {
                name,
                span: token.span,
            }),
            found => Err(ParseError::UnexpectedToken {
                found,
                expected: "loop variable".to_string(),
                span: token.span,
            }),
        }
    }

    /// Reject a head whose arity disagrees with the iterable: a pair binds what only a
    /// position-yielding head produces, and such a head produces what only a pair can
    /// bind. `pair_head` names that head (`enumerate`, `char_indices`) when the iterable
    /// wears one, so the diagnostic points at the spelling actually written.
    fn check_head_agrees(
        index: &Option<Identifier>,
        pair_head: Option<&str>,
        span: Span,
    ) -> ParseResult<()> {
        match (index.is_some(), pair_head) {
            (true, None) => Err(ParseError::PairWithoutEnumerate { span }),
            (false, Some(head)) => Err(ParseError::PairHeadWithoutPair {
                head: head.to_string(),
                span,
            }),
            _ => Ok(()),
        }
    }

    /// Parse a `break` statement after its keyword: an optional loop `label`, then an
    /// optional value-producing expression `break v`.
    ///
    /// The quote tells the two apart: `break 'outer` names a loop, and anything else
    /// begins the value. The value, like the label, must sit on the same logical line:
    /// a `break` at end of line carries neither.
    pub(super) fn parse_break_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        let label = self.parse_optional_loop_label();

        let value = if self.is_at_end()
            || matches!(
                self.peek_kind(),
                // A comma is here because a match arm body is comma-terminated; no
                // expression can begin with one, so it never hides a real value.
                Some(TokenKind::Newline | TokenKind::RightBrace | TokenKind::Comma)
            ) {
            None
        } else {
            Some(self.parse_expr(Precedence::Lowest)?)
        };

        let end_span = value
            .as_ref()
            .map(|e| e.span())
            .or_else(|| label.as_ref().map(|l| l.span))
            .map(|s| start_span.merge(s))
            .unwrap_or(start_span);

        Ok(Stmt::Break {
            label,
            value,
            span: end_span,
        })
    }

    /// Parse a trailing loop label on `break` / `continue` (`break 'outer`).
    ///
    /// The label, when present, sits on the same logical line as the keyword, so
    /// the immediately following token is inspected without skipping newlines:
    /// a `break` at the end of a line is never mistaken for a labeled break.
    pub(super) fn parse_optional_loop_label(&mut self) -> Option<Identifier> {
        let Some(TokenKind::Lifetime(name)) = self.peek_kind() else {
            return None;
        };
        let name = name.clone();
        let span = self.peek().map(|t| t.span)?;
        self.advance();
        Some(Identifier { name, span })
    }

    /// True at `ident : <for|while|loop>`, the label spelling without its quote. Nothing
    /// else opens a statement with an identifier and a single colon, so this only ever
    /// sharpens an error the statement would hit anyway.
    pub(super) fn unquoted_label_ahead(&self) -> bool {
        let kind_at = |i: usize| self.tokens.get(i).map(|t| &t.kind);
        if !matches!(kind_at(self.current + 1), Some(TokenKind::Colon)) {
            return false;
        }
        let mut keyword_index = self.current + 2;
        while matches!(kind_at(keyword_index), Some(TokenKind::Newline)) {
            keyword_index += 1;
        }
        matches!(
            kind_at(keyword_index),
            Some(TokenKind::For | TokenKind::While | TokenKind::Loop)
        )
    }

    /// Parse a labeled loop statement, `'outer: for ...`, with the cursor on the label.
    /// A label names a loop and nothing else, so anything but `for`, `while` or `loop`
    /// after its colon is an error.
    pub(super) fn parse_labeled_loop(&mut self) -> ParseResult<Stmt> {
        let label = self
            .parse_optional_loop_label()
            .ok_or_else(|| ParseError::UnexpectedEof {
                expected: "loop label".to_string(),
            })?;
        self.consume(TokenKind::Colon, "':' after a loop label")?;
        self.skip_newlines();
        let keyword_token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
            expected: LOOP_AFTER_LABEL.to_string(),
        })?;
        let start_span = keyword_token.span;
        match keyword_token.kind {
            TokenKind::For => self.parse_for_stmt(start_span, Some(label)),
            TokenKind::While => self.parse_while_stmt(start_span, Some(label)),
            TokenKind::Loop => self.parse_loop_stmt(start_span, Some(label)),
            found => Err(ParseError::UnexpectedToken {
                found,
                expected: LOOP_AFTER_LABEL.to_string(),
                span: start_span,
            }),
        }
    }
}

/// What a loop label must be followed by.
pub(super) const LOOP_AFTER_LABEL: &str = "`for`, `while` or `loop` after a loop label";

/// The position-binding adapter a `for` head recognises on its iterable.
const ENUMERATE_METHOD: &str = "enumerate";
/// The codepoint head that binds a byte offset beside each scalar.
const CHAR_INDICES_METHOD: &str = "char_indices";
/// The element-transforming adapter.
const MAP_METHOD: &str = "map";
/// The element-dropping adapter.
const FILTER_METHOD: &str = "filter";
/// The range-only adapter that walks the bounds from the top down.
const REV_METHOD: &str = "rev";
/// The range-only adapter that keeps every `n`th value of the walk.
const STEP_METHOD: &str = "step";

/// Split the adapter chain off a `for` loop's iterable: an outermost `.enumerate()`,
/// then any number of `.map(f)` / `.filter(p)` calls.
///
/// Returns the base iterable, the adapters in SOURCE order (the peel runs
/// outside-in, so the collected list is reversed), and whether the head was
/// enumerated. `.enumerate()` is recognised only at the outermost position: it yields
/// pairs, and no adapter beneath one could be given a single element. The base still
/// wears any `.step(n)` and `.rev()`, which the caller peels next: they decide which of
/// a range's own values are walked, so they sit beneath every adapter.
fn strip_adapters(iterable: Expr) -> ParseResult<(Expr, Vec<LoopAdapter>, bool)> {
    let (mut current, enumerated) = strip_enumerate(iterable)?;
    let mut adapters = Vec::new();
    loop {
        match peel_adapter(current)? {
            Peeled::Adapter(receiver, adapter) => {
                adapters.push(adapter);
                current = receiver;
            }
            Peeled::Base(expr) => {
                adapters.reverse();
                return Ok((expr, adapters, enumerated));
            }
        }
    }
}

/// Split a trailing `.rev()` off a range literal, returning the receiver and whether
/// the adapter was there.
///
/// Any other receiver keeps its call: a `Range` value has a `.rev()` method of its own,
/// and on a receiver that has none the type checker names the range spelling.
fn strip_rev(iterable: Expr) -> ParseResult<(Expr, bool)> {
    let Expr::Call {
        func, args, span, ..
    } = &iterable
    else {
        return Ok((iterable, false));
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return Ok((iterable, false));
    };
    if field.name != REV_METHOD {
        return Ok((iterable, false));
    }
    let receiver = unwrap_paren(object.as_ref().clone());
    if !matches!(receiver, Expr::Range { .. }) {
        return Ok((iterable, false));
    }
    if !args.is_empty() {
        return Err(ParseError::AdapterTakesNoArguments {
            adapter: field.name.clone(),
            span: *span,
        });
    }
    Ok((receiver, true))
}

/// Split a trailing `.step(n)` off a range literal, possibly reversed, returning the
/// receiver (still wearing any `.rev()`) and the stride. Any other receiver keeps its
/// call, for the reason `strip_rev` gives.
fn strip_step(iterable: Expr) -> ParseResult<(Expr, Option<Box<Expr>>)> {
    let Expr::Call {
        func, args, span, ..
    } = &iterable
    else {
        return Ok((iterable, None));
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return Ok((iterable, None));
    };
    if field.name != STEP_METHOD {
        return Ok((iterable, None));
    }
    let receiver = object.as_ref().clone();
    let (under_rev, _) = strip_rev(receiver.clone())?;
    if !matches!(unwrap_paren(under_rev), Expr::Range { .. }) {
        return Ok((iterable, None));
    }
    let [stride] = args.as_slice() else {
        return Err(ParseError::StepArity { span: *span });
    };
    Ok((receiver, Some(Box::new(stride.clone()))))
}

/// One step of the adapter peel: either an adapter and the receiver under it, or
/// the base iterable that wears none.
enum Peeled {
    Adapter(Expr, LoopAdapter),
    Base(Expr),
}

/// Split one trailing `.map(f)` / `.filter(p)` off `iterable`.
fn peel_adapter(iterable: Expr) -> ParseResult<Peeled> {
    let Expr::Call {
        func, args, span, ..
    } = &iterable
    else {
        return Ok(Peeled::Base(iterable));
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return Ok(Peeled::Base(iterable));
    };
    let kind = match field.name.as_str() {
        MAP_METHOD => LoopAdapterKind::Map,
        FILTER_METHOD => LoopAdapterKind::Filter,
        _ => return Ok(Peeled::Base(iterable)),
    };
    let [callee] = args.as_slice() else {
        return Err(ParseError::LoopAdapterArity {
            adapter: field.name.clone(),
            span: *span,
        });
    };
    let adapter = LoopAdapter {
        kind,
        callee: callee.clone(),
        span: *span,
    };
    Ok(Peeled::Adapter(object.as_ref().clone(), adapter))
}

/// Split a trailing `.enumerate()` off a `for` loop's iterable, returning the
/// receiver and whether the adapter was there.
///
/// Method calls parse as a call whose callee is a field access, so this matches
/// that shape rather than a dedicated node.
fn strip_enumerate(iterable: Expr) -> ParseResult<(Expr, bool)> {
    let Expr::Call {
        func, args, span, ..
    } = &iterable
    else {
        return Ok((iterable, false));
    };
    let Expr::FieldAccess { object, field, .. } = func.as_ref() else {
        return Ok((iterable, false));
    };
    if field.name != ENUMERATE_METHOD {
        return Ok((iterable, false));
    }
    if !args.is_empty() {
        return Err(ParseError::AdapterTakesNoArguments {
            adapter: field.name.clone(),
            span: *span,
        });
    }
    Ok((object.as_ref().clone(), true))
}

/// Whether `iterable` is a `.char_indices()` call, the second head form that binds a
/// pair.
///
/// Unlike `.enumerate()` the call is left in the tree: it names an intrinsic on the
/// receiver, so the passes downstream resolve it as the method it is. All that is
/// decided here is the arity of the head it may wear.
fn is_char_indices_head(iterable: &Expr) -> bool {
    let Expr::Call { func, args, .. } = iterable else {
        return false;
    };
    let Expr::FieldAccess { field, .. } = func.as_ref() else {
        return false;
    };
    field.name == CHAR_INDICES_METHOD && args.is_empty()
}

/// Peel the grouping parentheses `(0..n).enumerate()` needs, so the range inside
/// is visible to the caller.
fn unwrap_paren(expr: Expr) -> Expr {
    match expr {
        Expr::Paren(inner, _) => unwrap_paren(*inner),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use crate::errors::ParseError;
    use crate::parse;
    use ast_types::{Expr, Item, Stmt};

    /// The first statement of the first function body.
    fn first_stmt(source: &str) -> Stmt {
        let items = parse(source).expect("program should parse");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        func.body.first().cloned().expect("expected a statement")
    }

    fn parse_err(source: &str) -> ParseError {
        parse(source).expect_err("program should not parse")
    }

    #[test]
    fn enumerated_array_loop_binds_a_pair() {
        let stmt = first_stmt("func main() -> i32 { for (i, x) in xs.enumerate() { }\n 0 }");
        let Stmt::ForEach {
            index, iterator, ..
        } = stmt
        else {
            panic!("expected a for-each");
        };
        assert_eq!(index.map(|i| i.name), Some("i".to_string()));
        assert_eq!(iterator.name, "x");
    }

    /// The `.enumerate()` receiver is the iterable, not a method call left in the
    /// tree: nothing downstream resolves methods on a sequence.
    #[test]
    fn enumerated_array_loop_keeps_the_receiver_as_the_iterable() {
        let stmt = first_stmt("func main() -> i32 { for (i, x) in xs.enumerate() { }\n 0 }");
        let Stmt::ForEach { iterable, .. } = stmt else {
            panic!("expected a for-each");
        };
        assert!(matches!(iterable, ast_types::Expr::Identifier(id) if id.name == "xs"));
    }

    /// The chain is recorded in source order, so the lowering applies it left to
    /// right over the elements.
    #[test]
    fn an_adapter_chain_is_kept_in_source_order() {
        use ast_types::LoopAdapterKind;
        let stmt = first_stmt("func main() -> i32 { for v in xs.map(f).filter(p).map(g) { }\n 0 }");
        let Stmt::ForEach {
            iterable, adapters, ..
        } = stmt
        else {
            panic!("expected a for-each");
        };
        assert!(matches!(iterable, ast_types::Expr::Identifier(id) if id.name == "xs"));
        let kinds: Vec<LoopAdapterKind> = adapters.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LoopAdapterKind::Map,
                LoopAdapterKind::Filter,
                LoopAdapterKind::Map
            ]
        );
    }

    /// A range wears its adapters through the parenthesised spelling and still
    /// lowers to the counted range loop.
    #[test]
    fn an_adapted_range_stays_a_range_loop() {
        let stmt = first_stmt("func main() -> i32 { for v in (0..4).filter(p) { }\n 0 }");
        let Stmt::ForRange { adapters, .. } = stmt else {
            panic!("expected a for-range");
        };
        assert_eq!(adapters.len(), 1);
    }

    /// `.enumerate()` is the outermost adapter: it yields pairs, so nothing beneath
    /// one could be handed a single element.
    #[test]
    fn enumerate_sits_outside_the_adapter_chain() {
        let stmt =
            first_stmt("func main() -> i32 { for (i, v) in xs.filter(p).enumerate() { }\n 0 }");
        let Stmt::ForEach {
            index, adapters, ..
        } = stmt
        else {
            panic!("expected a for-each");
        };
        assert_eq!(index.map(|i| i.name), Some("i".to_string()));
        assert_eq!(adapters.len(), 1);
    }

    #[test]
    fn an_adapter_without_exactly_one_argument_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for v in xs.map() { }\n 0 }"),
            ParseError::LoopAdapterArity { .. }
        ));
        assert!(matches!(
            parse_err("func main() -> i32 { for v in xs.filter(p, q) { }\n 0 }"),
            ParseError::LoopAdapterArity { .. }
        ));
    }

    /// A parenthesised range is the only spelling `.enumerate()` accepts on a
    /// range, and it must still lower to the range loop rather than a value.
    #[test]
    fn enumerated_range_loop_stays_a_range_loop() {
        let stmt = first_stmt("func main() -> i32 { for (k, v) in (0..=4).enumerate() { }\n 0 }");
        let Stmt::ForRange {
            index,
            iterator,
            inclusive,
            ..
        } = stmt
        else {
            panic!("expected a for-range");
        };
        assert_eq!(index.map(|i| i.name), Some("k".to_string()));
        assert_eq!(iterator.name, "v");
        assert!(inclusive);
    }

    #[test]
    fn plain_loops_carry_no_index() {
        let Stmt::ForRange { index, .. } =
            first_stmt("func main() -> i32 { for i in 0..4 { }\n 0 }")
        else {
            panic!("expected a for-range");
        };
        assert!(index.is_none());

        let Stmt::ForEach { index, .. } = first_stmt("func main() -> i32 { for x in xs { }\n 0 }")
        else {
            panic!("expected a for-each");
        };
        assert!(index.is_none());
    }

    #[test]
    fn labeled_enumerated_loop_keeps_its_label() {
        let stmt =
            first_stmt("func main() -> i32 { 'outer: for (i, x) in xs.enumerate() { }\n 0 }");
        let Stmt::ForEach { label, index, .. } = stmt else {
            panic!("expected a for-each");
        };
        assert_eq!(label.map(|l| l.name), Some("outer".to_string()));
        assert!(index.is_some());
    }

    /// A pair over any other head destructures the item, whose shape the checker owns.
    #[test]
    fn pair_head_without_a_position_destructures_the_item() {
        let Stmt::ForEach {
            index,
            iterator,
            pair,
            body,
            ..
        } = first_stmt("func main() -> i32 { for (k, _) in m { }\n 0 }")
        else {
            panic!("expected a for-each");
        };
        assert!(pair);
        assert!(index.is_none());
        assert!(iterator.name.starts_with("__destructure_"));
        let [Stmt::VarDecl { name, init, .. }] = body.as_slice() else {
            panic!("expected one destructuring declaration, got {body:?}");
        };
        assert_eq!(name.name, "k");
        assert!(matches!(
            init,
            Some(ast_types::Expr::TupleIndex { index: 0, .. })
        ));
    }

    #[test]
    fn pair_head_over_a_parenthesised_range_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, x) in (0..4) { }\n 0 }"),
            ParseError::PairWithoutEnumerate { .. }
        ));
    }

    /// An unparenthesised range binds looser than the method call, so this would
    /// have enumerated the upper bound rather than the range.
    #[test]
    fn pair_head_over_a_bare_range_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, x) in 0..4 { }\n 0 }"),
            ParseError::PairWithoutEnumerate { .. }
        ));
    }

    #[test]
    fn enumerate_without_a_pair_head_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for x in xs.enumerate() { }\n 0 }"),
            ParseError::PairHeadWithoutPair { .. }
        ));
    }

    #[test]
    fn enumerate_with_arguments_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, x) in xs.enumerate(2) { }\n 0 }"),
            ParseError::AdapterTakesNoArguments { .. }
        ));
    }

    /// The second pair-yielding head. Unlike `.enumerate()` the call stays in the tree:
    /// it is an intrinsic on the receiver, resolved by the passes downstream.
    /// `.rev()` is peeled off the range the way `.enumerate()` is, so no pass
    /// downstream meets a method call on a value ranges do not have.
    #[test]
    fn a_reversed_range_stays_a_range_loop() {
        let Stmt::ForRange {
            reversed,
            inclusive,
            ..
        } = first_stmt("func main() -> i32 { for i in (0..4).rev() { }\n 0 }")
        else {
            panic!("expected a for-range");
        };
        assert!(reversed);
        assert!(!inclusive);
    }

    #[test]
    fn an_unreversed_range_is_not_marked() {
        let Stmt::ForRange { reversed, .. } =
            first_stmt("func main() -> i32 { for i in 0..4 { }\n 0 }")
        else {
            panic!("expected a for-range");
        };
        assert!(!reversed);
    }

    /// `.rev()` reorders the range's own bounds, so it sits beneath every adapter and
    /// beneath `.enumerate()`, and all three survive one head together.
    #[test]
    fn rev_composes_with_enumerate_and_adapters() {
        let stmt = first_stmt(
            "func main() -> i32 { for (i, v) in (0..9).rev().filter(p).enumerate() { }\n 0 }",
        );
        let Stmt::ForRange {
            reversed,
            index,
            adapters,
            ..
        } = stmt
        else {
            panic!("expected a for-range");
        };
        assert!(reversed);
        assert_eq!(index.map(|i| i.name), Some("i".to_string()));
        assert_eq!(adapters.len(), 1);
    }

    /// Only a range literal's `.rev()` folds into the loop. On any other receiver it is
    /// a call, which a `Range` value answers and the checker refuses elsewhere.
    #[test]
    fn rev_on_a_non_range_head_stays_a_call() {
        for source in [
            "func main() -> i32 { for x in xs.rev() { }\n 0 }",
            "func main() -> i32 { for x in (0..4).map(f).rev() { }\n 0 }",
            "func main() -> i32 { for x in (0..9).step(3).rev() { }\n 0 }",
        ] {
            let Stmt::ForEach { iterable, .. } = first_stmt(source) else {
                panic!("expected a for-each over a call: {source}");
            };
            assert!(matches!(iterable, Expr::Call { .. }), "{source}");
        }
    }

    #[test]
    fn rev_with_arguments_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for x in (0..4).rev(2) { }\n 0 }"),
            ParseError::AdapterTakesNoArguments { adapter, .. } if adapter == "rev"
        ));
    }

    /// `.step(n)` sits beneath the adapters and above `.rev()`, so both peel off the
    /// range and the loop stays a counted range loop.
    #[test]
    fn a_stepped_reversed_range_stays_a_range_loop() {
        let Stmt::ForRange {
            reversed,
            step,
            adapters,
            index,
            ..
        } = first_stmt(
            "func main() -> i32 { for (i, v) in (0..9).rev().step(3).filter(p).enumerate() { }\n 0 }",
        )
        else {
            panic!("expected a range loop");
        };
        assert!(reversed);
        assert!(matches!(step.as_deref(), Some(Expr::Literal(..))));
        assert_eq!(adapters.len(), 1);
        assert!(index.is_some());
    }

    #[test]
    fn step_on_a_non_range_head_stays_a_call() {
        for source in [
            "func main() -> i32 { for x in xs.step(2) { }\n 0 }",
            "func main() -> i32 { for x in (0..4).step(2).step(2) { }\n 0 }",
            "func main() -> i32 { for x in r.rev().step(2) { }\n 0 }",
        ] {
            let Stmt::ForEach { iterable, .. } = first_stmt(source) else {
                panic!("expected a for-each over a call: {source}");
            };
            assert!(matches!(iterable, Expr::Call { .. }), "{source}");
        }
    }

    #[test]
    fn step_takes_exactly_one_stride() {
        assert!(matches!(
            parse_err("func main() -> i32 { for x in (0..4).step() { }\n 0 }"),
            ParseError::StepArity { .. }
        ));
    }

    #[test]
    fn char_indices_binds_a_pair_and_keeps_its_call() {
        let stmt = first_stmt("func main() -> i32 { for (o, c) in s.char_indices() { }\n 0 }");
        let Stmt::ForEach {
            index,
            iterator,
            iterable,
            ..
        } = stmt
        else {
            panic!("expected a for-each");
        };
        assert_eq!(index.map(|i| i.name), Some("o".to_string()));
        assert_eq!(iterator.name, "c");
        assert!(matches!(iterable, ast_types::Expr::Call { .. }));
    }

    #[test]
    fn char_indices_without_a_pair_head_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for c in s.char_indices() { }\n 0 }"),
            ParseError::PairHeadWithoutPair { head, .. } if head == "char_indices"
        ));
    }

    #[test]
    fn char_indices_under_enumerate_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, c) in s.char_indices().enumerate() { }\n 0 }"),
            ParseError::CharIndicesHeadDecorated { .. }
        ));
    }

    #[test]
    fn an_adapter_over_char_indices_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (o, c) in s.char_indices().filter(p) { }\n 0 }"),
            ParseError::CharIndicesHeadDecorated { .. }
        ));
    }
}
