// `impl` blocks, trait definitions, and the type arguments both accept.
//
// One of the item-kind parsers; each adds methods to the same `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{ImplDef, Parameter, TraitDef, TraitMethod};
use crate::errors::{ParseError, ParseResult};

use super::statements::stmt_span;
use super::Parser;

impl Parser {
    /// Parse an `impl TypeName { … }` block
    pub(crate) fn parse_impl_def(&mut self) -> ParseResult<ImplDef> {
        let start = self.consume(TokenKind::Impl, "'impl'")?;
        // Optional impl-level generic parameters `impl<'a, T, U> ...`.
        let (mut generics, lifetimes) = self.parse_generic_params()?;
        self.skip_newlines();

        // The first identifier is the struct name for an inherent `impl T`, or the
        // trait name when a `for` follows it (`impl Drop for T`). Read it, then peek
        // for `for` to decide which form this is.
        let first = self.consume(TokenKind::Identifier(String::new()), "type or trait name")?;
        let first_ident = if let TokenKind::Identifier(n) = first.kind {
            Identifier {
                name: n,
                span: first.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: first.kind,
                expected: "type or trait name".to_string(),
                span: first.span,
            });
        };

        // Optional type arguments on the first name (`impl<T> Wrapper<T>` or the
        // trait side of a trait impl). Parsed to know the type constructor's args.
        let first_args = self.parse_optional_type_args()?;

        self.skip_newlines();
        let (trait_name, type_name, type_args) = if self.check(&TokenKind::For) {
            self.advance(); // consume `for`
            self.skip_newlines();
            let ty_token = self.consume(TokenKind::Identifier(String::new()), "struct name")?;
            let ty = if let TokenKind::Identifier(n) = ty_token.kind {
                Identifier {
                    name: n,
                    span: ty_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: ty_token.kind,
                    expected: "struct name".to_string(),
                    span: ty_token.span,
                });
            };
            let ty_args = self.parse_optional_type_args()?;
            (Some(first_ident), ty, ty_args)
        } else {
            (None, first_ident, first_args)
        };

        self.skip_newlines();
        // Optional impl-level `where` clause before the method block.
        let where_predicates = self.parse_where_clause(&mut generics)?;
        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut methods = Vec::new();
        let mut assoc_types = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            // An associated-type binding `type Name = Type` (e.g. `type Output =
            // Vec2` in an operator-trait impl). Distinct from a method, so handled before
            // the attribute/method path.
            if self.check(&TokenKind::Type) {
                assoc_types.push(self.parse_assoc_type_binding()?);
                self.skip_newlines();
                continue;
            }
            let attributes = self.parse_attributes()?;
            self.skip_newlines();
            methods.push(self.parse_method_def(attributes)?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(ImplDef {
            module: 0,
            trait_name,
            type_name,
            generics,
            lifetimes,
            type_args,
            where_predicates,
            assoc_types,
            methods,
            span: start.span.merge(close.span),
        })
    }

    /// Parse an optional `<T1, T2, ...>` type-argument list applied to a type name
    /// E.g. the `<T>` in `impl<T> Wrapper<T>`. Returns an empty vector when no
    /// `<` follows. Shares the delimiter grammar with [`Parser::parse_type`].
    pub(crate) fn parse_optional_type_args(&mut self) -> ParseResult<Vec<crate::ast::Type>> {
        if !self.check(&TokenKind::Less) {
            return Ok(Vec::new());
        }
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
        }
        self.consume(TokenKind::Greater, "'>' to close type arguments")?;
        Ok(args)
    }

    /// Parse an associated-type binding inside an `impl` block: `type Name = Type`
    /// Used by operator-trait impls to declare their `Output`.
    pub(super) fn parse_assoc_type_binding(
        &mut self,
    ) -> ParseResult<(Identifier, crate::ast::Type)> {
        self.consume(TokenKind::Type, "'type'")?;
        self.skip_newlines();
        let name = self.consume_identifier("associated type name")?;
        self.skip_newlines();
        self.consume(TokenKind::Equal, "'=' in associated type binding")?;
        self.skip_newlines();
        let ty = self.parse_type()?;
        Ok((name, ty))
    }

    /// Parse a `trait` declaration: `trait Name { <method signatures> }`.
    ///
    /// Each method is either **required** (signature terminated by a newline, no body)
    /// or a **default** method (signature followed by a `{ ... }` block). Traits carry
    /// no generic parameters, supertraits, or associated types this phase — those land
    /// with the operator traits and dispatch work.
    pub(crate) fn parse_trait_def(&mut self) -> ParseResult<TraitDef> {
        let start = self.consume(TokenKind::Trait, "'trait'")?;
        self.skip_newlines();
        let name = self.consume_identifier("trait name")?;
        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            methods.push(self.parse_trait_method_def()?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;
        Ok(TraitDef {
            name,
            exported: false,
            methods,
            span: start.span.merge(close.span),
        })
    }

    /// Parse one method signature inside a `trait` block.
    ///
    /// A `{` immediately after the return type opens a default-method body; otherwise the
    /// method is required and the signature ends at the newline.
    pub(super) fn parse_trait_method_def(&mut self) -> ParseResult<TraitMethod> {
        let start = self.consume(TokenKind::Func, "'func'")?;
        self.skip_newlines();
        let name = self.consume_identifier("method name")?;

        self.consume(TokenKind::LeftParen, "'('")?;
        self.skip_newlines();
        let self_param = self.try_parse_self_param()?;
        if self_param.is_some() {
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
            }
        }

        let mut params: Vec<Parameter> = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let param_start = self
                    .peek()
                    .ok_or(ParseError::UnexpectedEof {
                        expected: "parameter".to_string(),
                    })?
                    .span;
                let param_name = self.consume_identifier("parameter name")?;
                self.skip_newlines();
                self.consume(TokenKind::Colon, "':'")?;
                self.skip_newlines();
                let param_ty = self.parse_type()?;
                let param_span = param_start.merge(param_ty.span());
                params.push(Parameter {
                    name: param_name,
                    ty: param_ty,
                    span: param_span,
                });
                self.skip_newlines();
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance();
                self.skip_newlines();
            }
        }
        self.consume(TokenKind::RightParen, "')'")?;

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        // A brace on the same logical line begins a default-method body; anything else
        // (newline, next `func`, or `}`) means this is a required method with no body.
        let default_body = if matches!(self.peek_next_nonnewline_kind(), Some(TokenKind::LeftBrace))
        {
            self.skip_newlines();
            Some(self.parse_block()?)
        } else {
            None
        };

        let end_span = default_body
            .as_ref()
            .and_then(|b| b.last())
            .map(stmt_span)
            .unwrap_or(start.span);
        Ok(TraitMethod {
            name,
            self_param,
            params,
            return_type,
            default_body,
            span: start.span.merge(end_span),
        })
    }
}
