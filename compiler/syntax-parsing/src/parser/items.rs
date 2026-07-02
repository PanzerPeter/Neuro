use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{
    Attribute, ConstDef, EnumDef, EnumVariant, Expr, FieldDef, FieldInit, FunctionDef, ImplDef,
    Item, MethodDef, NewtypeDef, Parameter, SelfParam, StructDef, VariantPayload,
};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::statements::stmt_span;
use super::type_aliases::{expand_type_aliases, TypeAliasDecl};
use super::Parser;

impl Parser {
    /// Parse top-level items: function, struct, impl, const, or type-alias definitions.
    ///
    /// Type aliases (§3.14) are transparent and are resolved here: each declaration
    /// is collected, then every aliased type annotation in the remaining items is
    /// rewritten to its target type before the program is returned. No alias item
    /// reaches semantic analysis or codegen.
    pub(crate) fn parse_program(&mut self) -> ParseResult<Vec<Item>> {
        let mut items = Vec::new();
        let mut alias_decls: Vec<TypeAliasDecl> = Vec::new();

        self.skip_newlines();
        while !self.is_at_end() {
            let attributes = self.parse_attributes()?;
            self.skip_newlines();

            if self.check(&TokenKind::Func) {
                let func = self.parse_function(attributes)?;
                items.push(Item::Function(func));
            } else if self.check(&TokenKind::Struct) {
                let s = self.parse_struct_def(attributes)?;
                items.push(Item::Struct(s));
            } else if !attributes.is_empty() {
                // Attributes attach only to functions and structs today; rejecting here
                // gives an actionable diagnostic instead of silently dropping them.
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function or struct definition after attribute".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function or struct definition after attribute".to_string(),
                    span: token.span,
                });
            } else if self.check(&TokenKind::Enum) {
                let e = self.parse_enum_def()?;
                items.push(Item::Enum(e));
            } else if self.check(&TokenKind::Impl) {
                let impl_def = self.parse_impl_def()?;
                items.push(Item::Impl(impl_def));
            } else if self.check(&TokenKind::Const) {
                let c = self.parse_const_def()?;
                items.push(Item::Const(c));
            } else if self.check(&TokenKind::Type) {
                alias_decls.push(self.parse_type_alias()?);
            } else if self.check(&TokenKind::Newtype) {
                let nt = self.parse_newtype_def()?;
                items.push(Item::Newtype(nt));
            } else {
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function, struct, enum, impl, const, type, or newtype definition"
                        .to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function, struct, enum, impl, const, type, or newtype definition"
                        .to_string(),
                    span: token.span,
                });
            }
            self.skip_newlines();
        }

        expand_type_aliases(&mut items, alias_decls)?;
        Ok(items)
    }

    /// Parse zero or more `@name` / `@name(arg, ...)` attributes attached to the
    /// following item. Stops at the first token that is not `@`.
    pub(crate) fn parse_attributes(&mut self) -> ParseResult<Vec<Attribute>> {
        let mut attributes = Vec::new();
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::At) {
                break;
            }
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }

    /// Parse a single `@name` or `@name(arg, ...)` attribute. Assumes the
    /// current token is `@`.
    fn parse_attribute(&mut self) -> ParseResult<Attribute> {
        let at = self.consume(TokenKind::At, "'@'")?;

        let name_token = self.consume(TokenKind::Identifier(String::new()), "attribute name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "attribute name".to_string(),
                span: name_token.span,
            });
        };

        let mut args: Vec<Identifier> = Vec::new();
        let mut end_span = name.span;

        if self.check(&TokenKind::LeftParen) {
            self.advance(); // consume '('
            self.skip_newlines();

            if !self.check(&TokenKind::RightParen) {
                loop {
                    let arg_token =
                        self.consume(TokenKind::Identifier(String::new()), "attribute argument")?;
                    let arg = if let TokenKind::Identifier(n) = arg_token.kind {
                        Identifier {
                            name: n,
                            span: arg_token.span,
                        }
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            found: arg_token.kind,
                            expected: "attribute argument".to_string(),
                            span: arg_token.span,
                        });
                    };
                    args.push(arg);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                    self.skip_newlines();
                }
            }

            let close = self.consume(TokenKind::RightParen, "')'")?;
            end_span = close.span;
        }

        Ok(Attribute {
            name,
            args,
            span: at.span.merge(end_span),
        })
    }

    /// Parse a module-level constant: `const NAME: Type = expr`
    pub(crate) fn parse_const_def(&mut self) -> ParseResult<ConstDef> {
        let start = self.consume(TokenKind::Const, "'const'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "constant name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "constant name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();

        let ty = self.parse_type()?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = start.span.merge(value.span());

        Ok(ConstDef {
            name,
            ty,
            value,
            span,
        })
    }

    /// Parse a newtype declaration: `newtype Name = InnerType` (§3.15).
    ///
    /// Unlike a `type` alias, a newtype is a distinct nominal type, so it is kept as
    /// an `Item::Newtype` for semantic analysis rather than expanded at parse time.
    pub(crate) fn parse_newtype_def(&mut self) -> ParseResult<NewtypeDef> {
        let start = self.consume(TokenKind::Newtype, "'newtype'")?;
        self.skip_newlines();

        let name = self.consume_identifier("newtype name")?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let inner = self.parse_type()?;
        let span = start.span.merge(inner.span());

        Ok(NewtypeDef { name, inner, span })
    }

    /// Parse a function definition
    pub(crate) fn parse_function(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> ParseResult<FunctionDef> {
        let start = self.consume(TokenKind::Func, "'func'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "function name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "function name".to_string(),
                span: name_token.span,
            });
        };

        self.consume(TokenKind::LeftParen, "'('")?;
        self.skip_newlines();

        let mut params: Vec<Parameter> = Vec::new();
        if !self.check(&TokenKind::RightParen) {
            loop {
                let param_start = self
                    .peek()
                    .ok_or(ParseError::UnexpectedEof {
                        expected: "parameter".to_string(),
                    })?
                    .span;

                let param_name_token =
                    self.consume(TokenKind::Identifier(String::new()), "parameter name")?;
                let param_name = if let TokenKind::Identifier(n) = param_name_token.kind {
                    Identifier {
                        name: n,
                        span: param_name_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: param_name_token.kind,
                        expected: "parameter name".to_string(),
                        span: param_name_token.span,
                    });
                };

                self.skip_newlines();
                self.consume(TokenKind::Colon, "':'")?;
                self.skip_newlines();

                let param_ty = self.parse_type()?;
                let param_span = param_start.merge(param_ty.span());

                for existing_param in &params {
                    if existing_param.name.name == param_name.name {
                        return Err(ParseError::DuplicateParameter {
                            name: param_name.name.clone(),
                            span: param_name.span,
                        });
                    }
                }

                params.push(Parameter {
                    name: param_name,
                    ty: param_ty,
                    span: param_span,
                });

                self.skip_newlines();
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // consume ','
                self.skip_newlines();
            }
        }

        self.consume(TokenKind::RightParen, "')'")?;
        self.skip_newlines();

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance(); // consume '->'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();

        let body = self.parse_block()?;

        let end_span = body.last().map(stmt_span).unwrap_or(start.span);

        Ok(FunctionDef {
            name,
            params,
            return_type,
            body,
            attributes,
            span: start.span.merge(end_span),
        })
    }

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
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut fields: Vec<FieldDef> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
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

    /// Parse an enum definition: `enum Name { Unit, Tuple(T, ...), Named { f: T, ... } }` (§3.5).
    ///
    /// Each variant is one of three shapes — a bare tag, a parenthesised tuple of
    /// payload types, or a brace block of named fields — distinguished by the token
    /// following the variant name.
    pub(crate) fn parse_enum_def(&mut self) -> ParseResult<EnumDef> {
        let start = self.consume(TokenKind::Enum, "'enum'")?;
        self.skip_newlines();

        let name = self.consume_identifier("enum name")?;

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
            variants,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a single enum variant: its name plus an optional tuple `(...)` or
    /// named-field `{ ... }` payload.
    fn parse_enum_variant(&mut self) -> ParseResult<EnumVariant> {
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
    /// (§3.5). The path (`EnumName::Variant`) has already been consumed by
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

    /// Parse an `impl TypeName { … }` block
    pub(crate) fn parse_impl_def(&mut self) -> ParseResult<ImplDef> {
        let start = self.consume(TokenKind::Impl, "'impl'")?;
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

        self.skip_newlines();
        let (trait_name, type_name) = if self.check(&TokenKind::For) {
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
            (Some(first_ident), ty)
        } else {
            (None, first_ident)
        };

        self.skip_newlines();
        self.consume(TokenKind::LeftBrace, "'{'")?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            let attributes = self.parse_attributes()?;
            self.skip_newlines();
            methods.push(self.parse_method_def(attributes)?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(ImplDef {
            trait_name,
            type_name,
            methods,
            span: start.span.merge(close.span),
        })
    }

    /// Parse a single method definition inside an `impl` block.
    ///
    /// Handles three self-parameter forms:
    ///   `&self`     — immutable borrow (SelfParam::Ref)
    ///   `&mut self` — mutable borrow   (SelfParam::RefMut)
    ///   `self`      — owned/consuming  (SelfParam::Owned)
    ///
    /// Associated functions have no self parameter and use the same syntax as
    /// free functions. The distinction is detected by checking the first parameter.
    pub(crate) fn parse_method_def(
        &mut self,
        attributes: Vec<Attribute>,
    ) -> ParseResult<MethodDef> {
        let start = self.consume(TokenKind::Func, "'func'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "method name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "method name".to_string(),
                span: name_token.span,
            });
        };

        self.consume(TokenKind::LeftParen, "'('")?;
        self.skip_newlines();

        let self_param = self.try_parse_self_param()?;

        // If there was a self param and more params follow, consume the comma separator.
        if self_param.is_some() {
            self.skip_newlines();
            if self.check(&TokenKind::Comma) {
                self.advance(); // consume ','
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

                let param_name_token =
                    self.consume(TokenKind::Identifier(String::new()), "parameter name")?;
                let param_name = if let TokenKind::Identifier(n) = param_name_token.kind {
                    Identifier {
                        name: n,
                        span: param_name_token.span,
                    }
                } else {
                    return Err(ParseError::UnexpectedToken {
                        found: param_name_token.kind,
                        expected: "parameter name".to_string(),
                        span: param_name_token.span,
                    });
                };

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
                self.advance(); // consume ','
                self.skip_newlines();
            }
        }

        self.consume(TokenKind::RightParen, "')'")?;
        self.skip_newlines();

        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance(); // consume '->'
            self.skip_newlines();
            Some(self.parse_type()?)
        } else {
            None
        };

        self.skip_newlines();
        let body = self.parse_block()?;

        let end_span = body.last().map(stmt_span).unwrap_or(start.span);

        Ok(MethodDef {
            name,
            self_param,
            params,
            return_type,
            body,
            attributes,
            span: start.span.merge(end_span),
        })
    }

    /// Attempt to parse a self parameter (`self`, `&self`, `&mut self`) at the
    /// current token position. Returns `None` without consuming tokens if no
    /// self parameter is present.
    fn try_parse_self_param(&mut self) -> ParseResult<Option<SelfParam>> {
        match self.peek_kind() {
            Some(TokenKind::SelfLower) => {
                self.advance(); // consume 'self'
                Ok(Some(SelfParam::Owned))
            }
            // `&self` or `&mut self`
            Some(TokenKind::Amp) => {
                // Peek ahead to confirm this is a self/mut-self param, not a
                // regular reference type (reference types are not yet in the grammar,
                // but we guard against future ambiguity).
                let next = self.tokens.get(self.current + 1).map(|t| &t.kind);
                match next {
                    Some(TokenKind::SelfLower) => {
                        self.advance(); // consume '&'
                        self.advance(); // consume 'self'
                        Ok(Some(SelfParam::Ref))
                    }
                    Some(TokenKind::Mut) => {
                        let after_mut = self.tokens.get(self.current + 2).map(|t| &t.kind);
                        if matches!(after_mut, Some(TokenKind::SelfLower)) {
                            self.advance(); // consume '&'
                            self.advance(); // consume 'mut'
                            self.advance(); // consume 'self'
                            Ok(Some(SelfParam::RefMut))
                        } else {
                            Ok(None)
                        }
                    }
                    _ => Ok(None),
                }
            }
            _ => Ok(None),
        }
    }
}
