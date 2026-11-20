// Tensor value construction and in-place update: nested-array-literal coercion, the six
// `Tensor::<T, [...]>::ctor(...)` construction helpers, and the `*Assign` compound
// assignment path.
//
// Reached from the array-literal arm of `check_expr`, the associated-call arm of
// `check_call_expr`, and the compound-assignment arm of `check_stmt`. Adds methods to
// the same `impl TypeChecker` block as the rest of `type_checkers`.

use super::TypeChecker;
use super::resolution::NameSlot;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type, spliced_param};
use ast_types::{BinaryOp, Expr, GenericArg, Place};
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

    /// Report `operation` when any axis of `shape` is dynamic, and answer whether it did.
    ///
    /// The operations that build a buffer from literal data (a tensor literal, `Tensor::from`,
    /// a safetensors read or write) need every extent as a number, and funnel through here. A
    /// dynamic axis is otherwise read at run time, and widening a static tensor into a
    /// `?`-shaped type is what the axis is for, so that stays legal.
    pub(crate) fn reject_dynamic_extent(
        &mut self,
        shape: &[TensorAxis],
        operation: &str,
        ty: &Type,
        span: Span,
    ) -> bool {
        if !shape.iter().any(|axis| axis.extent.is_dynamic()) {
            return false;
        }
        self.record_error(TypeError::TensorDynamicExtent {
            operation: operation.to_string(),
            ty: ty.clone(),
            span,
        });
        true
    }

    /// Reject `operation` on a tensor whose shape splices in a shape parameter, reporting
    /// whether it did. Such a shape has no rank until the instantiation, so an operation
    /// that counts or picks axes has nothing to count.
    pub(crate) fn reject_splice(
        &mut self,
        shape: &[TensorAxis],
        operation: &str,
        ty: &Type,
        span: Span,
    ) -> bool {
        let Some(param) = spliced_param(shape) else {
            return false;
        };
        self.record_error(TypeError::TensorSpliceRank {
            operation: operation.to_string(),
            ty: ty.clone(),
            param: param.to_string(),
            span,
        });
        true
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
        shape: &[TensorAxis],
        span: Span,
    ) -> Type {
        let tensor = Type::Tensor {
            element: Box::new(element_ty.clone()),
            shape: shape.to_vec(),
        };
        if self.reject_dynamic_extent(shape, "a tensor literal", &tensor, span) {
            return tensor;
        }
        // A rank-0 tensor holds exactly one value and has no axis to write elements
        // along, so there is no array literal that could denote one.
        let Some((axis, rest)) = shape.split_first() else {
            self.record_error(TypeError::TensorScalarNeedsConstructor { span });
            return tensor;
        };
        // A shape parameter's extent is only known at the instantiation, and a literal
        // is written once for every one of them, so there is no length this could be
        // checked against. The constructors take the extent from the type instead.
        let ArrayLen::Fixed(extent) = &axis.extent else {
            self.record_error(TypeError::TensorLiteralSymbolicExtent {
                name: axis.extent.to_string(),
                span,
            });
            return tensor;
        };
        if elements.len() != *extent {
            self.record_error(TypeError::TensorExtentMismatch {
                expected: *extent,
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
        remaining_shape: &[TensorAxis],
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
    ///
    /// The open axes of the type (a `?`, or a dynamic name not yet in scope) take their
    /// run-time extents from `dims:`, one entry per open axis, in axis order with a name
    /// written twice taking one entry. A name the constructor binds is in scope afterwards.
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
        let shape = shape.clone();

        // A literal's buffer is sized by the literal itself, so `from` takes no run-time
        // extent: its argument is the nested array literal, never a `dims:` list.
        if ctor.name == CTOR_FROM {
            if self.reject_dynamic_extent(
                &shape,
                &format!("`Tensor::{}`", ctor.name),
                &tensor,
                span,
            ) {
                return tensor;
            }
            if self.check_tensor_ctor_arity(&args.iter().collect::<Vec<_>>(), 1, span) {
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
            return tensor;
        }

        let (fresh, open) = self.open_axes(&shape);
        let (positional, dims) = split_dims_argument(args);
        self.check_tensor_dims(ctor, open, dims, span);

        match ctor.name.as_str() {
            CTOR_ZEROS | CTOR_ONES => {
                self.check_tensor_ctor_arity(&positional, 0, span);
            }
            CTOR_IDENTITY => {
                self.check_tensor_ctor_arity(&positional, 0, span);
                // The EXTENTS decide squareness; a `[row: 4, col: 4]` identity is
                // square even though its two axes carry different names. An anonymous
                // `?` equals nothing, not even another `?`, so `[?, ?]` is not square.
                let square = matches!(shape.as_slice(), [rows, cols] if rows.extent.aligns_with(&cols.extent));
                if !square {
                    self.reject_tensor_ctor(
                        ctor,
                        &tensor,
                        "an identity matrix is square and rank 2, and a `?` axis is square only with a `?B` of the same name on the other axis",
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
                if self.check_tensor_ctor_arity(&positional, 2, span) {
                    self.check_tensor_ctor_args(&positional, &element);
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
                if self.check_tensor_ctor_arity(&positional, 1, span) {
                    self.check_tensor_ctor_args(&positional, &element);
                }
            }
            _ => unreachable!("guarded by is_tensor_constructor"),
        }
        // The names the constructor binds are in scope from here on, whatever it reported.
        for name in fresh {
            self.introduce_dynamic_name(&name, None, span);
        }
        tensor
    }

    /// The open axes of a constructed shape: how many `dims:` entries it takes, and the
    /// dynamic names among them that are not in scope yet, each once and in order.
    fn open_axes(&self, shape: &[TensorAxis]) -> (Vec<String>, usize) {
        let mut fresh: Vec<String> = Vec::new();
        let mut open = 0;
        for axis in shape {
            match &axis.extent {
                ArrayLen::Dynamic(None) => open += 1,
                ArrayLen::Dynamic(Some(name))
                    if !self.dynamic_name_in_scope(name) && !fresh.contains(name) =>
                {
                    fresh.push(name.clone());
                    open += 1;
                }
                _ => {}
            }
        }
        (fresh, open)
    }

    /// Check a constructor's `dims:` against the open axes it has: one `u64` entry per open
    /// axis, and none where the shape is closed.
    fn check_tensor_dims(
        &mut self,
        ctor: &Identifier,
        open: usize,
        dims: Option<(&[Expr], Span)>,
        span: Span,
    ) {
        let Some((entries, dims_span)) = dims else {
            if open > 0 {
                self.record_error(TypeError::TensorDimsCount {
                    ctor: ctor.name.clone(),
                    expected: open,
                    found: 0,
                    span,
                });
            }
            return;
        };
        let entries: Vec<&Expr> = entries.iter().collect();
        self.check_tensor_ctor_args(&entries, &Type::U64);
        if open == 0 {
            self.record_error(TypeError::TensorDimsWithoutOpenAxis {
                ctor: ctor.name.clone(),
                span: dims_span,
            });
        } else if entries.len() != open {
            self.record_error(TypeError::TensorDimsCount {
                ctor: ctor.name.clone(),
                expected: open,
                found: entries.len(),
                span: dims_span,
            });
        }
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
            return self.resolve_type_naming(annotation, NameSlot::Top);
        }
        match expected {
            Some(ty @ Type::Tensor { shape, .. }) => {
                // Lowering sees the expected type with its names erased, so a bound name
                // there would build an axis it cannot size.
                let bound = shape.iter().find_map(|axis| match &axis.extent {
                    ArrayLen::Dynamic(Some(name)) if self.dynamic_name_in_scope(name) => {
                        Some(name.clone())
                    }
                    _ => None,
                });
                if let Some(name) = bound {
                    self.record_error(TypeError::TensorCtorBoundNameInferred {
                        ctor: ctor.name.clone(),
                        name,
                        expected: ty.clone(),
                        turbofish: ty.to_string().replacen("Tensor<", "Tensor::<", 1),
                        span,
                    });
                    return None;
                }
                Some(ty.clone())
            }
            _ => {
                // `scalar` builds only the rank-0 tensor, so a `[3, 3]` example would
                // suggest a turbofish it then refuses.
                let shape = if ctor.name == CTOR_SCALAR {
                    "[]"
                } else {
                    "[3, 3]"
                };
                self.record_error(TypeError::TensorTypeNotInferable {
                    ctor: ctor.name.clone(),
                    shape,
                    span,
                });
                None
            }
        }
    }

    /// Report a constructor whose argument count is wrong, and say whether the rest of
    /// the checking may proceed against `args`.
    fn check_tensor_ctor_arity(&mut self, args: &[&Expr], expected: usize, span: Span) -> bool {
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
    fn check_tensor_ctor_args(&mut self, args: &[&Expr], element: &Type) {
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

    /// Check `target OP= value` where `target` is a tensor: the in-place `*Assign`
    /// path of the operator-trait dispatch rule.
    ///
    /// The language fixes the evaluation order: the right-hand side is evaluated first,
    /// and only then is the target borrowed mutably. That is what makes the canonical
    /// weight update writable at all, since the shared read of the weight finishes
    /// before the exclusive update begins, so the checks below run in that order too.
    pub(crate) fn check_tensor_compound_assign(
        &mut self,
        place: &Place,
        tensor_ty: &Type,
        op: BinaryOp,
        value: &Expr,
        span: Span,
    ) -> Option<()> {
        let Type::Tensor { element, .. } = tensor_ty else {
            return None;
        };

        let element_ty = (**element).clone();
        let element = &element_ty;

        // The update rewrites the buffer every live borrow of the target reads, so a
        // borrowed binding refuses it exactly as it refuses `=`. Tested before the RHS, like
        // `=`: the `w.grad()` in `w -= lr * w.grad()` is a borrow this update does not
        // conflict with.
        self.refuse_store_while_borrowed(place);

        // The element carries the arithmetic: `bool` has none. A half-precision element
        // does, although its scalar does not: the scalar restriction stops at tensor ops.
        if !self.is_arithmetic(element) {
            self.record_error(TypeError::TensorElementNotArithmetic {
                op: format!("{op}="),
                element: element.clone(),
                span,
            });
            return None;
        }

        // The right-hand side is typed by the ELEMENT, not by the target: `w *= 2.0` is
        // the scalar broadcast, and a tensor operand ignores the expectation anyway.
        let value_ty = self
            .check_expr(value, Some(element))
            .unwrap_or(Type::Unknown);
        // A borrowed operand is read rather than consumed, which is what lets a weight
        // be updated from a tensor the caller still owns.
        let borrowed = matches!(value_ty, Type::Reference { .. });
        if !matches!(value_ty, Type::Unknown)
            && !self.compound_assign_operand_fits(&value_ty, tensor_ty, op, span)
        {
            return None;
        }
        if !borrowed {
            self.record_move(value);
        }

        // The place is read as well as written, so a right-hand side that moved its
        // root out (`w += w`) leaves nothing to update in place.
        let root = place.root()?;
        let symbol_info = self.symbols.lookup(&root.name)?;
        if let Some(moved_at) = symbol_info.moves.conflict_with_whole() {
            self.record_error(TypeError::UseOfMovedValue {
                name: root.name.clone(),
                span: root.span,
                moved_at,
            });
            return None;
        }
        Some(())
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

/// Split a constructor's arguments into its `dims:` list, the one array literal among them,
/// and the rest, which are the value arguments (`mean`, `std`, the fill value).
fn split_dims_argument(args: &[Expr]) -> (Vec<&Expr>, Option<(&[Expr], Span)>) {
    let dims_at = args
        .iter()
        .position(|arg| matches!(arg, Expr::ArrayLiteral { .. }));
    let dims = dims_at.and_then(|at| match &args[at] {
        Expr::ArrayLiteral { elements, span } => Some((elements.as_slice(), *span)),
        _ => None,
    });
    let positional = args
        .iter()
        .enumerate()
        .filter_map(|(at, arg)| (Some(at) != dims_at).then_some(arg))
        .collect();
    (positional, dims)
}
