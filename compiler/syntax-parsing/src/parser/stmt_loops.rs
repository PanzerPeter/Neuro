// Control-flow statements: `if`, `while`, `loop`, `for`, and `break`, with the
// loop labels each may carry.
//
// One of the statement-shape parsers; each adds methods to the same
// `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::statements::stmt_span;
use super::Parser;

impl Parser {
    /// Parse an if/else statement
    pub(crate) fn parse_if_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        let condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        let then_block = self.parse_block()?;
        self.skip_newlines();

        let mut else_if_blocks = Vec::new();
        let mut else_block = None;

        while self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();

            if self.check(&TokenKind::If) {
                self.advance(); // consume 'if'
                self.skip_newlines();

                let else_if_condition =
                    self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
                self.skip_newlines();

                let else_if_block = self.parse_block()?;
                else_if_blocks.push((else_if_condition, else_if_block));
                self.skip_newlines();
            } else {
                else_block = Some(self.parse_block()?);
                break;
            }
        }

        let end_span = else_block
            .as_ref()
            .and_then(|stmts| stmts.last())
            .or_else(|| else_if_blocks.last().and_then(|(_, stmts)| stmts.last()))
            .or_else(|| then_block.last())
            .map(stmt_span)
            .unwrap_or(start_span);

        Ok(Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a while statement, optionally prefixed with a loop `label`.
    pub(crate) fn parse_while_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        let condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        let body = self.parse_labeled_block(label.as_ref())?;

        let end_span = body.last().map(stmt_span).unwrap_or(condition.span());

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

        let body = self.parse_labeled_block(label.as_ref())?;

        let end_span = body.last().map(stmt_span).unwrap_or(start_span);

        Ok(Stmt::Expr(Expr::Loop {
            label,
            body,
            span: start_span.merge(end_span),
        }))
    }

    /// Parse a loop body, tracking `label` as in-scope for the duration so a
    /// `break label` inside is recognised as a labeled break rather than a
    /// value-carrying `break label`. An unlabeled loop parses normally.
    pub(super) fn parse_labeled_block(
        &mut self,
        label: Option<&Identifier>,
    ) -> ParseResult<Vec<Stmt>> {
        match label {
            Some(label) => {
                self.active_labels.push(label.name.clone());
                let body = self.parse_block();
                self.active_labels.pop();
                body
            }
            None => self.parse_block(),
        }
    }

    /// Parse a for statement: `for <head> in <iterable> { ... }`, optionally
    /// prefixed with a loop `label`.
    ///
    /// `<head>` is one loop variable, or the pair `(index, value)` that
    /// `.enumerate()` binds. `<iterable>` is a numeric range, an expression
    /// yielding a sequence, or either of those with `.enumerate()` applied — the
    /// adapter is recognised here rather than type-checked as a method because a
    /// range is not a first-class value, so `(0..n).enumerate()` has no receiver
    /// to resolve a method against.
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
            // No range operator follows, so the whole iterable is already parsed —
            // either a sequence expression or a parenthesised range, each possibly
            // wearing `.enumerate()`.
            let (iterable, enumerated) = strip_enumerate(start)?;
            Self::check_head_agrees(&index, enumerated, iterable.span())?;
            let body = self.parse_labeled_block(label.as_ref())?;
            let end_span = body.last().map(stmt_span).unwrap_or(iterable.span());
            let span = start_span.merge(end_span);

            // A parenthesised range is the only way to write `(0..n).enumerate()`,
            // so unwrap it here rather than leaving a range value no other pass
            // knows how to consume.
            return Ok(match unwrap_paren(iterable) {
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
                    body,
                    span,
                },
                iterable => Stmt::ForEach {
                    label,
                    index,
                    iterator,
                    iterable,
                    body,
                    span,
                },
            });
        };

        self.skip_newlines();

        let end = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        // An unparenthesised range binds looser than a method call, so
        // `for i in 0..n.enumerate()` would enumerate `n`, not the range. Reject the
        // pair head here and point at the spelling that works.
        Self::check_head_agrees(&index, false, end.span())?;

        let body = self.parse_labeled_block(label.as_ref())?;

        let end_span = body.last().map(stmt_span).unwrap_or(end.span());

        Ok(Stmt::ForRange {
            label,
            index: None,
            iterator,
            start,
            end,
            inclusive,
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

    /// Reject a head whose arity disagrees with the iterable: a pair binds what
    /// only `.enumerate()` produces, and `.enumerate()` produces what only a pair
    /// can bind.
    fn check_head_agrees(
        index: &Option<Identifier>,
        enumerated: bool,
        span: Span,
    ) -> ParseResult<()> {
        match (index.is_some(), enumerated) {
            (true, false) => Err(ParseError::PairWithoutEnumerate { span }),
            (false, true) => Err(ParseError::EnumerateWithoutPair { span }),
            _ => Ok(()),
        }
    }

    /// Parse a `break` statement after its keyword: an optional in-scope loop
    /// `label`, then an optional value-producing expression `break v`.
    ///
    /// `break label` and `break value` collide syntactically (both a bare token
    /// after `break`), so a leading identifier is consumed as a label only when it
    /// names a loop currently in scope ([`Parser::active_labels`]); otherwise it
    /// begins the value expression. The value, like the label, must sit on the
    /// same logical line — a `break` at end of line carries neither.
    pub(super) fn parse_break_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        let label = match self.peek_kind() {
            Some(TokenKind::Identifier(name)) if self.active_labels.iter().any(|l| l == name) => {
                let name = name.clone();
                let label_span = self.peek().map(|t| t.span).unwrap_or(start_span);
                self.advance();
                Some(Identifier {
                    name,
                    span: label_span,
                })
            }
            _ => None,
        };

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

    /// Parse a trailing loop label on `break` / `continue` (`break outer`).
    ///
    /// The label, when present, sits on the same logical line as the keyword, so
    /// the immediately following token is inspected without skipping newlines —
    /// a `break` at the end of a line is never mistaken for a labeled break.
    pub(super) fn parse_optional_loop_label(&mut self) -> Option<Identifier> {
        let Some(TokenKind::Identifier(name)) = self.peek_kind() else {
            return None;
        };
        let name = name.clone();
        let span = self.peek().map(|t| t.span)?;
        self.advance();
        Some(Identifier { name, span })
    }

    /// Parse a labeled loop when the statement begins with `ident : <loop-keyword>`
    /// (`outer: for ...`). Returns `None` (consuming nothing) when the
    /// identifier does not introduce a loop label, so the caller falls through to
    /// its normal identifier-statement handling.
    pub(super) fn try_parse_labeled_loop(&mut self) -> ParseResult<Option<Stmt>> {
        if !matches!(
            self.tokens.get(self.current + 1).map(|t| &t.kind),
            Some(TokenKind::Colon)
        ) {
            return Ok(None);
        }

        // The token after the colon (skipping newlines) must be a loop keyword.
        let mut keyword_index = self.current + 2;
        while matches!(
            self.tokens.get(keyword_index).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            keyword_index += 1;
        }
        let keyword = match self.tokens.get(keyword_index).map(|t| &t.kind) {
            Some(TokenKind::For) | Some(TokenKind::While) | Some(TokenKind::Loop) => {
                self.tokens[keyword_index].kind.clone()
            }
            _ => return Ok(None),
        };

        let label_token = self.consume(TokenKind::Identifier(String::new()), "loop label")?;
        let label = match label_token.kind {
            TokenKind::Identifier(name) => Identifier {
                name,
                span: label_token.span,
            },
            other => {
                return Err(ParseError::UnexpectedToken {
                    found: other,
                    expected: "loop label".to_string(),
                    span: label_token.span,
                })
            }
        };
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();

        let keyword_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "loop keyword".to_string(),
        })?;
        let start_span = keyword_token.span;

        let stmt = match keyword {
            TokenKind::For => self.parse_for_stmt(start_span, Some(label))?,
            TokenKind::While => self.parse_while_stmt(start_span, Some(label))?,
            TokenKind::Loop => self.parse_loop_stmt(start_span, Some(label))?,
            _ => unreachable!("keyword guarded above"),
        };
        Ok(Some(stmt))
    }
}

/// The adapter name a `for` head recognises on its iterable.
const ENUMERATE_METHOD: &str = "enumerate";

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
        return Err(ParseError::EnumerateTakesNoArguments { span: *span });
    }
    Ok((object.as_ref().clone(), true))
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
    use crate::ast::{Item, Stmt};
    use crate::errors::ParseError;
    use crate::parse;

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
    /// tree — nothing downstream resolves methods on a sequence.
    #[test]
    fn enumerated_array_loop_keeps_the_receiver_as_the_iterable() {
        let stmt = first_stmt("func main() -> i32 { for (i, x) in xs.enumerate() { }\n 0 }");
        let Stmt::ForEach { iterable, .. } = stmt else {
            panic!("expected a for-each");
        };
        assert!(matches!(iterable, crate::ast::Expr::Identifier(id) if id.name == "xs"));
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
        let stmt = first_stmt("func main() -> i32 { outer: for (i, x) in xs.enumerate() { }\n 0 }");
        let Stmt::ForEach { label, index, .. } = stmt else {
            panic!("expected a for-each");
        };
        assert_eq!(label.map(|l| l.name), Some("outer".to_string()));
        assert!(index.is_some());
    }

    #[test]
    fn pair_head_without_enumerate_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, x) in xs { }\n 0 }"),
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
            ParseError::EnumerateWithoutPair { .. }
        ));
    }

    #[test]
    fn enumerate_with_arguments_is_rejected() {
        assert!(matches!(
            parse_err("func main() -> i32 { for (i, x) in xs.enumerate(2) { }\n 0 }"),
            ParseError::EnumerateTakesNoArguments { .. }
        ));
    }
}
