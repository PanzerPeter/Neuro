// Tensor value construction: nested-array-literal coercion and the six
// `Tensor::<T, [...]>::ctor(...)` construction helpers.
//
// Reached from the array-literal arm of `check_expr` and from the associated-call arm
// of `check_call_expr`. Adds methods to the same `impl TypeChecker` block as the rest
// of `type_checkers`.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::{Expr, GenericArg};
use shared_types::{Identifier, Span};

/// The prelude name a tensor constructor is qualified by. A module may shadow it with
/// a type of its own, so the constructor resolvers below check `struct_defs` /
/// `enum_defs` first and stand aside when it is taken.
pub(crate) const TENSOR_TYPE_NAME: &str = "Tensor";

/// The prelude enum naming the device a tensor's buffer lives on, and the one argument
/// `tensor.to(device)` accepts.
pub(crate) const DEVICE_TYPE_NAME: &str = "Device";

/// A tensor filled with one value everywhere.
const CTOR_ZEROS: &str = "zeros";
const CTOR_ONES: &str = "ones";
/// The rank-2 identity matrix.
const CTOR_IDENTITY: &str = "identity";
/// A normally distributed fill, `random_normal(mean:, std:)`.
const CTOR_RANDOM_NORMAL: &str = "random_normal";
/// The rank-0 tensor holding one value.
const CTOR_SCALAR: &str = "scalar";
/// A tensor built from a nested array literal, without needing an annotation.
const CTOR_FROM: &str = "from";

/// Whether `ctor` names one of the construction helpers, so an unrelated
/// `Tensor::whatever(...)` is still reported as an unknown associated function rather
/// than silently type-checked.
pub(crate) fn is_tensor_constructor(ctor: &str) -> bool {
    matches!(
        ctor,
        CTOR_ZEROS | CTOR_ONES | CTOR_IDENTITY | CTOR_RANDOM_NORMAL | CTOR_SCALAR | CTOR_FROM
    )
}

impl TypeChecker {
    /// Whether the program leaves the prelude `Tensor` name free, so a
    /// `Tensor::ctor(...)` call may be read as a builtin construction.
    pub(crate) fn tensor_name_is_free(&self) -> bool {
        !self.struct_defs.contains_key(TENSOR_TYPE_NAME)
            && !self.enum_defs.contains_key(TENSOR_TYPE_NAME)
    }

    /// Type-check a nested array literal against a `Tensor<T, [d0, ...]>` annotation.
    ///
    /// The annotation supplies both the element type and every extent, so each leaf
    /// literal is typed *by* it, exactly as `val x: f32 = 0.01` types its literal,
    /// and no narrowing of an already-typed value is involved. The literal must be
    /// rectangular and as deep as the shape is long.
    pub(crate) fn check_tensor_literal(
        &mut self,
        elements: &[Expr],
        element_ty: &Type,
        shape: &[usize],
        span: Span,
    ) -> Type {
        let tensor = Type::Tensor {
            element: Box::new(element_ty.clone()),
            shape: shape.to_vec(),
        };
        // A rank-0 tensor holds exactly one value and has no axis to write elements
        // along, so there is no array literal that could denote one.
        let Some((&extent, rest)) = shape.split_first() else {
            self.record_error(TypeError::TensorScalarNeedsConstructor { span });
            return tensor;
        };
        if elements.len() != extent {
            self.record_error(TypeError::TensorExtentMismatch {
                expected: extent,
                found: elements.len(),
                span,
            });
        }
        for element in elements {
            self.check_tensor_literal_element(element, element_ty, rest, shape.len());
        }
        tensor
    }

    /// One element of a tensor literal: a nested literal when axes remain, a leaf value
    /// at the innermost axis.
    fn check_tensor_literal_element(
        &mut self,
        element: &Expr,
        element_ty: &Type,
        remaining_shape: &[usize],
        rank: usize,
    ) {
        if remaining_shape.is_empty() {
            // A nested literal where the shape has run out is the ragged case reported
            // as a rank mismatch: the shape says this axis holds scalars.
            if let Expr::ArrayLiteral { span, .. } = element {
                self.record_error(TypeError::TensorRankMismatch {
                    expected: rank,
                    found: rank + 1,
                    span: *span,
                });
                return;
            }
            let Some(found) = self.check_expr(element, Some(element_ty)) else {
                return;
            };
            if !matches!(found, Type::Unknown) && !found.is_compatible_with(element_ty) {
                self.record_error(TypeError::Mismatch {
                    expected: element_ty.clone(),
                    found,
                    span: element.span(),
                });
            }
            return;
        }
        let Expr::ArrayLiteral {
            elements: nested,
            span,
        } = element
        else {
            self.record_error(TypeError::TensorRankMismatch {
                expected: rank,
                found: rank - remaining_shape.len(),
                span: element.span(),
            });
            return;
        };
        self.check_tensor_literal(nested, element_ty, remaining_shape, *span);
    }

    /// Type-check `Tensor::<T, [...]>::ctor(args)` and the annotation-driven
    /// `Tensor::ctor(args)` that spells the same construction without a turbofish.
    ///
    /// `type_args` carries the turbofish's assembled tensor type when one was written;
    /// otherwise the type comes from the surrounding expectation, and a position that
    /// supplies neither is reported rather than guessed at.
    pub(crate) fn check_tensor_construction(
        &mut self,
        ctor: &Identifier,
        type_args: &[GenericArg],
        args: &[Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Type {
        if !is_tensor_constructor(&ctor.name) {
            self.record_error(TypeError::UnknownTensorConstructor {
                ctor: ctor.name.clone(),
                span,
            });
            return Type::Unknown;
        }
        let Some(tensor) = self.tensor_construction_type(type_args, expected, ctor, span) else {
            return Type::Unknown;
        };
        let Type::Tensor { element, shape } = &tensor else {
            return Type::Unknown;
        };
        let element = (**element).clone();

        match ctor.name.as_str() {
            CTOR_ZEROS | CTOR_ONES => {
                self.check_tensor_ctor_arity(args, 0, span);
            }
            CTOR_IDENTITY => {
                self.check_tensor_ctor_arity(args, 0, span);
                let square = matches!(shape.as_slice(), [rows, cols] if rows == cols);
                if !square {
                    self.reject_tensor_ctor(
                        ctor,
                        &tensor,
                        "an identity matrix is square and rank 2",
                        span,
                    );
                }
            }
            CTOR_RANDOM_NORMAL => {
                if !matches!(element, Type::F32 | Type::F64) {
                    self.reject_tensor_ctor(
                        ctor,
                        &tensor,
                        "a normal distribution is drawn in `f32` or `f64`",
                        span,
                    );
                }
                if self.check_tensor_ctor_arity(args, 2, span) {
                    self.check_tensor_ctor_args(args, &element);
                }
            }
            CTOR_SCALAR => {
                if !shape.is_empty() {
                    self.reject_tensor_ctor(
                        ctor,
                        &tensor,
                        "`scalar` builds the rank-0 tensor `Tensor<T, []>`",
                        span,
                    );
                }
                if self.check_tensor_ctor_arity(args, 1, span) {
                    self.check_tensor_ctor_args(args, &element);
                }
            }
            CTOR_FROM => {
                if self.check_tensor_ctor_arity(args, 1, span) {
                    let shape = shape.clone();
                    match &args[0] {
                        Expr::ArrayLiteral {
                            elements,
                            span: lit_span,
                        } => {
                            self.check_tensor_literal(elements, &element, &shape, *lit_span);
                        }
                        // `from` exists so a tensor can be built where no annotation
                        // reaches; its argument is the same nested literal the annotated
                        // form coerces, not an arbitrary expression.
                        other => self.reject_tensor_ctor(
                            ctor,
                            &tensor,
                            "`from` takes a nested array literal",
                            other.span(),
                        ),
                    }
                }
            }
            _ => unreachable!("guarded by is_tensor_constructor"),
        }
        tensor
    }

    /// The tensor type a construction builds: the turbofish's when one was written,
    /// otherwise the surrounding expectation's.
    fn tensor_construction_type(
        &mut self,
        type_args: &[GenericArg],
        expected: Option<&Type>,
        ctor: &Identifier,
        span: Span,
    ) -> Option<Type> {
        if let [GenericArg::Type(annotation)] = type_args {
            return self.resolve_type(annotation);
        }
        match expected {
            Some(ty @ Type::Tensor { .. }) => Some(ty.clone()),
            _ => {
                self.record_error(TypeError::TensorTypeNotInferable {
                    ctor: ctor.name.clone(),
                    span,
                });
                None
            }
        }
    }

    /// Report a constructor whose argument count is wrong, and say whether the rest of
    /// the checking may proceed against `args`.
    fn check_tensor_ctor_arity(&mut self, args: &[Expr], expected: usize, span: Span) -> bool {
        if args.len() == expected {
            return true;
        }
        self.record_error(TypeError::ArgumentCountMismatch {
            expected,
            found: args.len(),
            span,
        });
        for arg in args {
            self.check_expr(arg, None);
        }
        false
    }

    /// Check every constructor argument at the tensor's element type.
    fn check_tensor_ctor_args(&mut self, args: &[Expr], element: &Type) {
        for arg in args {
            let Some(found) = self.check_expr(arg, Some(element)) else {
                continue;
            };
            if !matches!(found, Type::Unknown) && !found.is_compatible_with(element) {
                self.record_error(TypeError::Mismatch {
                    expected: element.clone(),
                    found,
                    span: arg.span(),
                });
            }
        }
    }

    fn reject_tensor_ctor(&mut self, ctor: &Identifier, ty: &Type, reason: &str, span: Span) {
        self.record_error(TypeError::TensorConstructorNotApplicable {
            ctor: ctor.name.clone(),
            ty: ty.clone(),
            reason: reason.to_string(),
            span,
        });
    }
}
