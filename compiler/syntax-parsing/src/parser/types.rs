use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::Type;
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        // Fixed-size array type `[T; N]` (§3.1): element type, `;`, then a non-negative
        // integer length literal, closed by `]`.
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
                TokenKind::Integer(n) if n >= 0 => n as usize,
                other => {
                    return Err(ParseError::UnexpectedToken {
                        found: other,
                        expected: "non-negative integer array length".to_string(),
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
        // Borrow type `&T` (§2.4) / `&mut T` (§2.5). The referent is parsed
        // recursively, so the `&` distributes over whatever type follows. A `mut`
        // keyword immediately after `&` marks a mutable borrow.
        if self.check(&TokenKind::Amp) {
            let amp = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'&'".to_string(),
            })?;
            let mutable = self.check(&TokenKind::Mut);
            if mutable {
                self.advance(); // consume 'mut'
            }
            let inner = self.parse_type()?;
            let span = amp.span.merge(inner.span());
            return Ok(Type::Reference {
                inner: Box::new(inner),
                mutable,
                span,
            });
        }

        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "type".to_string(),
        })?;

        match token.kind {
            TokenKind::Identifier(name) => {
                let span = token.span;
                let ident = Identifier { name, span };
                // Generic type application `Name<T1, T2, ...>` (§3.8). Without a
                // following `<`, this is a plain named type.
                if self.check(&TokenKind::Less) {
                    let args = self.parse_optional_type_args()?;
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
}
