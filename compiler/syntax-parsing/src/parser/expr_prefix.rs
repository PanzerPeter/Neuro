//! Prefix position: literals, names, paths, grouping, unary operators and every other
//! expression that can start one.

use ast_types::{Expr, UnaryOp};
use lexical_analysis::{StringValue, TokenKind};
use shared_types::{Identifier, Literal};

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;
use super::interpolation::parse_interp_string;
use super::types::TENSOR_TYPE_NAME;

impl Parser {
    /// Parse a prefix expression (literals, identifiers, unary operators, parentheses)
    pub(super) fn parse_prefix(&mut self) -> ParseResult<Expr> {
        let token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
            expected: "expression".to_string(),
        })?;

        match token.kind {
            TokenKind::Integer(n) => {
                Ok(Expr::Literal(Literal::Integer(n as i128, None), token.span))
            }
            TokenKind::IntegerSuffix(tok) => Ok(Expr::Literal(
                Literal::Integer(tok.value as i128, Some(tok.suffix)),
                token.span,
            )),
            TokenKind::Float(f) => Ok(Expr::Literal(Literal::Float(f, None), token.span)),
            TokenKind::FloatSuffix(tok) => Ok(Expr::Literal(
                Literal::Float(tok.value, Some(tok.suffix)),
                token.span,
            )),
            TokenKind::String(StringValue::Plain(s)) => {
                Ok(Expr::Literal(Literal::String(s), token.span))
            }
            TokenKind::String(StringValue::Interp(chunks)) => {
                parse_interp_string(&chunks, token.span)
            }
            TokenKind::Char(c) => Ok(Expr::Literal(Literal::Char(c), token.span)),
            TokenKind::True => Ok(Expr::Literal(Literal::Boolean(true), token.span)),
            TokenKind::False => Ok(Expr::Literal(Literal::Boolean(false), token.span)),

            // Identifiers: path expressions (`Type::member`), struct literals, or plain idents
            TokenKind::Identifier(name) => {
                let ident = Identifier {
                    name,
                    span: token.span,
                };
                // Labeled loop expression `label: loop { ... }`: a single `:`
                // (not `::`) after an identifier followed by `loop` is the only
                // expression-position use of a bare colon.
                if self.check(&TokenKind::Colon) {
                    let mut idx = self.current + 1;
                    while matches!(
                        self.tokens.get(idx).map(|t| &t.kind),
                        Some(TokenKind::Newline)
                    ) {
                        idx += 1;
                    }
                    if matches!(self.tokens.get(idx).map(|t| &t.kind), Some(TokenKind::Loop)) {
                        return self.parse_labeled_loop_expr(ident, token.span);
                    }
                }
                // `Tensor::<f32, [3, 3]>::zeros()`: the tensor constructor spelling.
                // A turbofish is otherwise the callee's own generic arguments and must
                // be followed by `(`; here it applies to the *type* that qualifies the
                // constructor, so it is followed by another `::`. `Tensor` is the only
                // name that takes this form, and the shape inside the turbofish is what
                // claims it, so a module shadowing `Tensor` is unaffected.
                if ident.name == TENSOR_TYPE_NAME
                    && self.check(&TokenKind::ColonColon)
                    && self.colon_colon_opens_turbofish()
                {
                    return self.parse_tensor_qualified_call(ident);
                }
                // `::<` is a turbofish (`f::<T>(x)`), not a path member: leave it
                // for `parse_infix` to attach to the following call. Only `::member`
                // is a path here.
                if self.check(&TokenKind::ColonColon) && !self.colon_colon_opens_turbofish() {
                    // A path may carry more than two segments once modules exist
                    // (`utils::io::read`). Everything ahead of the final segment folds into
                    // one qualifier identifier; module resolution splits it again and erases
                    // the module prefix before semantic analysis sees the name.
                    let mut qualifier = ident;
                    let mut member;
                    loop {
                        self.advance(); // consume '::'
                        let member_token = self.consume(
                            TokenKind::Identifier(String::new()),
                            "member name after '::'",
                        )?;
                        member = if let TokenKind::Identifier(n) = member_token.kind {
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
                        if !self.check(&TokenKind::ColonColon) || self.colon_colon_opens_turbofish()
                        {
                            break;
                        }
                        qualifier = Identifier {
                            name: format!("{}::{}", qualifier.name, member.name),
                            span: qualifier.span.merge(member.span),
                        };
                    }
                    let ident = qualifier;
                    // `EnumName::Variant { ... }` is a struct-variant construction
                    // The trailing brace is the only enum-construction shape
                    // distinguishable at parse time. Suppressed inside a `no_struct_lit`
                    // context (an `if`/`while` condition), exactly like a struct literal.
                    if !self.no_struct_lit && self.check(&TokenKind::LeftBrace) {
                        return self.parse_enum_struct_literal(ident, member);
                    }
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

            // Borrow `&place` / `&mut place`. In prefix position `&` is
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

            // Dereference `*operand`. In prefix position `*` reads through a
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

            // `( ... )` is either grouping or a tuple literal. A comma after
            // the first expression makes it a tuple; otherwise it is plain grouping.
            TokenKind::LeftParen => self.inside_delimiters(|p| {
                p.skip_newlines();
                let first = p.parse_expr(Precedence::Lowest)?;
                p.skip_newlines();
                if p.check(&TokenKind::Comma) {
                    let mut elements = vec![first];
                    while p.check(&TokenKind::Comma) {
                        p.advance(); // consume ','
                        p.skip_newlines();
                        // A trailing comma before `)` closes the tuple.
                        if p.check(&TokenKind::RightParen) {
                            break;
                        }
                        elements.push(p.parse_expr(Precedence::Lowest)?);
                        p.skip_newlines();
                    }
                    let close = p.consume(TokenKind::RightParen, "')' to close tuple literal")?;
                    let span = token.span.merge(close.span);
                    Ok(Expr::TupleLiteral { elements, span })
                } else {
                    let close = p.consume(TokenKind::RightParen, "')'")?;
                    let span = token.span.merge(close.span);
                    Ok(Expr::Paren(Box::new(first), span))
                }
            }),

            // Array literal `[e0, e1, ...]`. Elements parse at the lowest
            // precedence so each may be a full expression; a trailing comma is not
            // accepted (each element must be followed by `,` or the closing `]`).
            TokenKind::LeftBracket => self.inside_delimiters(|p| {
                p.skip_newlines();
                let mut elements = Vec::new();
                if !p.check(&TokenKind::RightBracket) {
                    loop {
                        elements.push(p.parse_expr(Precedence::Lowest)?);
                        p.skip_newlines();
                        if !p.check(&TokenKind::Comma) {
                            break;
                        }
                        p.advance(); // consume ','
                        p.skip_newlines();
                    }
                }
                let close = p.consume(TokenKind::RightBracket, "']' to close array literal")?;
                let span = token.span.merge(close.span);
                Ok(Expr::ArrayLiteral { elements, span })
            }),

            TokenKind::If => self.parse_if_expr(token.span),

            TokenKind::LeftBrace => self.parse_block_expr(token.span),

            TokenKind::Loop => self.parse_loop_expr(token.span),

            TokenKind::Unsafe => self.parse_unsafe_expr(token.span),

            TokenKind::Pool => self.parse_pool_expr(token.span),

            TokenKind::Match => self.parse_match_expr(token.span),

            // Closure literals: `|params| body`, `|| body`, or `move |params| body`.
            // The `|` / `||` token has already been consumed as `token`; a leading
            // `move` is consumed here and the following pipe fetched.
            TokenKind::Move => {
                let pipe = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
                    expected: "'|' or '||' after `move`".to_string(),
                })?;
                match pipe.kind {
                    TokenKind::Pipe => self.parse_closure(true, token.span, false),
                    TokenKind::PipePipe => self.parse_closure(true, token.span, true),
                    other => Err(ParseError::UnexpectedToken {
                        found: other,
                        expected: "'|' or '||' after `move`".to_string(),
                        span: pipe.span,
                    }),
                }
            }
            TokenKind::Pipe => self.parse_closure(false, token.span, false),
            TokenKind::PipePipe => self.parse_closure(false, token.span, true),

            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "expression".to_string(),
                span: token.span,
            }),
        }
    }
}
