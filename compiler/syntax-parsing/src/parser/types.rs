use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{ArraySize, GenericArg, Type};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        // Fixed-size array type `[T; N]` (§3.1, §3.8): element type, `;`, then either a
        // non-negative integer length literal or a `const` generic parameter name
        // (`[T; CAP]`), closed by `]`.
        if self.check(&TokenKind::LeftBracket) {
            let open = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'['".to_string(),
            })?;
            let element = self.parse_type()?;
            self.consume(TokenKind::Semicolon, "';' in array type `[T; N]`")?;
            let size_token = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "array length".to_string(),
            })?;
            let size = match size_token.kind {
                TokenKind::Integer(n) if n >= 0 => ArraySize::Literal(n as u64),
                TokenKind::Identifier(name) => ArraySize::Const(Identifier {
                    name,
                    span: size_token.span,
                }),
                other => {
                    return Err(ParseError::UnexpectedToken {
                        found: other,
                        expected: "non-negative integer array length or const parameter name"
                            .to_string(),
                        span: size_token.span,
                    })
                }
            };
            let close = self.consume(TokenKind::RightBracket, "']' to close array type")?;
            let span = open.span.merge(close.span);
            return Ok(Type::Array {
                element: Box::new(element),
                size,
                span,
            });
        }
        // Tuple type `(T1, T2, ...)` (§3.2): two or more element types separated by
        // commas. A single parenthesized type is grouping, not a tuple, and the empty
        // `()` unit type is not yet produced — both are rejected here.
        if self.check(&TokenKind::LeftParen) {
            let open = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'('".to_string(),
            })?;
            let mut elements = Vec::new();
            loop {
                self.skip_newlines();
                elements.push(self.parse_type()?);
                self.skip_newlines();
                if !self.check(&TokenKind::Comma) {
                    break;
                }
                self.advance(); // consume ','
            }
            let close = self.consume(TokenKind::RightParen, "')' to close tuple type")?;
            if elements.len() < 2 {
                return Err(ParseError::UnexpectedToken {
                    found: TokenKind::RightParen,
                    expected: "a tuple type with at least two elements `(T1, T2, ...)`".to_string(),
                    span: close.span,
                });
            }
            let span = open.span.merge(close.span);
            return Ok(Type::Tuple { elements, span });
        }
        // Borrow type `&T` (§2.4) / `&mut T` (§2.5), with an optional explicit lifetime
        // `&'a T` / `&'a mut T` (§2.6). The referent is parsed recursively, so the `&`
        // distributes over whatever type follows. Order after `&`: an optional lifetime,
        // then an optional `mut` keyword marking a mutable borrow.
        if self.check(&TokenKind::Amp) {
            let amp = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'&'".to_string(),
            })?;
            let lifetime =
                if let Some(TokenKind::Lifetime(name)) = self.peek().map(|t| t.kind.clone()) {
                    let lt_token = self.advance().ok_or(ParseError::UnexpectedEof {
                        expected: "lifetime".to_string(),
                    })?;
                    Some(Identifier {
                        name,
                        span: lt_token.span,
                    })
                } else {
                    None
                };
            let mutable = self.check(&TokenKind::Mut);
            if mutable {
                self.advance(); // consume 'mut'
            }
            let inner = self.parse_type()?;
            let span = amp.span.merge(inner.span());
            return Ok(Type::Reference {
                inner: Box::new(inner),
                mutable,
                lifetime,
                span,
            });
        }

        // Static-dispatch bound `impl Trait` (§3.17): the `impl` keyword followed by a
        // trait name. In argument position `parse_function` later rewrites it into a
        // trait-bounded generic parameter; in return position it survives to semantic.
        if self.check(&TokenKind::Impl) {
            let kw = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'impl'".to_string(),
            })?;
            let trait_name = self.parse_trait_ref_name("trait name after `impl`")?;
            let span = kw.span.merge(trait_name.span);
            return Ok(Type::ImplTrait { trait_name, span });
        }
        // Dynamic-dispatch trait object `dyn Trait` (§3.17): the `dyn` keyword followed
        // by a trait name. Valid only behind a reference; semantic rejects a bare `dyn`.
        if self.check(&TokenKind::Dyn) {
            let kw = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'dyn'".to_string(),
            })?;
            let trait_name = self.parse_trait_ref_name("trait name after `dyn`")?;
            let span = kw.span.merge(trait_name.span);
            return Ok(Type::DynTrait { trait_name, span });
        }

        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "type".to_string(),
        })?;

        match token.kind {
            TokenKind::Identifier(name) => {
                let span = token.span;
                let ident = Identifier { name, span };
                // Generic type application `Name<T1, T2, ...>` (§3.8). Without a
                // following `<`, this is a plain named type. Arguments may be types or
                // const (integer) values, as in `Ring<i32, 4>`.
                if self.check(&TokenKind::Less) {
                    let args = self.parse_generic_type_args()?;
                    let end = args.last().map(|a| a.span()).unwrap_or(span);
                    return Ok(Type::Generic {
                        name: ident,
                        args,
                        span: span.merge(end),
                    });
                }
                Ok(Type::Named(ident))
            }
            _ => Err(ParseError::UnexpectedToken {
                found: token.kind,
                expected: "type name".to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse the trait-name identifier following an `impl` / `dyn` keyword (§3.17).
    fn parse_trait_ref_name(&mut self, context: &str) -> ParseResult<Identifier> {
        let token = self
            .consume(TokenKind::Identifier(String::new()), context)
            .map_err(|_| ParseError::UnexpectedEof {
                expected: context.to_string(),
            })?;
        match token.kind {
            TokenKind::Identifier(name) => Ok(Identifier {
                name,
                span: token.span,
            }),
            other => Err(ParseError::UnexpectedToken {
                found: other,
                expected: context.to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse a `<T1, N, ...>` generic-argument list in a type application (§3.8). Each
    /// argument is a type or a non-negative integer const value (`Ring<i32, 4>`).
    fn parse_generic_type_args(&mut self) -> ParseResult<Vec<GenericArg>> {
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();
        let mut args = Vec::new();
        loop {
            if let Some(TokenKind::Integer(n)) = self.peek_kind() {
                let value = *n;
                let span = self
                    .advance()
                    .map(|t| t.span)
                    .ok_or(ParseError::UnexpectedEof {
                        expected: "const argument".to_string(),
                    })?;
                if value < 0 {
                    return Err(ParseError::UnexpectedToken {
                        found: TokenKind::Integer(value),
                        expected: "a non-negative const argument".to_string(),
                        span,
                    });
                }
                args.push(GenericArg::Const {
                    value: value as i128,
                    span,
                });
            } else {
                args.push(GenericArg::Type(self.parse_type()?));
            }
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
}
