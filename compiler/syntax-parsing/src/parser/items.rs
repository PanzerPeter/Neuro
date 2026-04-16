use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{
    Expr, FieldDef, FieldInit, FunctionDef, ImplDef, Item, MethodDef, Parameter, SelfParam,
    StructDef, Type,
};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::statements::stmt_span;
use super::Parser;

impl Parser {
    /// Parse top-level items: function, struct, or impl definitions
    pub(crate) fn parse_program(&mut self) -> ParseResult<Vec<Item>> {
        let mut items = Vec::new();

        self.skip_newlines();
        while !self.is_at_end() {
            if self.check(&TokenKind::Func) {
                let func = self.parse_function()?;
                items.push(Item::Function(func));
            } else if self.check(&TokenKind::Struct) {
                let s = self.parse_struct_def()?;
                items.push(Item::Struct(s));
            } else if self.check(&TokenKind::Impl) {
                let impl_def = self.parse_impl_def()?;
                items.push(Item::Impl(impl_def));
            } else {
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function, struct, or impl definition".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function, struct, or impl definition".to_string(),
                    span: token.span,
                });
            }
            self.skip_newlines();
        }

        Ok(items)
    }

    /// Parse a function definition
    pub(crate) fn parse_function(&mut self) -> ParseResult<FunctionDef> {
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
                let param_span = param_start.merge(match &param_ty {
                    Type::Named(ident) => ident.span,
                    Type::Tensor { span, .. } => *span,
                });

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

        let end_span = body
            .last()
            .map(stmt_span)
            .unwrap_or(start.span);

        Ok(FunctionDef {
            name,
            params,
            return_type,
            body,
            span: start.span.merge(end_span),
        })
    }

    /// Parse a struct definition: `struct Name { field: Type, ... }`
    pub(crate) fn parse_struct_def(&mut self) -> ParseResult<StructDef> {
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
            let field_span = field_name.span.merge(match &field_ty {
                Type::Named(ident) => ident.span,
                Type::Tensor { span, .. } => *span,
            });

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

            let value = self.parse_expr(Precedence::Lowest)?;
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

        Ok(Expr::StructLiteral { name, fields, span })
    }

    /// Parse an `impl TypeName { … }` block
    pub(crate) fn parse_impl_def(&mut self) -> ParseResult<ImplDef> {
        let start = self.consume(TokenKind::Impl, "'impl'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "struct name")?;
        let type_name = if let TokenKind::Identifier(n) = name_token.kind {
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

        let mut methods = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.is_at_end() {
            methods.push(self.parse_method_def()?);
            self.skip_newlines();
        }

        let close = self.consume(TokenKind::RightBrace, "'}'")?;

        Ok(ImplDef {
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
    pub(crate) fn parse_method_def(&mut self) -> ParseResult<MethodDef> {
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
                let param_span = param_start.merge(match &param_ty {
                    Type::Named(ident) => ident.span,
                    Type::Tensor { span, .. } => *span,
                });

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

        let end_span = body
            .last()
            .map(stmt_span)
            .unwrap_or(start.span);

        Ok(MethodDef {
            name,
            self_param,
            params,
            return_type,
            body,
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
