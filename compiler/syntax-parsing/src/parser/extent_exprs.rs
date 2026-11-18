// Constant-expression tensor extents: `[B, L, H, D / H]`, `[N + M, D]`, `[S.count()]`.
//
// A shape entry is not an ordinary expression. Its grammar is closed over integer literals,
// names, a shape parameter's `.count()` and the five integer operators, so any other call,
// a field read or a comparison in a shape is a parse error rather than something the
// checker has to refuse later, and a shape stays readable as a type.

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Literal};

use crate::errors::{ParseError, ParseResult};
use ast_types::{BinaryOp, Expr};

use super::Parser;

/// What an extent operand may be, for the error that finds something else.
const EXTENT_OPERAND: &str = "an extent: a non-negative integer, a name, or `(`";

impl Parser {
    /// `product (('+' | '-') product)*`
    pub(super) fn parse_extent_sum(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_extent_product()?;
        while let Some(op) = self.peek_kind().and_then(|kind| match kind {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Subtract),
            _ => None,
        }) {
            self.advance();
            let right = self.parse_extent_product()?;
            left = binary(left, op, right);
        }
        Ok(left)
    }

    /// `operand (('*' | '/' | '%') operand)*`
    fn parse_extent_product(&mut self) -> ParseResult<Expr> {
        let mut left = self.parse_extent_operand()?;
        while let Some(op) = self.peek_kind().and_then(|kind| match kind {
            TokenKind::Star => Some(BinaryOp::Multiply),
            TokenKind::Slash => Some(BinaryOp::Divide),
            TokenKind::Percent => Some(BinaryOp::Modulo),
            _ => None,
        }) {
            self.advance();
            let right = self.parse_extent_operand()?;
            left = binary(left, op, right);
        }
        Ok(left)
    }

    /// An integer literal, a name, or a parenthesized sum.
    fn parse_extent_operand(&mut self) -> ParseResult<Expr> {
        let token = self.advance().ok_or_else(|| ParseError::UnexpectedEof {
            expected: EXTENT_OPERAND.to_string(),
        })?;
        match token.kind {
            TokenKind::Integer(value) => Ok(Expr::Literal(
                Literal::Integer(i128::from(value), None),
                token.span,
            )),
            TokenKind::Identifier(name) => {
                let ident = Identifier {
                    name,
                    span: token.span,
                };
                if self.check(&TokenKind::Dot) {
                    return self.parse_shape_count(ident);
                }
                Ok(Expr::Identifier(ident))
            }
            TokenKind::LeftParen => {
                let inner = self.parse_extent_sum()?;
                let close = self.consume(TokenKind::RightParen, "')' to close an extent")?;
                Ok(Expr::Paren(Box::new(inner), token.span.merge(close.span)))
            }
            found => Err(ParseError::UnexpectedToken {
                found,
                expected: EXTENT_OPERAND.to_string(),
                span: token.span,
            }),
        }
    }

    /// `S.count()`, a shape parameter's element count: the extent a flattened `[..S]`
    /// has. It is the one call a shape admits, built as the body would parse it.
    fn parse_shape_count(&mut self, shape: Identifier) -> ParseResult<Expr> {
        self.advance(); // consume '.'
        let method = self.consume(TokenKind::Identifier(String::new()), SHAPE_COUNT_FORM)?;
        if !matches!(&method.kind, TokenKind::Identifier(name) if name == SHAPE_COUNT_METHOD) {
            return Err(ParseError::UnexpectedToken {
                found: method.kind,
                expected: SHAPE_COUNT_FORM.to_string(),
                span: method.span,
            });
        }
        self.consume(TokenKind::LeftParen, SHAPE_COUNT_FORM)?;
        let close = self.consume(TokenKind::RightParen, SHAPE_COUNT_FORM)?;
        let span = shape.span.merge(close.span);
        let field = Identifier {
            name: SHAPE_COUNT_METHOD.to_string(),
            span: method.span,
        };
        Ok(Expr::Call {
            func: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Identifier(shape)),
                field,
                span,
            }),
            type_args: Vec::new(),
            args: Vec::new(),
            arg_labels: Vec::new(),
            span,
        })
    }
}

/// The only method an extent may call, on a shape parameter.
const SHAPE_COUNT_METHOD: &str = "count";
const SHAPE_COUNT_FORM: &str = "`.count()` after a shape parameter in an extent";

fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    let span = left.span().merge(right.span());
    Expr::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
        span,
    }
}
