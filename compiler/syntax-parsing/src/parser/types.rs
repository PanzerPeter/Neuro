use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{ArraySize, GenericArg, Type};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

impl Parser {
    /// Parse a type annotation
    pub(crate) fn parse_type(&mut self) -> ParseResult<Type> {
        // Bracketed sequence type: `[T; N]` is a fixed-size array, `[T]` an unsized
        // slice. They share a prefix, so the `;` (or its absence before `]`) selects.
        if self.check(&TokenKind::LeftBracket) {
            let open = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'['".to_string(),
            })?;
            let element = self.parse_type()?;
            if self.check(&TokenKind::RightBracket) {
                let close = self.advance().ok_or(ParseError::UnexpectedEof {
                    expected: "']'".to_string(),
                })?;
                return Ok(Type::Slice {
                    element: Box::new(element),
                    span: open.span.merge(close.span),
                });
            }
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
        // A parenthesized type list opens either a tuple type `(T1, T2, ...)` or a
        // closure/function type `(T1, ...) -> R` — disambiguated by a trailing `->`.
        // A tuple needs two or more elements; a function type accepts zero or more.
        if self.check(&TokenKind::LeftParen) {
            let open = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'('".to_string(),
            })?;
            let mut elements = Vec::new();
            self.skip_newlines();
            if !self.check(&TokenKind::RightParen) {
                loop {
                    self.skip_newlines();
                    elements.push(self.parse_type()?);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                }
            }
            let close = self.consume(TokenKind::RightParen, "')' to close type list")?;
            // `(T1, ...) -> R` is a closure/function type.
            if self.check(&TokenKind::Arrow) {
                self.advance(); // consume '->'
                let ret = self.parse_type()?;
                let span = open.span.merge(ret.span());
                return Ok(Type::Function {
                    params: elements,
                    ret: Box::new(ret),
                    span,
                });
            }
            if elements.len() < 2 {
                return Err(ParseError::UnexpectedToken {
                    found: TokenKind::RightParen,
                    expected: "a tuple type `(T1, T2, ...)` or a function type `(T1, ...) -> R`"
                        .to_string(),
                    span: close.span,
                });
            }
            let span = open.span.merge(close.span);
            return Ok(Type::Tuple { elements, span });
        }
        // Borrow type `&T` / `&mut T`, with an optional explicit lifetime
        // `&'a T` / `&'a mut T`. The referent is parsed recursively, so the `&`
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

        // Static-dispatch bound `impl Trait`: the `impl` keyword followed by a
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
        // Dynamic-dispatch trait object `dyn Trait`: the `dyn` keyword followed
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
                let mut span = token.span;
                let mut name = name;
                // A module-qualified type (`geometry::Point`). The qualifier rides in the
                // name until module resolution verifies and strips it; no downstream pass
                // ever sees a `::` in a type name.
                while self.check(&TokenKind::ColonColon) {
                    self.advance(); // consume '::'
                    let segment =
                        self.consume(TokenKind::Identifier(String::new()), "type name after '::'")?;
                    if let TokenKind::Identifier(next) = segment.kind {
                        name = format!("{}::{}", name, next);
                        span = span.merge(segment.span);
                    }
                }
                let ident = Identifier { name, span };
                // Generic type application `Name<T1, T2, ...>`. Without a
                // following `<`, this is a plain named type. Arguments may be types or
                // const (integer) values, as in `Ring<i32, 4>`.
                if self.check(&TokenKind::Less) {
                    let (args, close_span) = self.parse_generic_type_args()?;
                    return Ok(Type::Generic {
                        name: ident,
                        args,
                        span: span.merge(close_span),
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

    /// Parse the trait-name identifier following an `impl` / `dyn` keyword.
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

    /// Parse a `<T1, N, ...>` generic-argument list in a type application. Each
    /// argument is a type or a non-negative integer const value (`Ring<i32, 4>`).
    /// Returns the arguments and the span of the closing `>`, so the caller can
    /// span the whole application: ending at the last argument leaves the `>` out
    /// of every diagnostic that points at the type.
    fn parse_generic_type_args(&mut self) -> ParseResult<(Vec<GenericArg>, Span)> {
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
        let close = self.consume(TokenKind::Greater, "'>' to close type arguments")?;
        Ok((args, close.span))
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Item, Stmt, Type};
    use crate::parse;

    /// The declared type of the first `val` in the first function body.
    fn first_var_type(items: &[Item]) -> Option<Type> {
        for item in items {
            if let Item::Function(func) = item {
                for stmt in &func.body {
                    if let Stmt::VarDecl { ty, .. } = stmt {
                        return ty.clone();
                    }
                }
            }
        }
        None
    }

    /// Regression: the span of a generic type application ended at its last argument,
    /// leaving the closing `>` out of every diagnostic that pointed at the type.
    #[test]
    fn generic_type_span_covers_the_closing_angle_bracket() {
        let src = "func main() -> i32 { val b: Box<i32> = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Generic { span, .. } = ty else {
            panic!("expected a generic type application, got {ty:?}");
        };
        assert_eq!(&src[span.start..span.end], "Box<i32>");
    }

    #[test]
    fn slice_type_parses_without_a_length() {
        let src = "func sum(xs: &[i32]) -> i32 { return 0 }";
        let items = parse(src).expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Reference { inner, mutable, .. } = &func.params[0].ty else {
            panic!("expected a reference parameter type");
        };
        assert!(!mutable);
        let Type::Slice { element, span } = inner.as_ref() else {
            panic!("expected a slice referent, got {inner:?}");
        };
        assert!(matches!(element.as_ref(), Type::Named(id) if id.name == "i32"));
        assert_eq!(&src[span.start..span.end], "[i32]");
    }

    #[test]
    fn mutable_slice_type_parses() {
        let src = "func fill(xs: &mut [u8]) { }";
        let items = parse(src).expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Reference { inner, mutable, .. } = &func.params[0].ty else {
            panic!("expected a reference parameter type");
        };
        assert!(mutable);
        assert!(matches!(inner.as_ref(), Type::Slice { .. }));
    }

    /// `[T; N]` keeps its own shape now that `[T]` shares the opening bracket.
    #[test]
    fn sized_array_type_still_parses() {
        let src = "func main() -> i32 { val a: [i32; 3] = [1, 2, 3]\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        assert!(matches!(ty, Type::Array { .. }));
    }

    #[test]
    fn multi_argument_generic_type_span_covers_the_closing_angle_bracket() {
        let src = "func main() -> i32 { val p: Pair<i32, bool> = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Generic { span, .. } = ty else {
            panic!("expected a generic type application, got {ty:?}");
        };
        assert_eq!(&src[span.start..span.end], "Pair<i32, bool>");
    }
}
