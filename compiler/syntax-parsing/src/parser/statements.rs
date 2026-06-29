use lexical_analysis::TokenKind;
use shared_types::{Identifier, Literal, Span};

use crate::ast::{BinaryOp, Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

/// A binding pattern on the left of a destructuring `val`/`mut` (§3.2). Lives only
/// during parsing — it is expanded to ordinary variable declarations and never
/// reaches the AST.
enum DestructurePattern {
    /// `_` — matches and discards the value, binding nothing.
    Wildcard,
    /// A binding name.
    Bind(Identifier),
    /// A nested tuple pattern `(a, b, ...)`.
    Tuple(Vec<DestructurePattern>),
    /// A struct pattern `Name { field, field, ... }` binding each field by its name.
    /// The type name is syntax-only — field access in the desugar resolves against
    /// the scrutinee's own type — so it is not retained.
    Struct { fields: Vec<Identifier> },
    /// An array pattern `[p0, p1, ..rest]` binding elements positionally with an
    /// optional trailing rest.
    Array(Vec<ArrayPatternElem>),
}

/// One element of an array destructuring pattern (§3.2).
enum ArrayPatternElem {
    /// A positional sub-pattern.
    Pattern(DestructurePattern),
    /// A trailing rest `..` (discarded) or `..name` (binds the remainder).
    Rest(Option<Identifier>),
}

impl Parser {
    /// Parse a const declaration statement: `const NAME: Type = expr`
    pub(crate) fn parse_const_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        let name_token = self.consume(TokenKind::Identifier(String::new()), "constant name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "constant name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();

        let ty = self.parse_type()?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = start_span.merge(value.span());

        Ok(Stmt::Const {
            name,
            ty,
            value,
            span,
        })
    }
}

impl Parser {
    /// Parse a variable declaration statement (val/mut)
    pub(crate) fn parse_var_decl(&mut self, mutable: bool, start_span: Span) -> ParseResult<Stmt> {
        let name_token = self.consume(TokenKind::Identifier(String::new()), "variable name")?;

        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "identifier".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        let ty = if self.check(&TokenKind::Colon) {
            self.advance(); // consume ':'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();
        let init = if self.check(&TokenKind::Equal) {
            self.advance(); // consume '='
            self.skip_newlines();
            Some(self.parse_expr(Precedence::Lowest)?)
        } else {
            None
        };

        let span = start_span.merge(
            init.as_ref()
                .map(|e| e.span())
                .or_else(|| ty.as_ref().map(|t| t.span()))
                .unwrap_or(name.span),
        );

        Ok(Stmt::VarDecl {
            name,
            ty,
            init,
            mutable,
            span,
        })
    }

    /// Parse a return statement
    pub(crate) fn parse_return_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        let value = if self.is_at_end()
            || matches!(
                self.peek_kind(),
                Some(TokenKind::Newline) | Some(TokenKind::RightBrace)
            ) {
            None
        } else {
            Some(self.parse_expr(Precedence::Lowest)?)
        };

        let span = value
            .as_ref()
            .map(|e| start_span.merge(e.span()))
            .unwrap_or(start_span);

        Ok(Stmt::Return { value, span })
    }

    /// Parse an assignment statement (identifier = expression)
    pub(crate) fn parse_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let target_token = self.consume(TokenKind::Identifier(String::new()), "identifier")?;
        let target = if let TokenKind::Identifier(name) = target_token.kind {
            Identifier {
                name,
                span: target_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: target_token.kind,
                expected: "identifier".to_string(),
                span: target_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = target.span.merge(value.span());

        Ok(Stmt::Assignment {
            target,
            value,
            span,
        })
    }

    /// Parse a compound assignment statement, desugaring into a plain assignment.
    /// `target OP= rhs` → `target = target OP rhs` — no new AST nodes required.
    pub(crate) fn parse_compound_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let target_token = self.consume(TokenKind::Identifier(String::new()), "identifier")?;
        let target = if let TokenKind::Identifier(name) = target_token.kind {
            Identifier {
                name,
                span: target_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: target_token.kind,
                expected: "identifier".to_string(),
                span: target_token.span,
            });
        };

        self.skip_newlines();

        let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "compound assignment operator".to_string(),
        })?;

        let binary_op = match op_token.kind {
            TokenKind::PlusEqual => BinaryOp::Add,
            TokenKind::MinusEqual => BinaryOp::Subtract,
            TokenKind::StarEqual => BinaryOp::Multiply,
            TokenKind::SlashEqual => BinaryOp::Divide,
            TokenKind::PercentEqual => BinaryOp::Modulo,
            _ => {
                return Err(ParseError::UnexpectedToken {
                    found: op_token.kind,
                    expected: "compound assignment operator".to_string(),
                    span: op_token.span,
                })
            }
        };

        self.skip_newlines();

        let rhs = self.parse_expr(Precedence::Lowest)?;

        let target_expr = Expr::Identifier(target.clone());
        let binary_span = target.span.merge(rhs.span());
        let value = Expr::Binary {
            left: Box::new(target_expr),
            op: binary_op,
            right: Box::new(rhs),
            span: binary_span,
        };
        let span = target.span.merge(value.span());

        Ok(Stmt::Assignment {
            target,
            value,
            span,
        })
    }

    /// Parse an if/else statement
    pub(crate) fn parse_if_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
        self.skip_newlines();

        self.no_struct_lit = true;
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
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

                self.no_struct_lit = true;
                let else_if_condition = self.parse_expr(Precedence::Lowest)?;
                self.no_struct_lit = false;
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

    /// Parse a while statement, optionally prefixed with a loop `label` (§3.7).
    pub(crate) fn parse_while_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        self.no_struct_lit = true;
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
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

    /// Parse a loop statement: `loop { ... }` (§3.7), optionally `label`ed.
    pub(crate) fn parse_loop_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        let body = self.parse_labeled_block(label.as_ref())?;

        let end_span = body.last().map(stmt_span).unwrap_or(start_span);

        Ok(Stmt::Loop {
            label,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a loop body, tracking `label` as in-scope for the duration so a
    /// `break label` inside is recognised as a labeled break rather than a
    /// value-carrying `break label` (§3.7). An unlabeled loop parses normally.
    fn parse_labeled_block(&mut self, label: Option<&Identifier>) -> ParseResult<Vec<Stmt>> {
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

    /// Parse a for-range statement: `for <ident> in <expr>..<expr> { ... }`,
    /// optionally prefixed with a loop `label` (§3.7).
    pub(crate) fn parse_for_stmt(
        &mut self,
        start_span: Span,
        label: Option<Identifier>,
    ) -> ParseResult<Stmt> {
        self.skip_newlines();

        let iterator_token = self.consume(TokenKind::Identifier(String::new()), "loop variable")?;
        let iterator = if let TokenKind::Identifier(name) = iterator_token.kind {
            Identifier {
                name,
                span: iterator_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: iterator_token.kind,
                expected: "loop variable".to_string(),
                span: iterator_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::In, "'in'")?;
        self.skip_newlines();

        // The iterable expression must not be a struct literal or `{` would be
        // consumed. Parse it at `Range` precedence so a `..` / `..=` separator is
        // not swallowed as a range expression (§1.6, §2.7); the operator (if any)
        // then distinguishes a numeric range from an array iterable (§3.1).
        self.no_struct_lit = true;
        let start = self.parse_expr(Precedence::Range)?;
        self.skip_newlines();

        let inclusive = if self.check(&TokenKind::DotDotEqual) {
            self.advance();
            true
        } else if self.check(&TokenKind::DotDot) {
            self.advance();
            false
        } else {
            // No range operator: iterate the parsed expression as an array (§3.1).
            self.no_struct_lit = false;
            let body = self.parse_labeled_block(label.as_ref())?;
            let end_span = body.last().map(stmt_span).unwrap_or(start.span());
            return Ok(Stmt::ForEach {
                label,
                iterator,
                iterable: start,
                body,
                span: start_span.merge(end_span),
            });
        };

        self.skip_newlines();

        let end = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
        self.skip_newlines();

        let body = self.parse_labeled_block(label.as_ref())?;

        let end_span = body.last().map(stmt_span).unwrap_or(end.span());

        Ok(Stmt::ForRange {
            label,
            iterator,
            start,
            end,
            inclusive,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a `break` statement after its keyword: an optional in-scope loop
    /// `label`, then an optional value-producing expression `break v` (§3.7).
    ///
    /// `break label` and `break value` collide syntactically (both a bare token
    /// after `break`), so a leading identifier is consumed as a label only when it
    /// names a loop currently in scope ([`Parser::active_labels`]); otherwise it
    /// begins the value expression. The value, like the label, must sit on the
    /// same logical line — a `break` at end of line carries neither.
    fn parse_break_stmt(&mut self, start_span: Span) -> ParseResult<Stmt> {
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
                Some(TokenKind::Newline) | Some(TokenKind::RightBrace)
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

    /// Parse a trailing loop label on `break` / `continue` (`break outer`, §3.7).
    ///
    /// The label, when present, sits on the same logical line as the keyword, so
    /// the immediately following token is inspected without skipping newlines —
    /// a `break` at the end of a line is never mistaken for a labeled break.
    fn parse_optional_loop_label(&mut self) -> Option<Identifier> {
        let Some(TokenKind::Identifier(name)) = self.peek_kind() else {
            return None;
        };
        let name = name.clone();
        let span = self.peek().map(|t| t.span)?;
        self.advance();
        Some(Identifier { name, span })
    }

    /// Parse a labeled loop when the statement begins with `ident : <loop-keyword>`
    /// (`outer: for ...`, §3.7). Returns `None` (consuming nothing) when the
    /// identifier does not introduce a loop label, so the caller falls through to
    /// its normal identifier-statement handling.
    fn try_parse_labeled_loop(&mut self) -> ParseResult<Option<Stmt>> {
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

    /// Parse a single statement
    pub(crate) fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        self.skip_newlines();

        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "statement".to_string(),
        })?;

        match &token.kind {
            TokenKind::Val => {
                let start_span = token.span;
                self.advance(); // consume 'val'
                self.skip_newlines();
                self.parse_var_decl(false, start_span)
            }
            TokenKind::Mut => {
                let start_span = token.span;
                self.advance(); // consume 'mut'
                self.skip_newlines();
                self.parse_var_decl(true, start_span)
            }
            TokenKind::Const => {
                let start_span = token.span;
                self.advance(); // consume 'const'
                self.skip_newlines();
                self.parse_const_stmt(start_span)
            }
            TokenKind::Return => {
                let start_span = token.span;
                self.advance(); // consume 'return'
                self.parse_return_stmt(start_span)
            }
            TokenKind::If => {
                let start_span = token.span;
                self.advance(); // consume 'if'
                self.parse_if_stmt(start_span)
            }
            TokenKind::While => {
                let start_span = token.span;
                self.advance(); // consume 'while'
                self.parse_while_stmt(start_span, None)
            }
            TokenKind::Loop => {
                let start_span = token.span;
                self.advance(); // consume 'loop'
                self.parse_loop_stmt(start_span, None)
            }
            TokenKind::For => {
                let start_span = token.span;
                self.advance(); // consume 'for'
                self.parse_for_stmt(start_span, None)
            }
            TokenKind::Break => {
                let span = token.span;
                self.advance(); // consume 'break'
                self.parse_break_stmt(span)
            }
            TokenKind::Continue => {
                let span = token.span;
                self.advance(); // consume 'continue'
                let label = self.parse_optional_loop_label();
                Ok(Stmt::Continue { label, span })
            }
            TokenKind::Identifier(_) => {
                // A loop label (`outer: for ...`, §3.7) is the only statement form
                // where an identifier is immediately followed by a colon, so it is
                // unambiguous to dispatch on `ident : <loop-keyword>` here.
                if let Some(stmt) = self.try_parse_labeled_loop()? {
                    return Ok(stmt);
                }

                // Lookahead to distinguish:
                //   ident = expr          → assignment
                //   ident OP= expr        → compound assignment (desugared)
                //   ident.field = expr    → field assignment
                //   anything else         → expression statement
                if self.current + 1 < self.tokens.len() {
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        if matches!(next_token.kind, TokenKind::Equal) {
                            return self.parse_assignment_stmt();
                        }
                        if matches!(
                            next_token.kind,
                            TokenKind::PlusEqual
                                | TokenKind::MinusEqual
                                | TokenKind::StarEqual
                                | TokenKind::SlashEqual
                                | TokenKind::PercentEqual
                        ) {
                            return self.parse_compound_assignment_stmt();
                        }
                        if matches!(next_token.kind, TokenKind::Dot) {
                            if let (Some(field_tok), Some(eq_tok)) = (
                                self.tokens.get(self.current + 2),
                                self.tokens.get(self.current + 3),
                            ) {
                                if matches!(field_tok.kind, TokenKind::Identifier(_))
                                    && matches!(eq_tok.kind, TokenKind::Equal)
                                {
                                    return self.parse_field_assignment_stmt();
                                }
                            }
                        }
                    }
                }

                let expr = self.parse_expr(Precedence::Lowest)?;
                // Array element assignment `arr[i] = v` (§3.1): the parsed expression
                // is an index whose object is a bare binding, followed by `=`.
                if let Expr::Index { object, index, .. } = &expr {
                    if matches!(object.as_ref(), Expr::Identifier(_))
                        && self.check(&TokenKind::Equal)
                    {
                        let Expr::Identifier(target) = object.as_ref().clone() else {
                            unreachable!("guarded by the matches! above")
                        };
                        let index = (**index).clone();
                        self.advance(); // consume '='
                        self.skip_newlines();
                        let value = self.parse_expr(Precedence::Lowest)?;
                        let span = target.span.merge(value.span());
                        return Ok(Stmt::IndexAssignment {
                            target,
                            index,
                            value,
                            span,
                        });
                    }
                }
                Ok(Stmt::Expr(expr))
            }
            // `self` keyword as statement — detect `self.field = expr` field assignments
            TokenKind::SelfLower => {
                if self.current + 1 < self.tokens.len() {
                    if let Some(next_token) = self.tokens.get(self.current + 1) {
                        if matches!(next_token.kind, TokenKind::Dot) {
                            if let (Some(field_tok), Some(eq_tok)) = (
                                self.tokens.get(self.current + 2),
                                self.tokens.get(self.current + 3),
                            ) {
                                if matches!(field_tok.kind, TokenKind::Identifier(_))
                                    && matches!(eq_tok.kind, TokenKind::Equal)
                                {
                                    return self.parse_self_field_assignment_stmt();
                                }
                            }
                        }
                    }
                }
                let expr = self.parse_expr(Precedence::Lowest)?;
                Ok(Stmt::Expr(expr))
            }
            // A leading `*` is a dereference: either an assignment through a
            // mutable reference (`*r = value`, §2.5) or a deref expression statement.
            TokenKind::Star => {
                let start_span = token.span;
                let expr = self.parse_expr(Precedence::Lowest)?;
                if self.check(&TokenKind::Equal) {
                    self.advance(); // consume '='
                    let value = self.parse_expr(Precedence::Lowest)?;
                    let span = start_span.merge(value.span());
                    let pointer = match expr {
                        Expr::Deref { operand, .. } => *operand,
                        // The `*` prefix always parses to a Deref, so this is unreachable
                        // in practice; fall back to the parsed expression defensively.
                        other => other,
                    };
                    return Ok(Stmt::DerefAssignment {
                        pointer,
                        value,
                        span,
                    });
                }
                Ok(Stmt::Expr(expr))
            }
            _ => {
                let expr = self.parse_expr(Precedence::Lowest)?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    /// Parse one source statement and append the resulting AST statement(s) to
    /// `out`. Most statements append exactly one node, but a tuple-destructuring
    /// bind `val (a, b) = e` (§3.2) desugars to several — a temp binding plus one
    /// projection per leaf — so it is spliced in here rather than forcing the
    /// single-`Stmt` shape of [`Parser::parse_stmt`].
    pub(crate) fn parse_stmt_into(&mut self, out: &mut Vec<Stmt>) -> ParseResult<()> {
        self.skip_newlines();
        if matches!(self.peek_kind(), Some(TokenKind::Val | TokenKind::Mut)) {
            let mutable = matches!(self.peek_kind(), Some(TokenKind::Mut));
            // A tuple `(`, array `[`, or struct `Name {` pattern after the keyword is a
            // destructuring bind (§3.2); anything else is an ordinary variable
            // declaration (`val name`, `val name: T`).
            if self.starts_destructure_pattern() {
                let kw = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "'val' or 'mut'".to_string(),
                })?;
                let start_span = kw.span;
                self.skip_newlines();
                return self.parse_destructure_bind(mutable, start_span, out);
            }
        }
        out.push(self.parse_stmt()?);
        Ok(())
    }

    /// Whether the tokens after the current `val`/`mut` keyword open a destructuring
    /// pattern: `(` (tuple), `[` (array), or `Name {` (struct). A bare name followed
    /// by `:` or `=` is an ordinary variable declaration.
    fn starts_destructure_pattern(&self) -> bool {
        let (first, second) = self.peek_two_after_keyword();
        match first {
            Some(TokenKind::LeftParen | TokenKind::LeftBracket) => true,
            Some(TokenKind::Identifier(_)) => matches!(second, Some(TokenKind::LeftBrace)),
            _ => false,
        }
    }

    /// The first two non-newline token kinds *after* the current `val`/`mut` keyword,
    /// used to detect a destructuring pattern without consuming input.
    fn peek_two_after_keyword(&self) -> (Option<TokenKind>, Option<TokenKind>) {
        let mut i = self.current + 1;
        let next_non_newline = |start: usize| {
            let mut j = start;
            while matches!(
                self.tokens.get(j).map(|t| &t.kind),
                Some(TokenKind::Newline)
            ) {
                j += 1;
            }
            j
        };
        i = next_non_newline(i);
        let first = self.tokens.get(i).map(|t| t.kind.clone());
        let j = next_non_newline(i + 1);
        let second = self.tokens.get(j).map(|t| t.kind.clone());
        (first, second)
    }

    /// Desugar a destructuring bind `val PATTERN = expr` (§3.2), where `PATTERN` is a
    /// tuple, array, or struct pattern. The cursor sits on the pattern's opening
    /// token. The right-hand side is bound once to a fresh immutable temporary, then
    /// each pattern leaf is bound to a projection of that temporary — so the only new
    /// AST node any pattern needs is the array-rest remainder ([`Expr::ArrayRest`]).
    fn parse_destructure_bind(
        &mut self,
        mutable: bool,
        start_span: Span,
        out: &mut Vec<Stmt>,
    ) -> ParseResult<()> {
        let pattern = self.parse_top_pattern()?;
        self.skip_newlines();
        self.consume(TokenKind::Equal, "'=' in destructuring binding")?;
        self.skip_newlines();
        let init = self.parse_expr(Precedence::Lowest)?;
        let init_span = init.span();

        let tmp = Identifier {
            name: format!("__destructure_{}", self.next_destructure_id()),
            span: start_span,
        };
        out.push(Stmt::VarDecl {
            name: tmp.clone(),
            ty: None,
            init: Some(init),
            mutable: false,
            span: start_span.merge(init_span),
        });
        self.expand_pattern(&pattern, Expr::Identifier(tmp), mutable, start_span, out);
        Ok(())
    }

    /// Parse the top-level destructuring pattern: a tuple `(`, array `[`, or struct
    /// `Name {`. A bare-name pattern is never a top-level destructure (it is an
    /// ordinary `val name = ...`), so it is rejected here.
    fn parse_top_pattern(&mut self) -> ParseResult<DestructurePattern> {
        match self.peek_kind() {
            Some(TokenKind::LeftParen) => self.parse_tuple_pattern(),
            Some(TokenKind::LeftBracket) => self.parse_array_pattern(),
            Some(TokenKind::Identifier(_)) => self.parse_struct_pattern(),
            _ => {
                let (found, span) = self
                    .peek()
                    .map(|t| (t.kind.clone(), t.span))
                    .unwrap_or((TokenKind::Eof, Span::new(0, 0)));
                Err(ParseError::UnexpectedToken {
                    found,
                    expected: "a tuple `(`, array `[`, or struct `Name {` destructuring pattern"
                        .to_string(),
                    span,
                })
            }
        }
    }

    /// Allocate a unique id for a destructuring temporary.
    fn next_destructure_id(&mut self) -> usize {
        let id = self.destructure_counter;
        self.destructure_counter += 1;
        id
    }

    /// Parse a parenthesized tuple pattern `(p0, p1, ...)`. Requires at least two
    /// elements — a single `(p)` is not a tuple. The cursor sits on the `(`.
    fn parse_tuple_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let open = self.consume(TokenKind::LeftParen, "'(' to open destructuring pattern")?;
        let mut subs = Vec::new();
        loop {
            self.skip_newlines();
            subs.push(self.parse_pattern_element()?);
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
            if self.check(&TokenKind::RightParen) {
                break; // trailing comma
            }
        }
        let close = self.consume(TokenKind::RightParen, "')' to close destructuring pattern")?;
        if subs.len() < 2 {
            return Err(ParseError::UnexpectedToken {
                found: TokenKind::RightParen,
                expected: "a tuple pattern with at least two elements `(a, b, ...)`".to_string(),
                span: open.span.merge(close.span),
            });
        }
        Ok(DestructurePattern::Tuple(subs))
    }

    /// Parse a struct pattern `Name { field, field, ... }` (§3.2). Each field is a
    /// shorthand binding: `Point { x, y }` binds `x` and `y` to the matching fields.
    /// The cursor sits on the type-name identifier.
    fn parse_struct_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct type name")?;
        if !matches!(name_token.kind, TokenKind::Identifier(_)) {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "struct type name".to_string(),
                span: name_token.span,
            });
        }
        self.consume(TokenKind::LeftBrace, "'{' to open struct pattern")?;
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RightBrace) {
                break;
            }
            let field = self.consume(TokenKind::Identifier(String::new()), "struct field name")?;
            let TokenKind::Identifier(field_name) = field.kind else {
                return Err(ParseError::UnexpectedToken {
                    found: field.kind,
                    expected: "struct field name".to_string(),
                    span: field.span,
                });
            };
            fields.push(Identifier {
                name: field_name,
                span: field.span,
            });
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
        }
        let close = self.consume(TokenKind::RightBrace, "'}' to close struct pattern")?;
        if fields.is_empty() {
            return Err(ParseError::UnexpectedToken {
                found: TokenKind::RightBrace,
                expected: "at least one field in a struct pattern `Name { field, ... }`"
                    .to_string(),
                span: name_token.span.merge(close.span),
            });
        }
        Ok(DestructurePattern::Struct { fields })
    }

    /// Parse an array pattern `[p0, p1, ..rest]` (§3.2). Elements bind positionally;
    /// an optional trailing rest `..` discards or `..name` captures the remainder. At
    /// most one rest is allowed and it must come last. The cursor sits on the `[`.
    fn parse_array_pattern(&mut self) -> ParseResult<DestructurePattern> {
        let open = self.consume(TokenKind::LeftBracket, "'[' to open array pattern")?;
        let mut elems = Vec::new();
        let mut seen_rest = false;
        loop {
            self.skip_newlines();
            if self.check(&TokenKind::RightBracket) {
                break;
            }
            if self.check(&TokenKind::DotDot) {
                let dotdot = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "'..' rest pattern".to_string(),
                })?;
                if seen_rest {
                    return Err(ParseError::UnexpectedToken {
                        found: TokenKind::DotDot,
                        expected: "at most one `..` rest pattern in an array pattern".to_string(),
                        span: dotdot.span,
                    });
                }
                seen_rest = true;
                // An optional name binds the remainder; bare `..` discards it.
                let name = if let Some(TokenKind::Identifier(_)) = self.peek_kind() {
                    let tok = self.advance().ok_or(ParseError::UnexpectedEof {
                        expected: "rest binding name".to_string(),
                    })?;
                    let TokenKind::Identifier(n) = tok.kind else {
                        unreachable!("peeked an identifier")
                    };
                    Some(Identifier {
                        name: n,
                        span: tok.span,
                    })
                } else {
                    None
                };
                elems.push(ArrayPatternElem::Rest(name));
            } else {
                if seen_rest {
                    return Err(ParseError::UnexpectedToken {
                        found: self
                            .peek()
                            .map(|t| t.kind.clone())
                            .unwrap_or(TokenKind::Eof),
                        expected: "no elements after a `..` rest pattern".to_string(),
                        span: self.peek().map(|t| t.span).unwrap_or(open.span),
                    });
                }
                elems.push(ArrayPatternElem::Pattern(self.parse_pattern_element()?));
            }
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
        }
        self.consume(TokenKind::RightBracket, "']' to close array pattern")?;
        Ok(DestructurePattern::Array(elems))
    }

    /// Parse one element of a destructuring pattern: a nested tuple/array/struct
    /// pattern, the `_` wildcard, or a binding name.
    fn parse_pattern_element(&mut self) -> ParseResult<DestructurePattern> {
        if self.check(&TokenKind::LeftParen) {
            return self.parse_tuple_pattern();
        }
        if self.check(&TokenKind::LeftBracket) {
            return self.parse_array_pattern();
        }
        // `Name {` opens a nested struct pattern; a bare name is a binding.
        if matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
            && matches!(self.peek_second_kind(), Some(TokenKind::LeftBrace))
        {
            return self.parse_struct_pattern();
        }
        let token = self.consume(TokenKind::Identifier(String::new()), "binding name or `_`")?;
        let TokenKind::Identifier(name) = token.kind else {
            return Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "binding name or `_`".to_string(),
                span: token.span,
            });
        };
        if name == "_" {
            Ok(DestructurePattern::Wildcard)
        } else {
            Ok(DestructurePattern::Bind(Identifier {
                name,
                span: token.span,
            }))
        }
    }

    /// The kind of the token immediately after the current one, skipping newlines.
    fn peek_second_kind(&self) -> Option<TokenKind> {
        let mut i = self.current + 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        self.tokens.get(i).map(|t| t.kind.clone())
    }

    /// Emit the variable declarations a destructuring pattern expands to. `access` is
    /// the expression that reaches the value matched by `pattern` (the temporary for
    /// the whole tuple, or a nested `.N` projection). A wildcard binds nothing.
    fn expand_pattern(
        &mut self,
        pattern: &DestructurePattern,
        access: Expr,
        mutable: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) {
        match pattern {
            DestructurePattern::Wildcard => {}
            DestructurePattern::Bind(name) => out.push(Stmt::VarDecl {
                name: name.clone(),
                ty: None,
                init: Some(access),
                mutable,
                span,
            }),
            DestructurePattern::Tuple(subs) => {
                for (i, sub) in subs.iter().enumerate() {
                    let elem = Expr::TupleIndex {
                        object: Box::new(access.clone()),
                        index: i,
                        span,
                    };
                    self.expand_pattern(sub, elem, mutable, span, out);
                }
            }
            DestructurePattern::Struct { fields } => {
                for field in fields {
                    let access_field = Expr::FieldAccess {
                        object: Box::new(access.clone()),
                        field: field.clone(),
                        span,
                    };
                    out.push(Stmt::VarDecl {
                        name: field.clone(),
                        ty: None,
                        init: Some(access_field),
                        mutable,
                        span,
                    });
                }
            }
            DestructurePattern::Array(elems) => {
                // Count the leading positional patterns (everything before the rest).
                let lead = elems
                    .iter()
                    .take_while(|e| matches!(e, ArrayPatternElem::Pattern(_)))
                    .count();
                for (i, elem) in elems.iter().enumerate() {
                    match elem {
                        ArrayPatternElem::Pattern(sub) => {
                            let index = Expr::Literal(Literal::Integer(i as i64, None), span);
                            let access_i = Expr::Index {
                                object: Box::new(access.clone()),
                                index: Box::new(index),
                                span,
                            };
                            self.expand_pattern(sub, access_i, mutable, span, out);
                        }
                        ArrayPatternElem::Rest(name) => {
                            let rest = Expr::ArrayRest {
                                array: Box::new(access.clone()),
                                start: lead,
                                exact: false,
                                span,
                            };
                            match name {
                                // A named rest binds the remainder; a bare `..` keeps
                                // the node only as a `start <= N` bounds assertion.
                                Some(n) => out.push(Stmt::VarDecl {
                                    name: n.clone(),
                                    ty: None,
                                    init: Some(rest),
                                    mutable,
                                    span,
                                }),
                                None => out.push(Stmt::Expr(rest)),
                            }
                        }
                    }
                }
                // With no rest, the pattern must match the array length exactly; emit a
                // discarded `ArrayRest` whose `exact` flag carries that arity check.
                if !elems.iter().any(|e| matches!(e, ArrayPatternElem::Rest(_))) {
                    out.push(Stmt::Expr(Expr::ArrayRest {
                        array: Box::new(access),
                        start: lead,
                        exact: true,
                        span,
                    }));
                }
            }
        }
    }

    /// Parse a block of statements (within braces)
    pub(crate) fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut statements = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            self.parse_stmt_into(&mut statements)?;
            self.skip_newlines();
        }

        self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(statements)
    }

    /// Parse a field assignment statement: `object.field = value`
    pub(crate) fn parse_field_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let object_token = self.consume(TokenKind::Identifier(String::new()), "variable name")?;
        let object = if let TokenKind::Identifier(n) = object_token.kind {
            Identifier {
                name: n,
                span: object_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: object_token.kind,
                expected: "variable name".to_string(),
                span: object_token.span,
            });
        };

        self.consume(TokenKind::Dot, "'.'")?;

        let field_token = self.consume(TokenKind::Identifier(String::new()), "field name")?;
        let field = if let TokenKind::Identifier(n) = field_token.kind {
            Identifier {
                name: n,
                span: field_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: field_token.kind,
                expected: "field name".to_string(),
                span: field_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = object.span.merge(value.span());

        Ok(Stmt::FieldAssignment {
            object,
            field,
            value,
            span,
        })
    }

    /// Parse `self.field = value` inside a method body.
    pub(crate) fn parse_self_field_assignment_stmt(&mut self) -> ParseResult<Stmt> {
        let self_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "self".to_string(),
        })?;
        let object = Identifier {
            name: "self".to_string(),
            span: self_token.span,
        };

        self.consume(TokenKind::Dot, "'.'")?;

        let field_token = self.consume(TokenKind::Identifier(String::new()), "field name")?;
        let field = if let TokenKind::Identifier(n) = field_token.kind {
            Identifier {
                name: n,
                span: field_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: field_token.kind,
                expected: "field name".to_string(),
                span: field_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = object.span.merge(value.span());

        Ok(Stmt::FieldAssignment {
            object,
            field,
            value,
            span,
        })
    }
}

/// Extract the span from a statement — shared by span-calculation in block-ending logic.
pub(crate) fn stmt_span(stmt: &Stmt) -> shared_types::Span {
    match stmt {
        Stmt::VarDecl { span, .. } => *span,
        Stmt::Const { span, .. } => *span,
        Stmt::Assignment { span, .. } => *span,
        Stmt::Return { span, .. } => *span,
        Stmt::If { span, .. } => *span,
        Stmt::While { span, .. } => *span,
        Stmt::Loop { span, .. } => *span,
        Stmt::ForRange { span, .. } => *span,
        Stmt::ForEach { span, .. } => *span,
        Stmt::Break { span, .. } => *span,
        Stmt::Continue { span, .. } => *span,
        Stmt::FieldAssignment { span, .. } => *span,
        Stmt::IndexAssignment { span, .. } => *span,
        Stmt::DerefAssignment { span, .. } => *span,
        Stmt::Expr(e) => e.span(),
    }
}
