use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Literal, Span};

use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::statements::stmt_span;
use super::Parser;

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
            // A new line beginning with `*` is a dereference statement (`*r = v` or
            // `*r`), not a continued multiplication. The no-semicolon rule only
            // continues an expression across a newline when the *previous* line ends
            // with an operator (a trailing `*`, handled without skipping here), so a
            // leading `*` must end the current expression and fall to the statement
            // parser (§2.5).
            if matches!(self.peek_kind(), Some(TokenKind::Newline))
                && matches!(self.peek_next_nonnewline_kind(), Some(TokenKind::Star))
            {
                break;
            }
            self.skip_newlines();

            if let Some(token) = self.peek() {
                let token_precedence = self.get_precedence(&token.kind);
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

    /// Parse a prefix expression (literals, identifiers, unary operators, parentheses)
    fn parse_prefix(&mut self) -> ParseResult<Expr> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "expression".to_string(),
        })?;

        match token.kind {
            TokenKind::Integer(n) => Ok(Expr::Literal(Literal::Integer(n, None), token.span)),
            TokenKind::IntegerSuffix(tok) => Ok(Expr::Literal(
                Literal::Integer(tok.value, Some(tok.suffix)),
                token.span,
            )),
            TokenKind::Float(f) => Ok(Expr::Literal(Literal::Float(f, None), token.span)),
            TokenKind::FloatSuffix(tok) => Ok(Expr::Literal(
                Literal::Float(tok.value, Some(tok.suffix)),
                token.span,
            )),
            TokenKind::String(s) => Ok(Expr::Literal(Literal::String(s), token.span)),
            TokenKind::True => Ok(Expr::Literal(Literal::Boolean(true), token.span)),
            TokenKind::False => Ok(Expr::Literal(Literal::Boolean(false), token.span)),

            // Identifiers — path expressions (`Type::member`), struct literals, or plain idents
            TokenKind::Identifier(name) => {
                let ident = Identifier {
                    name,
                    span: token.span,
                };
                if self.check(&TokenKind::ColonColon) {
                    self.advance(); // consume '::'
                    let member_token = self.consume(
                        TokenKind::Identifier(String::new()),
                        "member name after '::'",
                    )?;
                    let member = if let TokenKind::Identifier(n) = member_token.kind {
                        Identifier {
                            name: n,
                            span: member_token.span,
                        }
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            found: member_token.kind,
                            expected: "member name".to_string(),
                            span: member_token.span,
                        });
                    };
                    let span = ident.span.merge(member.span);
                    Ok(Expr::Path {
                        type_name: ident,
                        member,
                        span,
                    })
                } else if !self.no_struct_lit && self.check(&TokenKind::LeftBrace) {
                    self.parse_struct_literal(ident)
                } else {
                    Ok(Expr::Identifier(ident))
                }
            }

            // `self` keyword used as expression inside method bodies
            TokenKind::SelfLower => Ok(Expr::Identifier(Identifier {
                name: "self".to_string(),
                span: token.span,
            })),

            TokenKind::Minus => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Bang => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                    span,
                })
            }
            TokenKind::Tilde => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Unary {
                    op: UnaryOp::BitNot,
                    operand: Box::new(operand),
                    span,
                })
            }

            // Borrow `&place` (§2.4) / `&mut place` (§2.5). In prefix position `&` is
            // a borrow; as an infix operator it is bitwise-AND, handled in
            // `parse_infix`. A `mut` keyword after `&` marks a mutable borrow.
            TokenKind::Amp => {
                let mutable = self.check(&TokenKind::Mut);
                if mutable {
                    self.advance(); // consume 'mut'
                }
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Reference {
                    operand: Box::new(operand),
                    mutable,
                    span,
                })
            }

            // Dereference `*operand` (§2.5). In prefix position `*` reads through a
            // reference; as an infix operator it is multiplication, handled in
            // `parse_infix`.
            TokenKind::Star => {
                let operand = self.parse_expr(Precedence::Unary)?;
                let span = token.span.merge(operand.span());
                Ok(Expr::Deref {
                    operand: Box::new(operand),
                    span,
                })
            }

            TokenKind::LeftParen => {
                let expr = self.parse_expr(Precedence::Lowest)?;
                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = token.span.merge(close.span);
                Ok(Expr::Paren(Box::new(expr), span))
            }

            TokenKind::If => self.parse_if_expr(token.span),

            TokenKind::LeftBrace => self.parse_block_expr(token.span),

            TokenKind::Unsafe => self.parse_unsafe_expr(token.span),

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "expression".to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse an if-expression. The `if` token has already been consumed; `start_span` is its span.
    fn parse_if_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        self.no_struct_lit = true;
        let condition = self.parse_expr(Precedence::Lowest)?;
        self.no_struct_lit = false;
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
                self.no_struct_lit = true;
                let elif_cond = self.parse_expr(Precedence::Lowest)?;
                self.no_struct_lit = false;
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
    fn parse_block_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        let mut stmts = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = start_span.merge(close.span);
        Ok(Expr::Block { stmts, span })
    }

    /// Parse an unsafe block expression. The `unsafe` keyword has already been
    /// consumed; `start_span` is its span. The body is an ordinary statement
    /// block — `unsafe` is inert in Phase 1.7, so this only records the node.
    fn parse_unsafe_expr(&mut self, start_span: Span) -> ParseResult<Expr> {
        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{' after 'unsafe'")?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = start_span.merge(close.span);
        Ok(Expr::Unsafe { stmts, span })
    }

    /// Parse an infix expression (binary operators, function calls, field access, casts)
    fn parse_infix(&mut self, left: Expr) -> ParseResult<Expr> {
        let token = self.peek().ok_or(ParseError::UnexpectedEof {
            expected: "operator or '('".to_string(),
        })?;

        match &token.kind {
            TokenKind::LeftParen => {
                self.advance(); // consume '('
                let mut args = Vec::new();

                self.skip_newlines();
                if !self.check(&TokenKind::RightParen) {
                    loop {
                        args.push(self.parse_expr(Precedence::Lowest)?);
                        self.skip_newlines();

                        if !self.check(&TokenKind::Comma) {
                            break;
                        }
                        self.advance(); // consume ','
                        self.skip_newlines();
                    }
                }

                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = left.span().merge(close.span);

                Ok(Expr::Call {
                    func: Box::new(left),
                    args,
                    span,
                })
            }

            // Field access: `expr.field`
            TokenKind::Dot => {
                self.advance(); // consume '.'
                let field_token =
                    self.consume(TokenKind::Identifier(String::new()), "field name")?;
                let field = if let TokenKind::Identifier(name) = field_token.kind {
                    Identifier {
                        name,
                        span: field_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: field_token.kind,
                        expected: "field name".to_string(),
                        span: field_token.span,
                    });
                };
                let span = left.span().merge(field.span);
                Ok(Expr::FieldAccess {
                    object: Box::new(left),
                    field,
                    span,
                })
            }

            // Type casts
            TokenKind::As => {
                self.advance(); // consume 'as'
                let target_type = self.parse_type()?;
                let span = left.span().merge(target_type.span());

                Ok(Expr::Cast {
                    expr: Box::new(left),
                    target_type,
                    span,
                })
            }

            kind if self.is_binary_op(kind) => {
                let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "operator".to_string(),
                })?;
                let op = self.token_to_binary_op(&op_token)?;
                let precedence = self.get_precedence(&op_token.kind);
                // R-to-L coalescing (`??`): recurse at one-step-lower precedence so the
                // outer loop re-enters on the next `??` instead of stopping. Appendix B row 14.
                let right_prec = if matches!(op_token.kind, TokenKind::QuestionQuestion) {
                    Precedence::Lowest
                } else {
                    precedence
                };
                let right = self.parse_expr(right_prec)?;
                let span = left.span().merge(right.span());

                Ok(Expr::Binary {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                    span,
                })
            }

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "operator or '('".to_string(),
                span: token.span,
            }),
        }
    }

    /// Check if a token kind is a binary operator
    pub(super) fn is_binary_op(&self, kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::EqualEqual
                | TokenKind::NotEqual
                | TokenKind::Less
                | TokenKind::Greater
                | TokenKind::LessEqual
                | TokenKind::GreaterEqual
                | TokenKind::AmpAmp
                | TokenKind::PipePipe
                | TokenKind::Amp
                | TokenKind::Pipe
                | TokenKind::Caret
                | TokenKind::LeftShift
                | TokenKind::QuestionQuestion
        )
    }

    /// Convert a token to a binary operator
    fn token_to_binary_op(&self, token: &Token) -> ParseResult<BinaryOp> {
        match &token.kind {
            TokenKind::Plus => Ok(BinaryOp::Add),
            TokenKind::Minus => Ok(BinaryOp::Subtract),
            TokenKind::Star => Ok(BinaryOp::Multiply),
            TokenKind::Slash => Ok(BinaryOp::Divide),
            TokenKind::Percent => Ok(BinaryOp::Modulo),
            TokenKind::EqualEqual => Ok(BinaryOp::Equal),
            TokenKind::NotEqual => Ok(BinaryOp::NotEqual),
            TokenKind::Less => Ok(BinaryOp::Less),
            TokenKind::Greater => Ok(BinaryOp::Greater),
            TokenKind::LessEqual => Ok(BinaryOp::LessEqual),
            TokenKind::GreaterEqual => Ok(BinaryOp::GreaterEqual),
            TokenKind::AmpAmp => Ok(BinaryOp::And),
            TokenKind::PipePipe => Ok(BinaryOp::Or),
            TokenKind::Amp => Ok(BinaryOp::BitAnd),
            TokenKind::Pipe => Ok(BinaryOp::BitOr),
            TokenKind::Caret => Ok(BinaryOp::BitXor),
            TokenKind::LeftShift => Ok(BinaryOp::Shl),
            TokenKind::QuestionQuestion => Ok(BinaryOp::NullCoalesce),
            _ => Err(ParseError::UnexpectedToken {
                found: token.kind.clone(),
                expected: "binary operator".to_string(),
                span: token.span,
            }),
        }
    }

    /// Get the precedence of an operator token
    pub(super) fn get_precedence(&self, kind: &TokenKind) -> Precedence {
        match kind {
            TokenKind::PipePipe => Precedence::LogicalOr,
            TokenKind::AmpAmp => Precedence::LogicalAnd,
            TokenKind::Pipe => Precedence::BitwiseOr,
            TokenKind::Caret => Precedence::BitwiseXor,
            TokenKind::Amp => Precedence::BitwiseAnd,
            TokenKind::EqualEqual | TokenKind::NotEqual => Precedence::Equality,
            TokenKind::Less
            | TokenKind::Greater
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual => Precedence::Comparison,
            TokenKind::LeftShift => Precedence::Shift,
            TokenKind::QuestionQuestion => Precedence::NullCoalesce,
            TokenKind::Plus | TokenKind::Minus => Precedence::Sum,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Product,
            TokenKind::As => Precedence::Cast,
            TokenKind::LeftParen => Precedence::Call,
            TokenKind::Dot => Precedence::FieldAccess,
            _ => Precedence::Lowest,
        }
    }
}
