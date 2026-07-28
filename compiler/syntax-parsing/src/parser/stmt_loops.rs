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

    /// Parse a for-range statement: `for <ident> in <expr>..<expr> { ... }`,
    /// optionally prefixed with a loop `label`.
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
            // No range operator: iterate the parsed expression as an array.
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

        let end = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
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
