// The shape-manipulation methods on a tensor: `.t()`, `.reshape([...])`,
// `.permute([...])`, and `.flatten()` / `.flatten([...])`, and the narrowing `.as_shape::<[...]>()`.
//
// Reached from the builtin-method arm of `check_call_expr`. What these four have in
// common is that their arguments are read at compile time and never as values: a
// `.reshape` extent is folded here, and a `.permute` axis may be written as a dimension
// NAME, which is resolved against the receiver's own shape rather than the surrounding
// value scope. So the arguments are inspected as syntax and never handed to `check_expr`,
// which would look an axis name up as a variable and not find one.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, ExtentExpr, TensorAxis, Type, shape_count};
use ast_types::{BinaryOp, Expr, GenericArg, TensorExtent, UnaryOp};
use shared_types::{Literal, Span};

/// The rank-2 transpose `matrix.t()`.
pub(crate) const TRANSPOSE_METHOD: &str = "t";
/// `tensor.reshape([d0, ...])`, with `-1` in at most one position.
pub(crate) const RESHAPE_METHOD: &str = "reshape";
/// `tensor.permute([...])`, whose entries name every axis exactly once.
pub(crate) const PERMUTE_METHOD: &str = "permute";
/// `tensor.flatten()` / `tensor.flatten(dims: [...])`.
pub(crate) const FLATTEN_METHOD: &str = "flatten";
/// `tensor.as_shape::<[...]>()`, the run-time narrowing to a written shape.
pub(crate) const AS_SHAPE_METHOD: &str = "as_shape";

/// The `-1` an extent may be written as to have it inferred from the others.
const INFERRED_EXTENT: i128 = -1;

/// Whether `method` names one of the four shape-manipulation intrinsics.
pub(crate) fn is_shape_method(method: &str) -> bool {
    matches!(
        method,
        TRANSPOSE_METHOD | RESHAPE_METHOD | PERMUTE_METHOD | FLATTEN_METHOD
    )
}

impl TypeChecker {
    /// Type-check `.as_shape::<[...]>()` on a tensor, returning `Option<Tensor<T, [...]>>`.
    ///
    /// The receiver is consumed, so it must be an owned tensor. The written shape has the
    /// receiver's rank, and each axis is a literal, an extent parameter, `?`, or `?B`. A
    /// name not in scope is fresh: it takes the receiver's extent at its axis and is in
    /// scope after the expression, whatever the narrowing answers. Every other axis is
    /// compared with the receiver's extent at run time, and the answer is `None` when one
    /// disagrees.
    pub(crate) fn check_as_shape(
        &mut self,
        object: &Expr,
        recv: &Type,
        type_args: &[GenericArg],
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        self.check_call_args(args, &[], call_span);
        let Type::Tensor { element, shape } = recv.referent() else {
            return Type::Unknown;
        };
        if matches!(recv, Type::Reference { .. }) {
            self.record_error(TypeError::TensorAsShapeBorrowed {
                ty: recv.clone(),
                span: call_span,
            });
            return Type::Unknown;
        }
        let [GenericArg::Shape { dims, .. }] = type_args else {
            self.record_error(TypeError::TurbofishCountMismatch {
                name: AS_SHAPE_METHOD.to_string(),
                expected: 1,
                found: type_args.len(),
                span: call_span,
            });
            return Type::Unknown;
        };
        if dims
            .iter()
            .any(|dim| matches!(dim.extent, TensorExtent::Splice(_) | TensorExtent::Expr(_)))
        {
            self.record_error(TypeError::TensorAsShapeEntry { span: call_span });
            return Type::Unknown;
        }
        let Some(written) = self.resolve_tensor_shape_in(dims, true) else {
            return Type::Unknown;
        };
        if written.len() != shape.len() {
            self.record_error(TypeError::TensorAsShapeRank {
                expected: shape.len(),
                found: written.len(),
                span: call_span,
            });
            return Type::Unknown;
        }

        // A name the narrowing introduces is bound to the receiver's extent at its axis, the
        // first occurrence doing the binding and any later one checking at run time.
        for (axis, source) in written.iter().zip(shape.iter()) {
            let ArrayLen::Dynamic(Some(name)) = &axis.extent else {
                continue;
            };
            if !self.dynamic_name_in_scope(name) {
                self.introduce_dynamic_name(name, Some(&source.extent), call_span);
            }
        }
        self.record_move(object);
        let result = Type::Tensor {
            element: element.clone(),
            shape: written,
        };
        let result = self.canonicalize_dynamic(&result, call_span);
        self.option_of(result, call_span)
    }

    /// Type-check one of the four shape-manipulation methods and return the tensor type
    /// it produces. The receiver is consumed: the one buffer moves into the result.
    pub(crate) fn check_tensor_shape_method(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        object: &Expr,
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        let Some(result) = self.resolve_result_shape(shape, method, args, call_span) else {
            return Type::Unknown;
        };
        self.record_move(object);
        Type::Tensor {
            element: Box::new(element.clone()),
            shape: result,
        }
    }

    /// The shape a shape-manipulation call produces, or `None` when a diagnostic was
    /// recorded instead. How the elements move is the backend's concern and is
    /// re-derived during lowering; the checker only has to name the result type.
    fn resolve_result_shape(
        &mut self,
        shape: &[TensorAxis],
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        match method {
            TRANSPOSE_METHOD => self.resolve_transpose(shape, args, call_span),
            RESHAPE_METHOD => self.resolve_reshape(shape, args, call_span),
            PERMUTE_METHOD => self.resolve_permute(shape, args, call_span),
            FLATTEN_METHOD => self.resolve_flatten(shape, args, call_span),
            _ => None,
        }
    }

    /// `.t()`: the rank-2 transpose. Both axes keep their dimension names, which is what
    /// makes a transposed `[height: H, width: W]` land as `[width: W, height: H]` and
    /// still be rejected where the original was expected.
    fn resolve_transpose(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        self.check_shape_arity(args, 0, call_span);
        if shape.len() != 2 {
            self.record_error(TypeError::TensorTransposeRank {
                rank: shape.len(),
                span: call_span,
            });
            return None;
        }
        Some(vec![shape[1].clone(), shape[0].clone()])
    }

    /// `.reshape([...])`: the same elements, re-described. Dimension names are dropped:
    /// a new extent is not the axis the old name documented.
    ///
    /// Over extent parameters the counts are compared here only when they are equal or
    /// both concrete: `[N, 2]` into `[N, 3]` holds at `N = 0`, so whether it holds at all
    /// is a question for each instance, which lowering answers. A dynamic axis of the
    /// receiver, or an entry naming a dynamic extent in scope, leaves the count to the
    /// run-time check: such an entry is the result's axis of that name, and a `-1` that
    /// cannot be divided out here is an anonymous `?`.
    fn resolve_reshape(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        let entries = self.shape_arg_entries(args, RESHAPE_METHOD, "-1", call_span)?;
        let total = shape_count(shape);

        let mut parts: Vec<ReshapePart> = Vec::with_capacity(entries.len());
        let mut inferred_at: Option<usize> = None;
        for entry in entries {
            if let Expr::Identifier(ident) = entry
                && self.dynamic_name_in_scope(&ident.name)
            {
                parts.push(ReshapePart::Named(self.canonical_dynamic_name(&ident.name)));
                continue;
            }
            let extent = self.reshape_extent(entry, CONST_FOLD_DEPTH);
            let value = extent.as_ref().and_then(ExtentExpr::as_constant);
            if value == Some(INFERRED_EXTENT) {
                if inferred_at.is_some() {
                    self.record_error(TypeError::TensorReshapeRepeatedInference {
                        span: entry.span(),
                    });
                    return None;
                }
                inferred_at = Some(parts.len());
                parts.push(ReshapePart::Inferred);
                continue;
            }
            let Some(extent) = extent.filter(|_| value.is_none_or(|value| value >= 0)) else {
                self.record_error(TypeError::TensorReshapeExtentNotConstant { span: entry.span() });
                return None;
            };
            parts.push(ReshapePart::Known(extent));
        }

        let runtime_count = total.is_none()
            || parts
                .iter()
                .any(|part| matches!(part, ReshapePart::Named(_)));
        if runtime_count {
            return Self::reshape_axes(parts, None);
        }
        let total = total?;

        let known = parts
            .iter()
            .filter_map(|part| match part {
                ReshapePart::Known(extent) => Some(extent),
                ReshapePart::Named(_) | ReshapePart::Inferred => None,
            })
            .try_fold(ExtentExpr::constant(1), |product, extent| {
                product.checked_mul(extent)
            });
        let Ok(known) = known else {
            self.record_error(TypeError::TensorReshapeExtentNotConstant { span: call_span });
            return None;
        };
        let concrete = known.as_constant().zip(total.as_constant());
        if inferred_at.is_some() {
            // A zero-extent axis beside a `-1` leaves nothing to divide by.
            let divides = concrete.is_none_or(|(known, total)| known != 0 && total % known == 0);
            let inferred = total.quot(&known).ok().filter(|_| divides);
            let Some(inferred) = inferred else {
                self.record_error(TypeError::TensorReshapeIndivisible {
                    known: known.to_string(),
                    total: total.to_string(),
                    span: call_span,
                });
                return None;
            };
            return Self::reshape_axes(parts, Some(inferred));
        } else if let Some((known, total)) = concrete
            && known != total
        {
            self.record_error(TypeError::TensorReshapeElementCount {
                expected: total as usize,
                found: known as usize,
                span: call_span,
            });
            return None;
        }
        Self::reshape_axes(parts, None)
    }

    /// The result axes of a reshape whose counts were settled: the inferred `-1`, when the
    /// counts divided it out, is `inferred`, and a `-1` they could not is `?`.
    fn reshape_axes(
        parts: Vec<ReshapePart>,
        inferred: Option<ExtentExpr>,
    ) -> Option<Vec<TensorAxis>> {
        let mut inferred = inferred;
        parts
            .into_iter()
            .map(|part| {
                let extent = match part {
                    ReshapePart::Known(extent) => ArrayLen::from_extent(extent)?,
                    ReshapePart::Named(name) => ArrayLen::Dynamic(Some(name)),
                    ReshapePart::Inferred => match inferred.take() {
                        Some(extent) => ArrayLen::from_extent(extent)?,
                        None => ArrayLen::Dynamic(None),
                    },
                };
                Some(TensorAxis { name: None, extent })
            })
            .collect()
    }

    /// `.permute([...])`: every axis named exactly once, in the order the result wants
    /// them. Each entry is a dimension name or a positional index.
    fn resolve_permute(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        let entries = self.shape_arg_entries(args, PERMUTE_METHOD, "1, 0", call_span)?;
        if entries.len() != shape.len() {
            self.record_error(TypeError::TensorPermuteRank {
                found: entries.len(),
                rank: shape.len(),
                span: call_span,
            });
            return None;
        }
        let order = self.resolve_axis_list(shape, entries, PERMUTE_METHOD)?;
        Some(order.iter().map(|axis| shape[*axis].clone()).collect())
    }

    /// `.flatten()` merges every axis; `.flatten(dims: [...])` merges the named run.
    ///
    /// A merged axis loses its name: it is no longer the axis either name documented.
    /// The run has to be adjacent, because merging non-neighbouring axes would move
    /// elements, and flattening is specified as a re-description of the same order. A run
    /// holding a dynamic axis has a run-time count, so the merged axis is an anonymous `?`.
    fn resolve_flatten(
        &mut self,
        shape: &[TensorAxis],
        args: &[Expr],
        call_span: Span,
    ) -> Option<Vec<TensorAxis>> {
        if args.is_empty() {
            return Some(vec![TensorAxis {
                name: None,
                extent: Self::merged_extent(shape),
            }]);
        }

        let entries = self.shape_arg_entries(args, FLATTEN_METHOD, "rows, cols", call_span)?;
        if entries.is_empty() {
            self.record_error(TypeError::TensorFlattenNoAxes { span: call_span });
            return None;
        }
        let axes = self.resolve_axis_list(shape, entries, FLATTEN_METHOD)?;
        let first = axes[0];
        if axes
            .iter()
            .enumerate()
            .any(|(step, axis)| *axis != first + step)
        {
            self.record_error(TypeError::TensorFlattenNotAdjacent { span: call_span });
            return None;
        }

        let run: Vec<TensorAxis> = axes.iter().map(|axis| shape[*axis].clone()).collect();
        let mut result: Vec<TensorAxis> = shape[..first].to_vec();
        result.push(TensorAxis {
            name: None,
            extent: Self::merged_extent(&run),
        });
        result.extend_from_slice(&shape[first + axes.len()..]);
        Some(result)
    }

    /// The extent a run of axes merges into: its element count as a number, or `?` when
    /// one of its axes is dynamic and the count is only known at run time.
    fn merged_extent(run: &[TensorAxis]) -> ArrayLen {
        shape_count(run)
            .and_then(ArrayLen::from_extent)
            .unwrap_or(ArrayLen::Dynamic(None))
    }

    /// Turn each entry of an axis list into a receiver axis index, reporting an unknown
    /// name, an out-of-range index, or a repeat.
    fn resolve_axis_list(
        &mut self,
        shape: &[TensorAxis],
        entries: &[Expr],
        method: &str,
    ) -> Option<Vec<usize>> {
        let mut axes = Vec::with_capacity(entries.len());
        for entry in entries {
            let axis = self.resolve_shape_axis(shape, entry, method)?;
            if axes.contains(&axis) {
                self.record_error(TypeError::TensorAxisRepeated {
                    method: method.to_string(),
                    axis,
                    span: entry.span(),
                });
                return None;
            }
            axes.push(axis);
        }
        Some(axes)
    }

    /// One axis-list entry: a bare identifier is a dimension name, an integer literal a
    /// positional index.
    ///
    /// An identifier here is ALWAYS a dimension name: the names live in a
    /// namespace attached to the tensor type, so a local binding of the same name
    /// neither shadows the axis nor is shadowed by it.
    fn resolve_shape_axis(
        &mut self,
        shape: &[TensorAxis],
        entry: &Expr,
        method: &str,
    ) -> Option<usize> {
        if let Expr::Identifier(ident) = entry {
            let found = shape
                .iter()
                .position(|axis| axis.name.as_deref() == Some(ident.name.as_str()));
            if found.is_none() {
                self.record_error(TypeError::UnknownTensorAxisName {
                    name: ident.name.clone(),
                    declared: declared_names(shape),
                    span: entry.span(),
                });
            }
            return found;
        }
        let Some(value) = const_integer(entry) else {
            self.record_error(TypeError::TensorShapeArgNotLiteral {
                method: method.to_string(),
                example: "0, 1".to_string(),
                span: entry.span(),
            });
            return None;
        };
        let axis = usize::try_from(value).ok().filter(|a| *a < shape.len());
        if axis.is_none() {
            self.record_error(TypeError::TensorAxisOutOfRange {
                axis: value.unsigned_abs() as usize,
                rank: shape.len(),
                span: entry.span(),
            });
        }
        axis
    }

    /// The single array-literal argument these calls take, as its element expressions.
    fn shape_arg_entries<'a>(
        &mut self,
        args: &'a [Expr],
        method: &str,
        example: &str,
        call_span: Span,
    ) -> Option<&'a [Expr]> {
        if !self.check_shape_arity(args, 1, call_span) {
            return None;
        }
        match &args[0] {
            Expr::ArrayLiteral { elements, .. } => Some(elements),
            other => {
                self.record_error(TypeError::TensorShapeArgNotLiteral {
                    method: method.to_string(),
                    example: example.to_string(),
                    span: other.span(),
                });
                None
            }
        }
    }

    /// Report an argument-count mismatch, returning whether the count was right.
    fn check_shape_arity(&mut self, args: &[Expr], expected: usize, call_span: Span) -> bool {
        if args.len() == expected {
            return true;
        }
        self.record_error(TypeError::ArgumentCountMismatch {
            expected,
            found: args.len(),
            span: call_span,
        });
        false
    }

    /// A `.reshape` extent's value. The spec calls the list "a constant-expression
    /// array", so beyond a literal an extent may name a `const` or an extent parameter in
    /// scope, or combine them with arithmetic. A local binding shadows both and is a
    /// run-time value, so it does not fold. `None` when the extent is not a constant.
    fn reshape_extent(&self, expr: &Expr, depth: u32) -> Option<ExtentExpr> {
        let depth = depth.checked_sub(1)?;
        match expr {
            Expr::Literal(Literal::Integer(value, _), _) => Some(ExtentExpr::constant(*value)),
            Expr::Paren(inner, _) => self.reshape_extent(inner, depth),
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
                ..
            } => ExtentExpr::constant(0)
                .checked_sub(&self.reshape_extent(operand, depth)?)
                .ok(),
            Expr::Identifier(ident) => {
                if self.symbols.lookup(&ident.name).is_some() {
                    return None;
                }
                let extent_param = !self.value_consts.contains(&ident.name)
                    && !matches!(
                        self.const_scope.get(&ident.name),
                        None | Some(Type::Shape(_))
                    );
                if extent_param {
                    return Some(ExtentExpr::param(&ident.name));
                }
                self.reshape_extent(self.constant_values.get(&ident.name)?, depth)
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let l = self.reshape_extent(left, depth)?;
                let r = self.reshape_extent(right, depth)?;
                match op {
                    BinaryOp::Add => l.checked_add(&r),
                    BinaryOp::Subtract => l.checked_sub(&r),
                    BinaryOp::Multiply => l.checked_mul(&r),
                    BinaryOp::Divide => l.quot(&r),
                    BinaryOp::Modulo => l.rem(&r),
                    _ => return None,
                }
                .ok()
            }
            _ => None,
        }
    }
}

/// One entry of a `.reshape` list, once read: a count folded here, a dynamic name in scope,
/// or the `-1` the others are divided into.
enum ReshapePart {
    Known(ExtentExpr),
    Named(String),
    Inferred,
}

/// How many constants deep an extent folds. A constant naming itself through a cycle
/// would otherwise never bottom out.
const CONST_FOLD_DEPTH: u32 = 64;

/// The value of an integer constant expression written in an axis list: a literal, or a
/// literal under a negation (`-1`) or parentheses.
pub(super) fn const_integer(expr: &Expr) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Paren(inner, _) => const_integer(inner),
        Expr::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => const_integer(operand).map(|value| -value),
        _ => None,
    }
}

/// The dimension names a shape declares, for the diagnostic that names them: naming a
/// dimension the type does not declare lists the ones it does.
pub(super) fn declared_names(shape: &[TensorAxis]) -> String {
    let names: Vec<&str> = shape
        .iter()
        .filter_map(|axis| axis.name.as_deref())
        .collect();
    if names.is_empty() {
        return "no dimension names".to_string();
    }
    names.join(", ")
}
