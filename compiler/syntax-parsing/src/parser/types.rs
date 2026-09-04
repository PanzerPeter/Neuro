use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{ArraySize, GenericArg, Type};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

/// A bound's parsed `<Assoc = T, ...>` list and the span of its closing `>`, both empty
/// for the bare `Trait` form.
pub(super) type AssocBindings = (Vec<(Identifier, Type)>, Option<Span>);

/// A parsed `[d0, d1, ...]` shape argument: its extents and the span of the brackets.
/// Only `Tensor<T, [...]>` accepts one, so `parse_generic_type_args` hands it back
/// separately rather than widening [`GenericArg`] for a single type.
type ShapeArg = (Vec<usize>, Span);

/// The only form `Self` takes in a type annotation: bare `Self` is not one, because the
/// implementing type is always nameable where an annotation is written.
/// The one type name that accepts a `[...]` shape argument. It is a prelude name
/// rather than a keyword, so the parser only claims it once a shape appears — a module
/// that shadows `Tensor` with its own generic type keeps parsing as before.
const TENSOR_TYPE_NAME: &str = "Tensor";

const SELF_ASSOC_FORM: &str = "`Self::` followed by an associated type name — bare `Self` is not a type annotation, name the type itself";

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
            let (assoc_bindings, close_span) = self.parse_assoc_bindings()?;
            let span = kw.span.merge(close_span.unwrap_or(trait_name.span));
            return Ok(Type::ImplTrait {
                trait_name,
                assoc_bindings,
                span,
            });
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

        // Associated-type path `Self::Item`. The qualifier rides in the name exactly as a
        // module qualifier does, so no pass between here and the type checker — which is
        // the first place an implementing type is known — needs a node of its own for it.
        if self.check(&TokenKind::SelfUpper) {
            let kw = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "'Self'".to_string(),
            })?;
            if !self.check(&TokenKind::ColonColon) {
                return Err(ParseError::UnexpectedToken {
                    found: TokenKind::SelfUpper,
                    expected: SELF_ASSOC_FORM.to_string(),
                    span: kw.span,
                });
            }
            self.advance(); // consume '::'
            let assoc = self.consume_identifier("associated type name after `Self::`")?;
            return Ok(Type::Named(Identifier {
                name: format!("Self::{}", assoc.name),
                span: kw.span.merge(assoc.span),
            }));
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
                    let (args, shape, close_span) = self.parse_generic_type_args()?;
                    let span = span.merge(close_span);
                    if let Some((dims, shape_span)) = shape {
                        return Self::build_tensor_type(ident, args, dims, shape_span, span);
                    }
                    return Ok(Type::Generic {
                        name: ident,
                        args,
                        span,
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

    /// Parse the optional `<Assoc = T, ...>` constraint list that follows a trait name
    /// in a bound, returning the bindings and the span of the closing `>`.
    ///
    /// Both are empty / `None` when no `<` follows, which is the bare `Trait` form. A
    /// trait's own generic parameters are not bound positionally here: every entry must
    /// name an associated type, so a positional argument is a parse error rather than a
    /// silently accepted one.
    pub(super) fn parse_assoc_bindings(&mut self) -> ParseResult<AssocBindings> {
        if !self.check(&TokenKind::Less) {
            return Ok((Vec::new(), None));
        }
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();
        let mut bindings = Vec::new();
        loop {
            let name_token = self.consume(
                TokenKind::Identifier(String::new()),
                "associated type name in a `Trait<Assoc = T>` bound",
            )?;
            let TokenKind::Identifier(name) = name_token.kind else {
                return Err(ParseError::UnexpectedToken {
                    found: name_token.kind,
                    expected: "associated type name in a `Trait<Assoc = T>` bound".to_string(),
                    span: name_token.span,
                });
            };
            self.consume(TokenKind::Equal, "'=' after an associated type name")?;
            self.skip_newlines();
            let ty = self.parse_type()?;
            bindings.push((
                Identifier {
                    name,
                    span: name_token.span,
                },
                ty,
            ));
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // ','
            self.skip_newlines();
        }
        let close = self.consume(TokenKind::Greater, "'>'")?;
        Ok((bindings, Some(close.span)))
    }

    /// Parse a `<T1, N, ...>` generic-argument list in a type application. Each
    /// argument is a type or a non-negative integer const value (`Ring<i32, 4>`).
    /// Returns the arguments and the span of the closing `>`, so the caller can
    /// span the whole application: ending at the last argument leaves the `>` out
    /// of every diagnostic that points at the type.
    fn parse_generic_type_args(
        &mut self,
    ) -> ParseResult<(Vec<GenericArg>, Option<ShapeArg>, Span)> {
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();
        let mut args = Vec::new();
        let mut shape: Option<ShapeArg> = None;
        loop {
            if self.shape_argument_ahead() {
                let parsed = self.parse_shape_argument()?;
                let parsed_span = parsed.1;
                // A second shape argument cannot be a tensor's, and `build_tensor_type`
                // only ever sees the first, so reject it here where the span is still to
                // hand rather than letting it vanish.
                if shape.replace(parsed).is_some() {
                    return Err(ParseError::TensorTypeArity { span: parsed_span });
                }
            } else if let Some(TokenKind::Integer(n)) = self.peek_kind() {
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
        Ok((args, shape, close.span))
    }

    /// Whether the argument at the cursor is a `[d0, d1, ...]` shape rather than an
    /// array or slice type. An integer (or an immediate `]`) can never open a type, so
    /// the token after `[` decides without backtracking.
    fn shape_argument_ahead(&self) -> bool {
        if !self.check(&TokenKind::LeftBracket) {
            return false;
        }
        let mut i = self.current + 1;
        while matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Newline)
        ) {
            i += 1;
        }
        matches!(
            self.tokens.get(i).map(|t| &t.kind),
            Some(TokenKind::Integer(_)) | Some(TokenKind::RightBracket)
        )
    }

    /// Parse a `[d0, d1, ...]` tensor shape. Every extent is a non-negative integer
    /// literal; an empty list is the rank-0 scalar shape.
    fn parse_shape_argument(&mut self) -> ParseResult<ShapeArg> {
        let open = self.consume(TokenKind::LeftBracket, "'[' to open a tensor shape")?;
        self.skip_newlines();
        let mut dims = Vec::new();
        while !self.check(&TokenKind::RightBracket) {
            let token = self.advance().ok_or(ParseError::UnexpectedEof {
                expected: "a tensor dimension".to_string(),
            })?;
            let TokenKind::Integer(extent) = token.kind else {
                return Err(ParseError::UnexpectedToken {
                    found: token.kind,
                    expected: "a non-negative integer tensor dimension".to_string(),
                    span: token.span,
                });
            };
            let Ok(extent) = usize::try_from(extent) else {
                return Err(ParseError::UnexpectedToken {
                    found: TokenKind::Integer(extent),
                    expected: "a non-negative integer tensor dimension".to_string(),
                    span: token.span,
                });
            };
            dims.push(extent);
            self.skip_newlines();
            if !self.check(&TokenKind::Comma) {
                break;
            }
            self.advance(); // consume ','
            self.skip_newlines();
        }
        let close = self.consume(TokenKind::RightBracket, "']' to close a tensor shape")?;
        Ok((dims, open.span.merge(close.span)))
    }

    /// Assemble `Tensor<T, [...]>` from an argument list that carried a shape.
    ///
    /// The shape is what marks the application as a tensor, so a shape under any other
    /// name is rejected here rather than left for the type checker: no other type in the
    /// language accepts one, and the parser already knows the name.
    fn build_tensor_type(
        name: Identifier,
        args: Vec<GenericArg>,
        shape: Vec<usize>,
        shape_span: Span,
        span: Span,
    ) -> ParseResult<Type> {
        if name.name != TENSOR_TYPE_NAME {
            return Err(ParseError::ShapeArgumentOnNonTensor {
                name: name.name,
                span: shape_span,
            });
        }
        let [GenericArg::Type(element_type)] =
            <[GenericArg; 1]>::try_from(args).map_err(|_| ParseError::TensorTypeArity { span })?
        else {
            return Err(ParseError::TensorTypeArity { span });
        };
        Ok(Type::Tensor {
            element_type: Box::new(element_type),
            shape,
            span,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{GenericArg, Item, Stmt, Type};
    use crate::errors::ParseError;
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

    #[test]
    fn tensor_type_parses_element_and_static_shape() {
        let src = "func main() -> i32 { val m: Tensor<f32, [2, 3]> = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Tensor {
            element_type,
            shape,
            span,
        } = ty
        else {
            panic!("expected a tensor type, got {ty:?}");
        };
        assert!(matches!(element_type.as_ref(), Type::Named(id) if id.name == "f32"));
        assert_eq!(shape, vec![2, 3]);
        assert_eq!(&src[span.start..span.end], "Tensor<f32, [2, 3]>");
    }

    #[test]
    fn rank_zero_tensor_type_parses_with_an_empty_shape() {
        let src = "func main() -> i32 { val s: Tensor<f32, []> = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Tensor { shape, .. } = ty else {
            panic!("expected a tensor type, got {ty:?}");
        };
        assert!(shape.is_empty());
    }

    #[test]
    fn higher_rank_tensor_type_parses() {
        let src = "func load(x: Tensor<f32, [3, 224, 224]>) { }";
        let items = parse(src).expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(shape, &vec![3, 224, 224]);
    }

    /// A shape argument is what marks a tensor, so `[T; N]` and `[T]` type arguments
    /// must still reach `parse_type` unchanged.
    #[test]
    fn bracketed_type_argument_is_still_an_array_or_slice() {
        let src = "func main() -> i32 { val b: Box<[i32; 3]> = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Generic { args, .. } = ty else {
            panic!("expected a generic type application, got {ty:?}");
        };
        assert!(matches!(&args[0], GenericArg::Type(Type::Array { .. })));
    }

    #[test]
    fn shape_argument_on_a_non_tensor_type_is_rejected() {
        let err = parse("func f(x: Grid<f32, [2, 2]>) { }").expect_err("rejected");
        assert!(
            matches!(&err, ParseError::ShapeArgumentOnNonTensor { name, .. } if name == "Grid"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn tensor_without_an_element_type_is_rejected() {
        let err = parse("func f(x: Tensor<[2, 2]>) { }").expect_err("rejected");
        assert!(
            matches!(err, ParseError::TensorTypeArity { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn tensor_with_a_third_argument_is_rejected() {
        let err = parse("func f(x: Tensor<f32, [2, 2], i32>) { }").expect_err("rejected");
        assert!(
            matches!(err, ParseError::TensorTypeArity { .. }),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn negative_tensor_dimension_is_rejected() {
        let err = parse("func f(x: Tensor<f32, [2, -1]>) { }").expect_err("rejected");
        assert!(
            matches!(&err, ParseError::UnexpectedToken { expected, .. }
                if expected.contains("tensor dimension")),
            "unexpected error: {err:?}"
        );
    }

    /// Shape parameters and dynamic axes are later roadmap items; until then a
    /// non-literal extent must fail loudly rather than parse as something else.
    #[test]
    fn symbolic_tensor_dimension_is_rejected() {
        let err = parse("func f(x: Tensor<f32, [2, N]>) { }").expect_err("rejected");
        assert!(
            matches!(&err, ParseError::UnexpectedToken { expected, .. }
                if expected.contains("tensor dimension")),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn self_assoc_path_carries_its_qualifier_and_spans_both_halves() {
        let src = "func main() -> i32 { val x: Self::Item = 0\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        let Type::Named(ident) = ty else {
            panic!("expected a named type, got {ty:?}");
        };
        assert_eq!(ident.name, "Self::Item");
        assert_eq!(&src[ident.span.start..ident.span.end], "Self::Item");
    }
}
