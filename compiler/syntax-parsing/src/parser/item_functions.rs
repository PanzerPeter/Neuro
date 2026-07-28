// Free functions and methods: signature, generic parameters, `where` clause, receiver.
//
// One of the item-kind parsers; each adds methods to the same `impl Parser` block.

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{
    Attribute, Expr, FunctionDef, GenericParam, GenericParamKind, MethodDef, Parameter, SelfParam,
};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::items::desugar_impl_trait_params;
use super::statements::stmt_span;
use super::Parser;

impl Parser {
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

        // Optional generic parameter list `<'a, T, U: Bound + Bound>`.
        let (mut generics, lifetimes) = self.parse_generic_params()?;

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

        // Optional `where` clause: trait bounds fold into `generics`, value
        // predicates are collected for per-instantiation checking.
        let where_predicates = self.parse_where_clause(&mut generics)?;
        self.skip_newlines();

        // Argument-position `impl Trait` is anonymous-generic sugar: rewrite each
        // occurrence in a parameter type into a fresh trait-bounded generic parameter, so
        // the rest of the pipeline reuses the ordinary monomorphized-generic machinery.
        // Return-position `impl Trait` is left intact for the transparent semantic
        // resolution and is therefore not visited here.
        let mut impl_counter = 0usize;
        for param in &mut params {
            param.ty = desugar_impl_trait_params(&param.ty, &mut impl_counter, &mut generics);
        }

        let body = self.parse_block()?;

        let end_span = body.last().map(stmt_span).unwrap_or(start.span);

        Ok(FunctionDef {
            name,
            generics,
            lifetimes,
            where_predicates,
            params,
            return_type,
            body,
            attributes,
            span: start.span.merge(end_span),
        })
    }

    /// Parse an optional generic parameter list `<'a, T, U: Bound + Bound, const N: u32>`.
    ///
    /// Returns two lists: the type/const parameters (which drive monomorphization) and
    /// the explicit lifetime names (`'a`), kept separate because a lifetime is a
    /// distinct namespace and does not monomorphize. Both are empty when no `<` follows.
    /// Type-parameter bounds are recorded but not enforced this phase (no trait system);
    /// lifetime parameters carry no bounds. An empty `<>` is rejected.
    pub(super) fn parse_generic_params(
        &mut self,
    ) -> ParseResult<(Vec<GenericParam>, Vec<Identifier>)> {
        if !self.check(&TokenKind::Less) {
            return Ok((Vec::new(), Vec::new()));
        }
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();

        let mut generics: Vec<GenericParam> = Vec::new();
        let mut lifetimes: Vec<Identifier> = Vec::new();
        loop {
            // A lifetime parameter `'a` is a leading-quote name lexed as a single
            // `Lifetime` token. Lifetimes are collected apart from type/const parameters.
            if let Some(TokenKind::Lifetime(lt_name)) = self.peek().map(|t| t.kind.clone()) {
                let lt_token = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "lifetime".to_string(),
                })?;
                let lt = Identifier {
                    name: lt_name,
                    span: lt_token.span,
                };
                if lifetimes.iter().any(|existing| existing.name == lt.name) {
                    return Err(ParseError::DuplicateParameter {
                        name: format!("'{}", lt.name),
                        span: lt.span,
                    });
                }
                lifetimes.push(lt);
                self.skip_newlines();
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // ','
                self.skip_newlines();
                continue;
            }

            // A const (value) parameter is introduced by the `const` keyword: `const N: u32`
            // Its declared type follows a mandatory `:`.
            let is_const = self.check(&TokenKind::Const);
            if is_const {
                self.advance(); // 'const'
                self.skip_newlines();
            }

            let name_token =
                self.consume(TokenKind::Identifier(String::new()), "type parameter name")?;
            let name = if let TokenKind::Identifier(n) = name_token.kind {
                Identifier {
                    name: n,
                    span: name_token.span,
                }
            } else {
                return Err(ParseError::UnexpectedToken {
                    found: name_token.kind,
                    expected: "type parameter name".to_string(),
                    span: name_token.span,
                });
            };

            let mut bounds: Vec<Identifier> = Vec::new();
            let mut end_span = name.span;
            let kind = if is_const {
                // `const N: T` — the declared integer type is mandatory.
                self.consume(
                    TokenKind::Colon,
                    "':' and a type after a const parameter name",
                )?;
                self.skip_newlines();
                let ty = self.parse_type()?;
                end_span = ty.span();
                GenericParamKind::Const(ty)
            } else {
                // Optional trait bounds on a type parameter: `T: A + B`. Parsed for forward
                // compatibility; the bound names are stored but not enforced until the trait
                // system lands.
                if self.check(&TokenKind::Colon) {
                    self.advance(); // ':'
                    self.skip_newlines();
                    loop {
                        let bound_token =
                            self.consume(TokenKind::Identifier(String::new()), "trait bound name")?;
                        if let TokenKind::Identifier(n) = bound_token.kind {
                            end_span = bound_token.span;
                            bounds.push(Identifier {
                                name: n,
                                span: bound_token.span,
                            });
                        }
                        if !self.check(&TokenKind::Plus) {
                            break;
                        }
                        self.advance(); // '+'
                        self.skip_newlines();
                    }
                }
                GenericParamKind::Type
            };

            for existing in &generics {
                if existing.name.name == name.name {
                    return Err(ParseError::DuplicateParameter {
                        name: name.name.clone(),
                        span: name.span,
                    });
                }
            }

            generics.push(GenericParam {
                name: name.clone(),
                kind,
                bounds,
                span: name.span.merge(end_span),
            });

            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // ','
            self.skip_newlines();
        }

        // An empty `<>` is impossible here: the first `consume` above already requires
        // a type-parameter name, so reaching this point means at least one was parsed.
        self.consume(TokenKind::Greater, "'>'")?;

        Ok((generics, lifetimes))
    }

    /// Parse an optional `where` clause, terminated by the following `{`.
    ///
    /// Each comma-separated item is either a **trait bound** (`T: A + B`, folded into the
    /// matching generic parameter's `bounds` and left unenforced this phase) or a **value
    /// predicate** — a boolean expression over const parameters (`N > 0`) returned for
    /// per-instantiation checking. Returns an empty vector when no `where` follows.
    pub(super) fn parse_where_clause(
        &mut self,
        generics: &mut [GenericParam],
    ) -> ParseResult<Vec<Expr>> {
        if !self.check(&TokenKind::Where) {
            return Ok(Vec::new());
        }
        self.advance(); // 'where'
        self.skip_newlines();

        let mut predicates: Vec<Expr> = Vec::new();
        loop {
            if self.check(&TokenKind::LeftBrace) {
                break;
            }
            if self.where_item_is_trait_bound() {
                let name_token =
                    self.consume(TokenKind::Identifier(String::new()), "type parameter name")?;
                let TokenKind::Identifier(name) = name_token.kind else {
                    unreachable!("guarded by where_item_is_trait_bound")
                };
                self.consume(TokenKind::Colon, "':'")?;
                self.skip_newlines();
                let mut bounds: Vec<Identifier> = Vec::new();
                loop {
                    let bound_token =
                        self.consume(TokenKind::Identifier(String::new()), "trait bound name")?;
                    if let TokenKind::Identifier(n) = bound_token.kind {
                        bounds.push(Identifier {
                            name: n,
                            span: bound_token.span,
                        });
                    }
                    if !self.check(&TokenKind::Plus) {
                        break;
                    }
                    self.advance(); // '+'
                    self.skip_newlines();
                }
                // Fold the bounds onto the matching type parameter; a bound naming an
                // unknown parameter is accepted and ignored (bounds are unenforced).
                if let Some(gp) = generics.iter_mut().find(|g| g.name.name == name) {
                    gp.bounds.extend(bounds);
                }
            } else {
                // A value predicate is a boolean expression over const parameters. Struct
                // literals cannot appear in a predicate, and the trailing `{` opens the
                // body/fields, so suppress struct-literal parsing while reading it.
                predicates.push(self.guarded_header(|p| p.parse_expr(Precedence::Lowest))?);
            }

            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // ','
            self.skip_newlines();
        }

        Ok(predicates)
    }

    /// Whether the upcoming `where`-clause item is a trait bound (`Ident : ...`) rather
    /// than a value predicate. True exactly when the current token is an identifier whose
    /// next non-newline token is `:`.
    pub(super) fn where_item_is_trait_bound(&self) -> bool {
        if !matches!(self.peek_kind(), Some(TokenKind::Identifier(_))) {
            return false;
        }
        let mut i = self.current + 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        matches!(self.tokens.get(i).map(|t| &t.kind), Some(TokenKind::Colon))
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
    pub(super) fn try_parse_self_param(&mut self) -> ParseResult<Option<SelfParam>> {
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
