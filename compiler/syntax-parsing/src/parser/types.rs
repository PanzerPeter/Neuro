use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::ast::{ArraySize, GenericArg, TensorDim, TensorExtent, Type};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

/// A bound's parsed `<Assoc = T, ...>` list and the span of its closing `>`, both empty
/// for the bare `Trait` form.
pub(super) type AssocBindings = (Vec<(Identifier, Type)>, Option<Span>);

/// A parsed `[d0, d1, ...]` shape argument: its extents and the span of the brackets.
/// Only `Tensor<T, [...]>` accepts one, so `parse_generic_type_args` hands it back
/// separately rather than widening [`GenericArg`] for a single type.
pub(super) type ShapeArg = (Vec<TensorDim>, Span);

/// The only form `Self` takes in a type annotation: bare `Self` is not one, because the
/// implementing type is always nameable where an annotation is written.
/// The one type name that accepts a `[...]` shape argument. It is a prelude name
/// rather than a keyword, so the parser only claims it once a shape appears: a module
/// that shadows `Tensor` with its own generic type keeps parsing as before.
pub(super) const TENSOR_TYPE_NAME: &str = "Tensor";

const SELF_ASSOC_FORM: &str = "`Self::` followed by an associated type name; bare `Self` is not a type annotation, name the type itself";

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
                TokenKind::Integer(n) => ArraySize::Literal(n),
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
        // closure/function type `(T1, ...) -> R`: disambiguated by a trailing `->`.
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
        // module qualifier does, so no pass between here and the type checker (which is
        // the first place an implementing type is known) needs a node of its own for it.
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
                    let symbolic_extents = ident.name == TENSOR_TYPE_NAME;
                    let (args, shape, close_span) =
                        self.parse_generic_type_args(symbolic_extents)?;
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
    ///
    /// `symbolic_extents` says whether an identifier-led `[...]` argument is a shape. Only
    /// `Tensor` accepts one: everywhere else `[T]` is the slice type it has always been,
    /// so the flag is what keeps `Vec<[T]>` parsing as it did. It is about the EXTENT
    /// being a name, not about dimension names, which any shape may carry.
    pub(super) fn parse_generic_type_args(
        &mut self,
        symbolic_extents: bool,
    ) -> ParseResult<(Vec<GenericArg>, Option<ShapeArg>, Span)> {
        self.consume(TokenKind::Less, "'<'")?;
        self.skip_newlines();
        let mut args = Vec::new();
        let mut shape: Option<ShapeArg> = None;
        loop {
            if self.shape_argument_ahead(symbolic_extents) {
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
                // An integer token carries a magnitude, so a negative const argument
                // is a `-` token followed by one and is rejected as an unexpected token
                // before reaching here.
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
    /// array or slice type. An integer, a `?`, or an immediate `]` can never open a
    /// type, so the token after `[` decides without backtracking.
    ///
    /// An identifier-led shape (`[M, K]`) is ambiguous with the slice type `[T]`, so it
    /// is claimed only under `Tensor`, where a slice cannot appear: `symbolic_extents` carries
    /// that from the caller, which already knows the name.
    fn shape_argument_ahead(&self, symbolic_extents: bool) -> bool {
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
            Some(TokenKind::Integer(_)) | Some(TokenKind::RightBracket) | Some(TokenKind::Question)
        ) || (symbolic_extents
            && matches!(
                self.tokens.get(i).map(|t| &t.kind),
                Some(TokenKind::Identifier(_))
            ))
    }

    /// Parse the `batch:` that may open an axis, or `None` when the axis is unnamed.
    ///
    /// The colon is what distinguishes a name from a shape parameter used as the extent,
    /// so the decision needs the token after the identifier and cannot be made from the
    /// identifier alone.
    fn parse_dimension_name(&mut self) -> ParseResult<Option<Identifier>> {
        let Some(TokenKind::Identifier(name)) = self.peek_kind() else {
            return Ok(None);
        };
        if !matches!(
            self.tokens.get(self.current + 1).map(|t| &t.kind),
            Some(TokenKind::Colon)
        ) {
            return Ok(None);
        }
        let name = name.clone();
        let span = self
            .advance()
            .map(|t| t.span)
            .ok_or(ParseError::UnexpectedEof {
                expected: "a tensor dimension name".to_string(),
            })?;
        self.advance(); // consume ':'
        self.skip_newlines();
        Ok(Some(Identifier { name, span }))
    }

    /// Parse one axis extent: a non-negative integer literal, a shape parameter's name,
    /// or `?` for an axis whose extent is not known until run time.
    fn parse_tensor_extent(&mut self) -> ParseResult<TensorExtent> {
        let token = self.advance().ok_or(ParseError::UnexpectedEof {
            expected: "a tensor dimension".to_string(),
        })?;
        match token.kind {
            TokenKind::Integer(extent) => {
                let Ok(extent) = usize::try_from(extent) else {
                    return Err(ParseError::UnexpectedToken {
                        found: TokenKind::Integer(extent),
                        expected: "a non-negative integer tensor dimension".to_string(),
                        span: token.span,
                    });
                };
                Ok(TensorExtent::Literal(extent))
            }
            TokenKind::Identifier(name) => Ok(TensorExtent::Param(Identifier {
                name,
                span: token.span,
            })),
            TokenKind::Question => Ok(TensorExtent::Dynamic(token.span)),
            found => Err(ParseError::UnexpectedToken {
                found,
                expected: "a tensor dimension: a non-negative integer, a shape parameter, or `?`"
                    .to_string(),
                span: token.span,
            }),
        }
    }

    /// Parse a `[d0, d1, ...]` tensor shape. An axis is an extent — a non-negative
    /// integer literal, a shape parameter's name, or `?` — optionally preceded by a
    /// dimension name and a colon (`[batch: 32]`); an empty list is the rank-0 scalar
    /// shape.
    fn parse_shape_argument(&mut self) -> ParseResult<ShapeArg> {
        let open = self.consume(TokenKind::LeftBracket, "'[' to open a tensor shape")?;
        self.skip_newlines();
        let mut dims = Vec::new();
        while !self.check(&TokenKind::RightBracket) {
            let name = self.parse_dimension_name()?;
            let dim = TensorDim {
                name,
                extent: self.parse_tensor_extent()?,
            };
            dims.push(dim);
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
    pub(super) fn build_tensor_type(
        name: Identifier,
        args: Vec<GenericArg>,
        shape: Vec<TensorDim>,
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
    use crate::ast::{GenericArg, Item, Stmt, TensorDim, TensorExtent, Type};
    use crate::errors::ParseError;
    use crate::parse;

    /// An unnamed axis of a literal extent, the shape a test without dimension names writes.
    fn dim(extent: usize) -> TensorDim {
        TensorDim {
            name: None,
            extent: TensorExtent::Literal(extent),
        }
    }

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
        assert_eq!(shape, vec![dim(2), dim(3)]);
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
        assert_eq!(shape, &vec![dim(3), dim(224), dim(224)]);
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

    #[test]
    fn a_named_tensor_dimension_parses_as_a_shape_parameter() {
        let items = parse("func f<N>(x: Tensor<f32, [2, N]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(shape.len(), 2);
        assert_eq!(shape[0], dim(2));
        assert!(matches!(&shape[1].extent, TensorExtent::Param(id) if id.name == "N"));
        assert!(shape[1].name.is_none());
    }

    #[test]
    fn a_dimension_name_parses_alongside_its_extent() {
        let src = "func f(x: Tensor<f32, [batch: 32, embed: 768]>) { }";
        let items = parse(src).expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(shape.len(), 2);
        assert_eq!(
            shape[0].name.as_ref().map(|n| n.name.as_str()),
            Some("batch")
        );
        assert_eq!(shape[0].extent, TensorExtent::Literal(32));
        assert_eq!(
            shape[1].name.as_ref().map(|n| n.name.as_str()),
            Some("embed")
        );
        assert_eq!(shape[1].extent, TensorExtent::Literal(768));
    }

    #[test]
    fn a_question_mark_parses_as_a_dynamic_extent() {
        let items = parse("func f(x: Tensor<f32, [?, 784]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(shape.len(), 2);
        assert!(matches!(shape[0].extent, TensorExtent::Dynamic(_)));
        assert!(shape[0].name.is_none());
        assert_eq!(shape[1].extent, TensorExtent::Literal(784));
    }

    /// A `?` axis documents itself like any other: the name and the extent are
    /// independent, so `[batch: ?]` is both named and dynamic.
    #[test]
    fn a_named_axis_may_be_dynamic() {
        let items = parse("func f(x: Tensor<f32, [batch: ?, embed: 768]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(
            shape[0].name.as_ref().map(|n| n.name.as_str()),
            Some("batch")
        );
        assert!(matches!(shape[0].extent, TensorExtent::Dynamic(_)));
        // A `?` is not a name, so it re-kinds nothing into a shape parameter.
        assert!(func.generics.is_empty());
    }

    /// The colon is the whole distinction: `[N]` names the extent, `[batch: N]` names
    /// the axis and leaves `N` as the extent.
    #[test]
    fn a_named_axis_may_still_take_a_shape_parameter_as_its_extent() {
        let items = parse("func f<N>(x: Tensor<f32, [batch: N]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(
            shape[0].name.as_ref().map(|n| n.name.as_str()),
            Some("batch")
        );
        assert!(matches!(&shape[0].extent, TensorExtent::Param(id) if id.name == "N"));
        // The extent re-kinds the generic; the axis name is not a parameter at all.
        assert_eq!(func.generics.len(), 1);
        assert!(matches!(
            func.generics[0].kind,
            crate::ast::GenericParamKind::Const(_)
        ));
    }

    /// A bare name used as an extent is a `const N: u32` parameter, so the signature
    /// need not spell `const` on every dimension.
    #[test]
    fn a_shape_parameter_is_re_kinded_to_a_const_parameter() {
        let items = parse("func f<N>(x: Tensor<f32, [N, N]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let [param] = &func.generics[..] else {
            panic!("expected one generic parameter, got {:?}", func.generics);
        };
        let crate::ast::GenericParamKind::Const(ty) = &param.kind else {
            panic!("expected a const parameter, got {:?}", param.kind);
        };
        assert!(matches!(ty, Type::Named(id) if id.name == "u32"));
    }

    /// A type parameter used only in ordinary positions keeps its kind: the re-kinding
    /// is driven by the shape positions the signature actually writes.
    #[test]
    fn a_type_parameter_outside_a_shape_keeps_its_kind() {
        let items = parse("func f<T>(x: T) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let [param] = &func.generics[..] else {
            panic!("expected one generic parameter");
        };
        assert!(matches!(param.kind, crate::ast::GenericParamKind::Type));
    }

    /// A `?` is claimed as a shape wherever it appears in the list, so a static axis
    /// before it does not decide how the rest is read.
    #[test]
    fn a_dynamic_axis_parses_after_a_static_one() {
        let items = parse("func f(x: Tensor<f32, [2, ?]>) { }").expect("parses");
        let Some(Item::Function(func)) = items.first() else {
            panic!("expected a function item");
        };
        let Type::Tensor { shape, .. } = &func.params[0].ty else {
            panic!("expected a tensor parameter type");
        };
        assert_eq!(shape[0].extent, TensorExtent::Literal(2));
        assert!(matches!(shape[1].extent, TensorExtent::Dynamic(_)));
    }

    /// A shape is not a type, so `?` still fails loudly where a type belongs.
    #[test]
    fn a_question_mark_is_not_a_type() {
        let err = parse("func f(x: ?) { }").expect_err("rejected");
        assert!(
            matches!(&err, ParseError::UnexpectedToken { .. }),
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

    /// The initializer expression of the first `val` in the first function body.
    fn first_var_init(items: &[Item]) -> Option<crate::ast::Expr> {
        for item in items {
            if let Item::Function(func) = item {
                for stmt in &func.body {
                    if let Stmt::VarDecl { init, .. } = stmt {
                        return init.clone();
                    }
                }
            }
        }
        None
    }

    #[test]
    fn a_tensor_turbofish_constructor_parses_as_a_call_on_a_path() {
        let src = "func main() -> i32 { val z = Tensor::<f32, [3, 3]>::zeros()\n return 0 }";
        let items = parse(src).expect("parses");
        let init = first_var_init(&items).expect("has a var decl");
        let crate::ast::Expr::Call {
            func,
            type_args,
            args,
            ..
        } = init
        else {
            panic!("expected a call, got {init:?}");
        };
        assert!(args.is_empty());
        let [GenericArg::Type(Type::Tensor {
            element_type,
            shape,
            ..
        })] = &type_args[..]
        else {
            panic!("expected one tensor type argument, got {type_args:?}");
        };
        assert!(matches!(**element_type, Type::Named(ref i) if i.name == "f32"));
        assert_eq!(*shape, vec![dim(3), dim(3)]);
        let crate::ast::Expr::Path {
            type_name, member, ..
        } = *func
        else {
            panic!("expected a path callee");
        };
        assert_eq!(type_name.name, "Tensor");
        assert_eq!(member.name, "zeros");
    }

    /// A rank-0 constructor is spelled with an empty shape, which is also the one
    /// shape argument that carries no integer to key on.
    #[test]
    fn a_rank_zero_turbofish_constructor_parses() {
        let src = "func main() -> i32 { val s = Tensor::<f32, []>::scalar(1.0)\n return 0 }";
        let items = parse(src).expect("parses");
        let init = first_var_init(&items).expect("has a var decl");
        let crate::ast::Expr::Call { type_args, .. } = init else {
            panic!("expected a call");
        };
        let [GenericArg::Type(Type::Tensor { shape, .. })] = &type_args[..] else {
            panic!("expected one tensor type argument");
        };
        assert!(shape.is_empty());
    }

    /// Without a shape the turbofish names no tensor, so the arity error fires rather
    /// than the form being read as a plain generic application.
    #[test]
    fn a_tensor_turbofish_without_a_shape_is_an_arity_error() {
        let src = "func main() -> i32 { val z = Tensor::<f32>::zeros()\n return 0 }";
        let err = parse(src).expect_err("should not parse");
        assert!(
            matches!(&err, ParseError::TensorTypeArity { .. }),
            "unexpected error: {err:?}"
        );
    }
}
