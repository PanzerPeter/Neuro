// Enum definitions, their variants, and struct-variant literals.
//
// One of the item-kind parsers; each adds methods to the same `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{EnumDef, EnumVariant, Expr, FieldDef, FieldInit, VariantPayload};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::Parser;

impl Parser {
    /// Parse an enum definition: `enum Name<T, ...> { Unit, Tuple(T, ...), Named { f: T, ... } }`.
    ///
    /// Each variant is one of three shapes — a bare tag, a parenthesised tuple of
    /// payload types, or a brace block of named fields — distinguished by the token
    /// following the variant name. The optional `<...>` list makes the enum a template
    /// monomorphized per set of type arguments.
    pub(crate) fn parse_enum_def(&mut self) -> ParseResult<EnumDef> {
        let start = self.consume(TokenKind::Enum, "'enum'")?;
        self.skip_newlines();

        let name = self.consume_identifier("enum name")?;

        self.skip_newlines();
        let (generics, lifetimes) = self.parse_generic_params()?;
        if let Some(lt) = lifetimes.first() {
            return Err(ParseError::EnumLifetimeParam {
                name: name.name.clone(),
                span: lt.span,
            });
        }

        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut variants: Vec<EnumVariant> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            variants.push(self.parse_enum_variant()?);
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
                self.skip_newlines();
            } else {
                break;
            }
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(EnumDef {
            name,
            generics,
            variants,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a single enum variant: its name plus an optional tuple `(...)` or
    /// named-field `{ ... }` payload.
    pub(super) fn parse_enum_variant(&mut self) -> ParseResult<EnumVariant> {
        let name = self.consume_identifier("variant name")?;
        let start_span = name.span;

        if self.check(&TokenKind::LeftParen) {
            self.advance(); // consume '('
            self.skip_newlines();
            let mut tys: Vec<crate::ast::Type> = Vec::new();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    tys.push(self.parse_type()?);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                    self.skip_newlines();
                }
            }
            let close = self.consume(TokenKind::RightParen, "')' to close variant payload")?;
            Ok(EnumVariant {
                name,
                payload: VariantPayload::Tuple(tys),
                span: start_span.merge(close.span),
            })
        } else if self.check(&TokenKind::LeftBrace) {
            self.advance(); // consume '{'
            self.skip_newlines();
            let mut fields: Vec<FieldDef> = Vec::new();
            while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
                let field_name = self.consume_identifier("field name")?;
                self.skip_newlines();
                self.consume(TokenKind::Colon, "':'")?;
                self.skip_newlines();
                let field_ty = self.parse_type()?;
                let field_span = field_name.span.merge(field_ty.span());
                fields.push(FieldDef {
                    name: field_name,
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
            let close = self.consume(TokenKind::RightBrace, "'}' to close variant fields")?;
            Ok(EnumVariant {
                name,
                payload: VariantPayload::Struct(fields),
                span: start_span.merge(close.span),
            })
        } else {
            Ok(EnumVariant {
                name,
                payload: VariantPayload::Unit,
                span: start_span,
            })
        }
    }

    /// Parse a struct-variant enum literal: `EnumName::Variant { field: expr, ... }`
    /// The path (`EnumName::Variant`) has already been consumed by
    /// `parse_prefix`; the cursor sits on `{`.
    pub(crate) fn parse_enum_struct_literal(
        &mut self,
        enum_name: Identifier,
        variant: Identifier,
    ) -> ParseResult<Expr> {
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldInit> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let field_name = self.consume_identifier("field name")?;
            self.skip_newlines();
            // Shorthand `Variant { x }` binds the same-named in-scope identifier.
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
        let span = enum_name.span.merge(close.span);

        Ok(Expr::EnumStructLiteral {
            enum_name,
            variant,
            fields,
            span,
        })
    }
}
