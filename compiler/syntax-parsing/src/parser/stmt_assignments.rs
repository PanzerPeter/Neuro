// Assignment statements: the place on the left, and the operator that joins it to
// the value on the right.
//
// One of the statement-shape parsers; each adds methods to the same
// `impl Parser` block.

use lexical_analysis::TokenKind;

use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;
use ast_types::{BinaryOp, Expr, Place, Stmt};

use super::Parser;

impl Parser {
    /// Finish a statement whose left-hand side has already been parsed as an
    /// expression, when the next token is `=` or a compound assignment operator.
    ///
    /// Reading the target as an ordinary expression first is what lets every place
    /// form share one path: the shapes that can be written to are a subset of the
    /// shapes that can be read, so the classification happens once the expression is
    /// in hand rather than as lookahead over raw tokens.
    pub(crate) fn parse_assign_tail(&mut self, target: Expr) -> ParseResult<Stmt> {
        let op_token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "assignment operator".to_string(),
        })?;
        let op = match op_token.kind {
            TokenKind::Equal => None,
            TokenKind::PlusEqual => Some(BinaryOp::Add),
            TokenKind::MinusEqual => Some(BinaryOp::Subtract),
            TokenKind::StarEqual => Some(BinaryOp::Multiply),
            TokenKind::SlashEqual => Some(BinaryOp::Divide),
            TokenKind::PercentEqual => Some(BinaryOp::Modulo),
            found => {
                return Err(ParseError::UnexpectedToken {
                    found,
                    expected: "assignment operator".to_string(),
                    span: op_token.span,
                })
            }
        };

        let spelling = match op {
            None => "=".to_string(),
            Some(binary_op) => format!("{}=", binary_op),
        };
        let place = place_from_expr(target, &spelling)?;

        self.skip_newlines();
        let value = self.parse_expr(Precedence::Lowest)?;
        let span = place.span().merge(value.span());

        Ok(Stmt::Assign {
            place,
            op,
            value,
            span,
        })
    }

    /// Whether the token at the cursor opens an assignment tail.
    pub(crate) fn at_assignment_operator(&self) -> bool {
        matches!(
            self.peek_kind(),
            Some(
                TokenKind::Equal
                    | TokenKind::PlusEqual
                    | TokenKind::MinusEqual
                    | TokenKind::StarEqual
                    | TokenKind::SlashEqual
                    | TokenKind::PercentEqual
            )
        )
    }
}

/// Classify a parsed expression as the place it writes to.
///
/// `op` names the operator in the diagnostic so `arr[i] += 5` and `arr[i] = 5`
/// report against what was actually written.
fn place_from_expr(expr: Expr, op: &str) -> ParseResult<Place> {
    let span = expr.span();
    match expr {
        Expr::Identifier(ident) => Ok(Place::Var(ident)),
        Expr::Paren(inner, _) => place_from_expr(*inner, op),
        Expr::FieldAccess {
            object,
            field,
            span,
        } => Ok(Place::Field {
            object,
            field,
            span,
        }),
        Expr::Index {
            object,
            index,
            span,
        } => Ok(Place::Index {
            object,
            index,
            span,
        }),
        Expr::TensorIndex {
            object,
            indices,
            span,
        } => Ok(Place::TensorIndex {
            object,
            indices,
            span,
        }),
        Expr::Deref { operand, span } => Ok(Place::Deref {
            pointer: operand,
            span,
        }),
        _ => Err(ParseError::NotAPlace {
            op: op.to_string(),
            span,
        }),
    }
}
