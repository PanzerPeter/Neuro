use super::expr_infix::{apply_compose, as_compose};
use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;
use ast_types::{ClosureParam, Expr, Stmt};

use super::Parser;
use super::statements::stmt_span;

/// A parsed call argument list: the argument expressions, plus the call-site names of
/// any named arguments, empty when the call named none.
type CallArguments = (Vec<Expr>, Vec<Option<Identifier>>);

/// Maximum expression nesting depth to prevent stack overflow
const MAX_EXPR_DEPTH: usize = 256;

impl Parser {
    /// Parse an expression with the given precedence
    pub fn parse_expr(&mut self, precedence: Precedence) -> ParseResult<Expr> {
        if self.expr_depth >= MAX_EXPR_DEPTH {
            return Err(ParseError::MaxDepthExceeded(MAX_EXPR_DEPTH));
        }

        self.expr_depth += 1;
        let result = self.parse_expr_inner(precedence);
        self.expr_depth -= 1;

        result
    }

    /// Inner expression parsing implementation
    fn parse_expr_inner(&mut self, precedence: Precedence) -> ParseResult<Expr> {
        self.skip_newlines();

        let mut left = self.parse_prefix()?;

        while !self.is_at_end() {
            // A new line beginning with a token that can also BEGIN an expression
            // starts a statement: a dereference (`*r = v`), a negation (`-x`), a borrow
            // (`&x`), a closure literal (`|x| ...`), a parenthesized expression, or an
            // array literal, not a continuation of this one. The no-semicolon rule only
            // continues an expression across a newline when the *previous* line ends
            // with an operator, a comma, or an opening delimiter, and every one of those
            // reaches here with the newline already behind it. Skipping the newline
            // first instead let the NEXT line decide: `val a = f()` followed by a line
            // `(2 + 3)` parsed as a call of `f()`'s result, and a tail `-x` as a
            // subtraction from the line above. `@` is in the set because a line opening
            // with it is an attribute on the item below. Inside `(` or `[` a newline
            // ends nothing, so there the next line always continues.
            if self.delimiter_depth == 0
                && matches!(self.peek_kind(), Some(TokenKind::Newline))
                && matches!(
                    self.peek_next_nonnewline_kind(),
                    Some(
                        TokenKind::Star
                            | TokenKind::Minus
                            | TokenKind::Amp
                            | TokenKind::Pipe
                            | TokenKind::LeftParen
                            | TokenKind::LeftBracket
                            | TokenKind::At
                    )
                )
            {
                break;
            }
            self.skip_newlines();

            if let Some(token) = self.peek() {
                let token_precedence = self.infix_precedence(&token.kind);
                if precedence >= token_precedence {
                    break;
                }

                left = self.parse_infix(left)?;
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse a closure literal after its opening pipe token has been consumed.
    ///
    /// `is_move` records a leading `move` keyword. `start_span` is the span of the
    /// opening token (`move` or the pipe). `empty_params` is true when the opener was
    /// `||`: the zero-parameter form, which has already consumed both pipes; otherwise
    /// a closing `|` is parsed after the comma-separated parameter list.
    pub(super) fn parse_closure(
        &mut self,
        is_move: bool,
        start_span: Span,
        empty_params: bool,
    ) -> ParseResult<Expr> {
        let mut params = Vec::new();
        if !empty_params {
            self.skip_newlines();
            if !self.check(&TokenKind::Pipe) {
                loop {
                    self.skip_newlines();
                    let name_tok =
                        self.consume(TokenKind::Identifier(String::new()), "closure parameter")?;
                    let name = match name_tok.kind {
                        TokenKind::Identifier(n) => Identifier {
                            name: n,
                            span: name_tok.span,
                        },
                        other => {
                            return Err(ParseError::UnexpectedToken {
                                found: other,
                                expected: "closure parameter name".to_string(),
                                span: name_tok.span,
                            });
                        }
                    };
                    let ty = if self.check(&TokenKind::Colon) {
                        self.advance(); // consume ':'
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    let span = name
                        .span
                        .merge(ty.as_ref().map(|t| t.span()).unwrap_or(name.span));
                    params.push(ClosureParam { name, ty, span });
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                }
            }
            self.consume(TokenKind::Pipe, "'|' to close closure parameters")?;
        }

        // Optional explicit return type `-> R`.
        let ret = if self.check(&TokenKind::Arrow) {
            self.advance(); // consume '->'
            Some(self.parse_type()?)
        } else {
            None
        };

        // Body: a brace block, or (when no return type is annotated) a single
        // expression. The single-expression form binds the whole remaining
        // expression, so it stops naturally at a `,`, `)`, or newline.
        let body = if self.check(&TokenKind::LeftBrace) {
            let brace = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                expected: "'{'".to_string(),
            })?;
            self.parse_block_expr(brace.span)?
        } else {
            self.parse_expr(Precedence::Lowest)?
        };

        let span = start_span.merge(body.span());
        Ok(Expr::Closure {
            params,
            ret,
            body: Box::new(body),
            is_move,
            span,
        })
    }

    /// Parse an if-expression. The `if` token has already been consumed; `start_span` is its span.
    pub(super) fn parse_if_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let condition = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
        self.skip_newlines();

        let then_block = self.parse_block()?;
        self.skip_newlines();

        let mut else_if_blocks: Vec<(Expr, Vec<Stmt>)> = Vec::new();
        let mut else_block: Option<Vec<Stmt>> = None;

        while self.check(&TokenKind::Else) {
            self.advance(); // consume 'else'
            self.skip_newlines();

            if self.check(&TokenKind::If) {
                self.advance(); // consume 'if'
                self.skip_newlines();
                let elif_cond = self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?;
                self.skip_newlines();
                let elif_block = self.parse_block()?;
                else_if_blocks.push((elif_cond, elif_block));
                self.skip_newlines();
            } else {
                else_block = Some(self.parse_block()?);
                break;
            }
        }

        let end_span = else_block
            .as_ref()
            .and_then(|s| s.last())
            .or_else(|| else_if_blocks.last().and_then(|(_, s)| s.last()))
            .or_else(|| then_block.last())
            .map(stmt_span)
            .unwrap_or(start_span);

        Ok(Expr::If {
            condition: Box::new(condition),
            then_block,
            else_if_blocks,
            else_block,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a block expression. The `{` has already been consumed; `start_span` is its span.
    pub(super) fn parse_block_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let mut stmts = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            self.parse_stmt_into(&mut stmts)?;
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = start_span.merge(close.span);
        Ok(Expr::Block { stmts, span })
    }

    /// Parse a loop expression in value position: `loop { ... break v }`.
    /// The `loop` keyword has already been consumed; `start_span` is its span. The
    /// loop evaluates to its value-carrying `break`s; an unlabeled form is used in
    /// expression position (labels are a statement-loop concern).
    pub(super) fn parse_loop_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let body = self.parse_block()?;
        let end_span = body.last().map(stmt_span).unwrap_or(start_span);
        Ok(Expr::Loop {
            label: None,
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a labeled loop expression `label: loop { ... }`. `label` is the
    /// already-parsed identifier; the cursor sits on the `:`. The label is tracked
    /// in scope for the body so a nested `break label v` resolves to it rather than
    /// being read as a value-carrying `break label`.
    pub(super) fn parse_labeled_loop_expr(
        &mut self,
        label: Identifier,
        start_span: Span,
    ) -> ParseResult<Expr> {
        self.advance(); // consume ':'
        self.skip_newlines();
        self.consume(TokenKind::Loop, "'loop'")?;
        self.skip_newlines();

        self.active_labels.push(label.name.clone());
        let body = self.parse_block();
        self.active_labels.pop();
        let body = body?;

        let end_span = body.last().map(stmt_span).unwrap_or(start_span);
        Ok(Expr::Loop {
            label: Some(label),
            body,
            span: start_span.merge(end_span),
        })
    }

    /// Parse an unsafe block expression. The `unsafe` keyword has already been
    /// consumed; `start_span` is its span. The body is an ordinary statement
    /// block: `unsafe` is inert in Phase 1.7, so this only records the node.
    pub(super) fn parse_unsafe_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{' after 'unsafe'")?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            self.parse_stmt_into(&mut stmts)?;
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = start_span.merge(close.span);
        Ok(Expr::Unsafe { stmts, span })
    }

    /// Parse `pool { ... }` or `pool label { ... }`. The `pool` keyword is already
    /// consumed.
    ///
    /// The label sits after the keyword rather than before it, as loop labels do:
    /// a loop label is a jump target that `break` names, so it is introduced where a
    /// jump can see it, while a pool label is only ever quoted back in a diagnostic.
    pub(super) fn parse_pool_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let label = match self.peek().map(|t| &t.kind) {
            Some(TokenKind::Identifier(name)) => {
                let name = name.clone();
                let span = self.advance().map(|t| t.span).unwrap_or(start_span);
                self.skip_newlines();
                Some(Identifier { name, span })
            }
            _ => None,
        };
        self.consume(TokenKind::LeftBrace, "'{' after 'pool'")?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            self.parse_stmt_into(&mut stmts)?;
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = start_span.merge(close.span);
        Ok(Expr::Pool { label, stmts, span })
    }

    /// Parse a comma-separated argument list, stopping at the closing `)` (which the
    /// caller consumes). The opening `(` is already consumed. Arguments sit inside a
    /// delimiter pair, so a struct literal is unambiguous here even when the call
    /// appears in a guarded header (`if f(Point { x: 1 }) { ... }`).
    pub(super) fn parse_call_arguments(&mut self) -> ParseResult<CallArguments> {
        self.inside_delimiters(|p| {
            let mut args = Vec::new();
            let mut labels: Vec<Option<Identifier>> = Vec::new();
            p.skip_newlines();
            if !p.check(&TokenKind::RightParen) {
                loop {
                    let (label, value) = p.parse_call_argument()?;
                    labels.push(label);
                    args.push(value);
                    p.skip_newlines();
                    if !p.check(&TokenKind::Comma) {
                        break;
                    }
                    p.advance(); // consume ','
                    p.skip_newlines();
                }
            }
            // A call that named nothing carries no label list at all, so the overwhelmingly
            // common case costs neither an allocation nor a downstream special case.
            if labels.iter().all(Option::is_none) {
                labels.clear();
            }
            Ok((args, labels))
        })
    }

    /// Parse one call argument: `expr`, or `label: expr` for a named argument.
    ///
    /// An identifier immediately followed by `:` can only be a label here: a bare `:`
    /// is not an expression operator in any other argument position, and a qualified
    /// path uses `::`, a single token.
    fn parse_call_argument(&mut self) -> ParseResult<(Option<Identifier>, Expr)> {
        let named = matches!(self.peek_kind(), Some(TokenKind::Identifier(_)))
            && matches!(
                self.tokens.get(self.current + 1).map(|t| &t.kind),
                Some(TokenKind::Colon)
            );
        if !named {
            return Ok((None, self.parse_expr(Precedence::Lowest)?));
        }
        let label = self.consume_identifier("argument label")?;
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();
        Ok((Some(label), self.parse_expr(Precedence::Lowest)?))
    }
}

/// Build the call expression for `callee(args)`.
///
/// A composition called where it is written, `(f >> g)(x)`, is the nested call it
/// stands for: no function value need exist for a chain that is never bound. Every
/// other callee keeps its `Expr::Call`.
pub(super) fn finish_call(
    callee: Expr,
    mut args: Vec<Expr>,
    arg_labels: Vec<Option<Identifier>>,
    span: Span,
) -> Expr {
    let applied = arg_labels.is_empty() && args.len() == 1;
    match as_compose(&callee) {
        Some(functions) if applied => {
            let functions = functions.to_vec();
            apply_compose(&functions, args.remove(0), span)
        }
        _ => Expr::Call {
            func: Box::new(callee),
            type_args: Vec::new(),
            args,
            arg_labels,
            span,
        },
    }
}
