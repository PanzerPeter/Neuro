// Assignment statements: plain, compound, and the two field-assignment forms.
//
// One of the statement-shape parsers; each adds methods to the same
// `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{BinaryOp, Expr, Stmt};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
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
