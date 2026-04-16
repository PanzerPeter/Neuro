use lexical_analysis::{Token, TokenKind};
use shared_types::{Identifier, Literal};

use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

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
            TokenKind::Integer(n) => Ok(Expr::Literal(Literal::Integer(n), token.span)),
            TokenKind::Float(f) => Ok(Expr::Literal(Literal::Float(f), token.span)),
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

            TokenKind::LeftParen => {
                let expr = self.parse_expr(Precedence::Lowest)?;
                let close = self.consume(TokenKind::RightParen, "')'")?;
                let span = token.span.merge(close.span);
                Ok(Expr::Paren(Box::new(expr), span))
            }

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "expression".to_string(),
                span: token.span,
            }),
        }
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
                let right = self.parse_expr(precedence)?;
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
            TokenKind::EqualEqual | TokenKind::NotEqual => Precedence::Equality,
            TokenKind::Less
            | TokenKind::Greater
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual => Precedence::Comparison,
            TokenKind::Plus | TokenKind::Minus => Precedence::Sum,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Precedence::Product,
            TokenKind::As => Precedence::Cast,
            TokenKind::LeftParen => Precedence::Call,
            TokenKind::Dot => Precedence::FieldAccess,
            _ => Precedence::Lowest,
        }
    }
}
