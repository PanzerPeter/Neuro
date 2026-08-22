// Struct definitions and struct literals.
//
// One of the item-kind parsers; each adds methods to the same `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{Attribute, Expr, FieldDef, FieldInit, StructDef};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
    /// Parse a struct definition: `struct Name { field: Type, ... }`,
    /// optionally preceded by `@derive(...)` attributes (already consumed by the caller).
    pub(crate) fn parse_struct_def(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> ParseResult<StructDef> {
        let start = self.consume(TokenKind::Struct, "'struct'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "struct name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        // Optional generic parameter list `<'a, T, U: Bound>`.
        let (mut generics, lifetimes) = self.parse_generic_params()?;
        self.skip_newlines();
        // Optional `where` clause before the field block.
        let where_predicates = self.parse_where_clause(&mut generics)?;
        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldDef> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // A field carries its own visibility: an exported struct may still keep an
            // internal field to itself, so the marker is per field, not per struct.
            let field_exported = self.check(&TokenKind::Export);
            if field_exported {
                self.advance();
                self.skip_newlines();
            }

            let field_name_token =
                self.consume(TokenKind::Identifier(String::new()), "field name")?;
            let field_name = if let TokenKind::Identifier(n) = field_name_token.kind {
                Identifier {
                    name: n,
                    span: field_name_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: field_name_token.kind,
                    expected: "field name".to_string(),
                    span: field_name_token.span,
                });
            };

            self.skip_newlines();
            self.consume(TokenKind::Colon, "':'")?;
            self.skip_newlines();

            let field_ty = self.parse_type()?;
            let field_span = field_name.span.merge(field_ty.span());

            fields.push(FieldDef {
                name: field_name,
                exported: field_exported,
                ty: field_ty,
                span: field_span,
            });

            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            } else {
                break;
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(StructDef {
            name,
            exported: false,
            module: 0,
            generics,
            lifetimes,
            where_predicates,
            fields,
            attributes,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a struct literal expression: `TypeName { field: expr, ... }`
    ///
    /// The `name` identifier has already been consumed by `parse_prefix`.
    pub(crate) fn parse_struct_literal(&mut self, name: Identifier) -> ParseResult<Expr> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldInit> = Vec::new();
        let mut base: Option<Box<Expr>> = None;
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // Functional-update base `..expr` terminates the field list: every
            // field not named above is sourced from it. `..` only appears as the
            // final entry, so we stop scanning fields once we see it.
            if self.check(&TokenKind::DotDot) {
                self.advance(); // consume '..'
                self.skip_newlines();
                base = Some(Box::new(self.parse_expr(Precedence::Lowest)?));
                self.skip_newlines();
                break;
            }

            let field_name_token =
                self.consume(TokenKind::Identifier(String::new()), "field name")?;
            let field_name = if let TokenKind::Identifier(n) = field_name_token.kind {
                Identifier {
                    name: n,
                    span: field_name_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: field_name_token.kind,
                    expected: "field name".to_string(),
                    span: field_name_token.span,
                });
            };

            self.skip_newlines();
            // Shorthand: `Point { x }` desugars to `Point { x: x }`. A field with
            // no `: value` binds the same-named identifier in scope.
            let value = if self.check(&TokenKind::Colon) {
                self.advance(); // consume ':'
                self.skip_newlines();
                self.parse_expr(Precedence::Lowest)?
            } else {
                Expr::Identifier(field_name.clone())
            };
            let field_span = field_name.span.merge(value.span());

            fields.push(FieldInit {
                name: field_name,
                value: Box::new(value),
                span: field_span,
            });

            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            } else {
                break;
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        let span = name.span.merge(close.span);

        Ok(Expr::StructLiteral {
            name,
            fields,
            base,
            span,
        })
    }
}
