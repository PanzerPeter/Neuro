//! The shape-manipulation methods `.t()`, `.reshape(...)`, `.permute(...)`, and
//! `.flatten(...)`.
//!
//! Reached from the method-call arm of `lower_expr`, before the arguments are lowered:
//! these calls read their arguments as syntax, and a `.permute` axis may be a dimension
//! NAME, which resolves against the receiver's own shape and would be an unresolved
//! variable if it reached `lower_expr`. The receiver's names ride on its
//! [`neuro_hir::HirType::Tensor`], which is why the HIR carries them at all.
//!
//! The type checker has already validated every one of these calls, so a shape that does
//! not work out here is a divergence between the two surfaces and becomes a
//! `LoweringError`, exactly as the governing rule in this slice's CONTEXT.md requires.
//!
//! A dynamic axis keeps its `None` in the result, and the result's run-time extents are
//! computed from the receiver's (`HirExtent`, operand 0). A reshape on a dynamic receiver
//! checks its element count at run time before the cast, as R9 requires.

use std::collections::HashMap;

use ast_types::{BinaryOp, Expr, UnaryOp};
use neuro_hir::{
    AxisNames, HirExpr, HirExprKind, HirExtent, HirFunction, HirItem, HirParam, HirStmt, HirTarget,
    HirType,
};
use shared_types::{Literal, Span};

use crate::extents::{
    binary, borrow, extent_read, guard, panic_with, shown, text, u64_value, variable,
};
use crate::{Lowerer, LoweringError, instance_shape, malformed};

pub(crate) const TRANSPOSE_METHOD: &str = "t";
pub(crate) const RESHAPE_METHOD: &str = "reshape";
pub(crate) const PERMUTE_METHOD: &str = "permute";
pub(crate) const FLATTEN_METHOD: &str = "flatten";

/// The `-1` an extent may be written as to have it inferred from the others.
const INFERRED_EXTENT: i128 = -1;

/// How many constants deep an extent folds; the checker uses the same bound.
const CONST_FOLD_DEPTH: u32 = 64;

/// The prefix of the host function a reshape over a dynamic name is outlined into.
const RESHAPE_PREFIX: &str = "__reshape_";

/// Whether `method` names one of the four shape-manipulation intrinsics.
pub(crate) fn is_shape_method(method: &str) -> bool {
    matches!(
        method,
        TRANSPOSE_METHOD | RESHAPE_METHOD | PERMUTE_METHOD | FLATTEN_METHOD
    )
}

/// The result shape of a cast, where each axis came from when the element order changes,
/// and the run-time extent of each dynamic result axis.
struct ShapeCast {
    shape: Vec<Option<usize>>,
    names: AxisNames,
    permutation: Option<Vec<usize>>,
    dims: Vec<HirExtent>,
}

/// One entry of a `.reshape` axis list, as the run-time count check reads it.
enum ReshapeEntry {
    /// A value known at this instance: a literal, an extent parameter, or a constant.
    Known(u64),
    /// The `-1`, inferred from the receiver's count.
    Inferred,
    /// A dynamic extent name in scope: the axis is `?B` and its value is the local `B`.
    Named(String),
}

impl Lowerer {
    /// Lower one shape-manipulation call into a [`HirExprKind::TensorShapeCast`].
    pub(crate) fn lower_tensor_shape_cast(
        &mut self,
        receiver: HirExpr,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let HirType::Tensor {
            element,
            shape,
            names,
        } = receiver.ty.referent().clone()
        else {
            return Err(malformed(format!(
                "`.{method}` reached lowering on a non-tensor receiver"
            )));
        };

        if method == RESHAPE_METHOD && self.reshape_is_dynamic(&shape, args) {
            return self.dynamic_reshape(receiver, element, &shape, args, span);
        }
        let cast = match method {
            TRANSPOSE_METHOD => transpose(&shape, &names),
            RESHAPE_METHOD => {
                let shape = crate::static_extents(&shape)?;
                let cast = reshape(
                    &shape,
                    args,
                    &|expr| self.reshape_extent(expr, CONST_FOLD_DEPTH),
                    span,
                )?;
                ShapeCast {
                    shape: cast.shape,
                    names: cast.names,
                    permutation: None,
                    dims: Vec::new(),
                }
            }
            PERMUTE_METHOD => permute(&shape, &names, args)?,
            FLATTEN_METHOD => flatten(&shape, &names, args)?,
            other => return Err(malformed(format!("`.{other}` is not a shape method"))),
        };

        Ok(HirExpr::new(
            HirExprKind::TensorShapeCast {
                receiver: Box::new(receiver),
                permutation: cast.permutation,
                dims: cast.dims,
            },
            HirType::Tensor {
                element,
                shape: cast.shape,
                names: cast.names,
            },
            span,
        ))
    }

    /// Whether a `.reshape` needs the run-time count check: a dynamic receiver, or an entry
    /// naming a dynamic extent in scope.
    fn reshape_is_dynamic(&self, shape: &[Option<usize>], args: &[Expr]) -> bool {
        shape.iter().any(Option::is_none)
            || axis_list(args, RESHAPE_METHOD)
                .is_ok_and(|entries| entries.iter().any(|entry| self.names_extent(entry)))
    }

    fn names_extent(&self, entry: &Expr) -> bool {
        matches!(entry, Expr::Identifier(ident) if self.extent_in_scope(&ident.name))
    }

    /// `.reshape` of a dynamic receiver, or into a dynamic name. The element count of the
    /// receiver is compared with the target's at run time, panicking if they differ, and the
    /// cast's dynamic result axes are their extents. The receiver is bound first so the check
    /// and the cast read the same tensor.
    ///
    /// A dynamic name's value is a local, which a `HirExtent` cannot read. So a reshape that
    /// names one becomes a host function taking each name's value as an integer operand, whose
    /// body is the cast: a name's axis is `Scalar` of its operand, and a `-1` is the receiver's
    /// count over the product of the other entries.
    fn dynamic_reshape(
        &mut self,
        receiver: HirExpr,
        element: Box<HirType>,
        shape: &[Option<usize>],
        args: &[Expr],
        span: Span,
    ) -> Result<HirExpr, LoweringError> {
        let entries = axis_list(args, RESHAPE_METHOD)?;
        let mut parsed = Vec::with_capacity(entries.len());
        for entry in entries {
            parsed.push(self.reshape_entry(entry)?);
        }
        if parsed
            .iter()
            .filter(|entry| matches!(entry, ReshapeEntry::Inferred))
            .count()
            > 1
        {
            return Err(malformed(
                "`.reshape` reached lowering with two `-1` extents".to_string(),
            ));
        }
        // The dynamic names, in entry order: the operand each one is passed as, after the
        // receiver.
        let named: Vec<String> = parsed
            .iter()
            .filter_map(|entry| match entry {
                ReshapeEntry::Named(name) => Some(name.clone()),
                _ => None,
            })
            .collect();

        let source = self.fresh_extent_temp();
        let receiver_ty = receiver.ty.clone();
        let count = count_expr(&variable(&source, receiver_ty.clone(), span), shape, span);
        let total = count_extent(shape);

        // The product of the entries that are not inferred, as a value and as an extent.
        let known: u64 = parsed
            .iter()
            .filter_map(|entry| match entry {
                ReshapeEntry::Known(value) => Some(*value),
                _ => None,
            })
            .product();
        let mut divisor = u64_value(known, span);
        let mut divisor_extent = HirExtent::Constant(known);
        for (position, name) in named.iter().enumerate() {
            let local = variable(&crate::extents::local_name(name), HirType::U64, span);
            divisor = binary(BinaryOp::Multiply, divisor, local, HirType::U64, span);
            divisor_extent = HirExtent::Mul(
                Box::new(divisor_extent),
                Box::new(HirExtent::Scalar(position + 1)),
            );
        }
        let inferred = parsed
            .iter()
            .any(|entry| matches!(entry, ReshapeEntry::Inferred));

        // The cast's result: a literal axis keeps its value; a dynamic name's axis is its
        // operand; a `-1` is the count quotient, zero where no other extent is known.
        let mut result_shape = Vec::with_capacity(parsed.len());
        let mut dims = Vec::new();
        let mut named_at = 0;
        for entry in &parsed {
            match entry {
                ReshapeEntry::Known(value) => result_shape.push(Some(*value as usize)),
                ReshapeEntry::Inferred => {
                    result_shape.push(None);
                    dims.push(match known {
                        0 => HirExtent::Constant(0),
                        _ => HirExtent::Div(
                            Box::new(total.clone()),
                            Box::new(divisor_extent.clone()),
                        ),
                    });
                }
                ReshapeEntry::Named(_) => {
                    result_shape.push(None);
                    dims.push(HirExtent::Scalar(named_at + 1));
                    named_at += 1;
                }
            }
        }
        let result = HirType::Tensor {
            element,
            shape: result_shape,
            names: AxisNames(vec![None; parsed.len()]),
        };

        // A `-1` divides by the product of the other entries, so a zero there is refused
        // before the cast; with it the target count is that product times the quotient.
        let mut checks = Vec::new();
        if inferred && (known == 0 || !named.is_empty()) {
            let nonzero = binary(
                BinaryOp::NotEqual,
                divisor.clone(),
                u64_value(0, span),
                HirType::Bool,
                span,
            );
            checks.push(guard(
                nonzero,
                panic_with(
                    vec![
                        text("tensor reshape: "),
                        shown(count.clone()),
                        text(" elements do not fit the target shape's "),
                        shown(u64_value(0, span)),
                    ],
                    span,
                ),
                span,
            ));
        }
        let expected = match inferred {
            true => binary(
                BinaryOp::Multiply,
                divisor.clone(),
                binary(
                    BinaryOp::Divide,
                    count.clone(),
                    divisor.clone(),
                    HirType::U64,
                    span,
                ),
                HirType::U64,
                span,
            ),
            false => divisor,
        };
        let agreed = binary(
            BinaryOp::Equal,
            count.clone(),
            expected.clone(),
            HirType::Bool,
            span,
        );
        checks.push(guard(
            agreed,
            panic_with(
                vec![
                    text("tensor reshape: "),
                    shown(count),
                    text(" elements do not fit the target shape's "),
                    shown(expected),
                ],
                span,
            ),
            span,
        ));

        let cast = if named.is_empty() {
            HirExpr::new(
                HirExprKind::TensorShapeCast {
                    receiver: Box::new(variable(&source, receiver_ty.clone(), span)),
                    permutation: None,
                    dims,
                },
                result.clone(),
                span,
            )
        } else {
            self.extent_counter += 1;
            let name = format!("{RESHAPE_PREFIX}{}", self.extent_counter);
            let operand_names: Vec<String> = (0..=named.len())
                .map(|index| format!("__operand{index}"))
                .collect();
            let mut params = vec![HirParam {
                name: operand_names[0].clone(),
                ty: receiver_ty.clone(),
                span,
            }];
            let mut arguments = vec![variable(&source, receiver_ty.clone(), span)];
            let mut parameter_types = vec![receiver_ty.clone()];
            for (index, extent_name) in named.iter().enumerate() {
                params.push(HirParam {
                    name: operand_names[index + 1].clone(),
                    ty: HirType::U64,
                    span,
                });
                arguments.push(variable(
                    &crate::extents::local_name(extent_name),
                    HirType::U64,
                    span,
                ));
                parameter_types.push(HirType::U64);
            }
            let body_cast = HirExpr::new(
                HirExprKind::TensorShapeCast {
                    receiver: Box::new(variable(&operand_names[0], receiver_ty.clone(), span)),
                    permutation: None,
                    dims: dims.clone(),
                },
                result.clone(),
                span,
            );
            self.mono_items.push(HirItem::Function(HirFunction {
                name: name.clone(),
                params,
                return_type: result.clone(),
                body: vec![HirStmt::Expr(body_cast)],
                target: HirTarget::Host,
                span,
                result_extents: dims,
            }));
            let callee = HirExpr::new(
                HirExprKind::Variable(name),
                HirType::Function {
                    params: parameter_types,
                    ret: Box::new(result.clone()),
                },
                span,
            );
            HirExpr::new(
                HirExprKind::Call {
                    callee: Box::new(callee),
                    args: arguments,
                },
                result.clone(),
                span,
            )
        };

        let mut stmts = vec![HirStmt::VarDecl {
            name: source,
            ty: receiver_ty,
            init: Some(receiver),
            mutable: false,
            span,
        }];
        stmts.extend(checks);
        stmts.push(HirStmt::Expr(cast));
        Ok(HirExpr::new(HirExprKind::Block { stmts }, result, span))
    }

    fn reshape_entry(&self, entry: &Expr) -> Result<ReshapeEntry, LoweringError> {
        if let Expr::Identifier(ident) = entry
            && self.extent_in_scope(&ident.name)
        {
            return Ok(ReshapeEntry::Named(ident.name.clone()));
        }
        match self.reshape_extent(entry, CONST_FOLD_DEPTH) {
            Some(INFERRED_EXTENT) => Ok(ReshapeEntry::Inferred),
            Some(value) => u64::try_from(value)
                .map(ReshapeEntry::Known)
                .map_err(|_| malformed("a `.reshape` extent is negative".to_string())),
            None => Err(malformed(
                "a `.reshape` extent over a dynamic receiver has no value".to_string(),
            )),
        }
    }
}

impl Lowerer {
    /// A `.reshape` extent's value, mirroring the checker: a literal, a constant or an
    /// extent parameter of the instance, or arithmetic over these. A local binding
    /// shadows both and is a run-time value. Checked, like the checker's fold.
    fn reshape_extent(&self, expr: &Expr, depth: u32) -> Option<i128> {
        let depth = depth.checked_sub(1)?;
        match expr {
            Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
            Expr::Paren(inner, _) => self.reshape_extent(inner, depth),
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
                ..
            } => self.reshape_extent(operand, depth)?.checked_neg(),
            Expr::Identifier(ident) => {
                if self.lookup_local(&ident.name).is_some() {
                    return None;
                }
                if let Some(value) = self.const_subst.get(&ident.name) {
                    return Some(i128::from(*value));
                }
                self.reshape_extent(self.constant_values.get(&ident.name)?, depth)
            }
            Expr::Binary {
                left, op, right, ..
            } => {
                let l = self.reshape_extent(left, depth)?;
                let r = self.reshape_extent(right, depth)?;
                match op {
                    BinaryOp::Add => l.checked_add(r),
                    BinaryOp::Subtract => l.checked_sub(r),
                    BinaryOp::Multiply => l.checked_mul(r),
                    BinaryOp::Divide if l >= 0 && r > 0 => l.checked_div(r),
                    BinaryOp::Modulo if l >= 0 && r > 0 => l.checked_rem(r),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// The run-time extent of one axis of the receiver (operand 0), or its value when static.
fn receiver_extent(shape: &[Option<usize>], axis: usize) -> HirExtent {
    match shape[axis] {
        Some(value) => HirExtent::Constant(value as u64),
        None => HirExtent::Axis { operand: 0, axis },
    }
}

/// The number of elements of a tensor `source` of extents `shape`: the product of its extents,
/// each dynamic one read from `source`, which is borrowed so the read consumes nothing.
fn count_expr(source: &HirExpr, shape: &[Option<usize>], span: Span) -> HirExpr {
    shape
        .iter()
        .enumerate()
        .fold(u64_value(1, span), |count, (axis, extent)| {
            let value = match extent {
                Some(value) => u64_value(*value as u64, span),
                None => extent_read(borrow(source.clone()), axis, span),
            };
            binary(BinaryOp::Multiply, count, value, HirType::U64, span)
        })
}

/// The element count of a receiver of extents `shape`, as the `HirExtent` a cast reads it
/// from operand 0.
fn count_extent(shape: &[Option<usize>]) -> HirExtent {
    product_extent(
        (0..shape.len())
            .map(|axis| receiver_extent(shape, axis))
            .collect(),
    )
}

/// The product of `factors`, folded so that static factors combine into one constant.
fn product_extent(factors: Vec<HirExtent>) -> HirExtent {
    factors
        .into_iter()
        .reduce(|left, right| HirExtent::Mul(Box::new(left), Box::new(right)))
        .unwrap_or(HirExtent::Constant(1))
}

/// Each result axis with no value gets the run-time extent `of` gives its index.
fn result_dims(result: &[Option<usize>], of: impl Fn(usize) -> HirExtent) -> Vec<HirExtent> {
    result
        .iter()
        .enumerate()
        .filter(|(_, extent)| extent.is_none())
        .map(|(axis, _)| of(axis))
        .collect()
}

fn transpose(shape: &[Option<usize>], names: &AxisNames) -> ShapeCast {
    permuted(shape, names, vec![1, 0])
}

/// A cast that permutes the receiver's axes by `order`, where result axis `d` is receiver
/// axis `order[d]`.
fn permuted(shape: &[Option<usize>], names: &AxisNames, order: Vec<usize>) -> ShapeCast {
    let result: Vec<Option<usize>> = order.iter().map(|axis| shape[*axis]).collect();
    let dims = result_dims(&result, |d| HirExtent::Axis {
        operand: 0,
        axis: order[d],
    });
    ShapeCast {
        shape: result,
        names: reorder_names(names, &order),
        permutation: Some(order),
        dims,
    }
}

/// `.reshape`'s result shape over a receiver with only static axes. The checker has validated
/// every count; inside a shape-generic instance this is where they are first known, so a count
/// that does not work out is the language's per-instance error, not a checker escape.
fn reshape(
    shape: &[usize],
    args: &[Expr],
    extent: &dyn Fn(&Expr) -> Option<i128>,
    span: Span,
) -> Result<ShapeCast, LoweringError> {
    let total: usize = shape.iter().product();
    let entries = axis_list(args, RESHAPE_METHOD)?;

    let mut extents: Vec<Option<usize>> = Vec::with_capacity(entries.len());
    let mut inferred_at = None;
    for entry in entries {
        let value = extent(entry);
        if value == Some(INFERRED_EXTENT) {
            inferred_at = Some(extents.len());
            extents.push(None);
            continue;
        }
        let Some(value) = value.and_then(|value| usize::try_from(value).ok()) else {
            return Err(instance_shape(
                "a `.reshape` extent has no value at this instance: it divides by zero or is \
                 negative, and an axis has a non-negative extent"
                    .to_string(),
                entry.span(),
            ));
        };
        extents.push(Some(value));
    }

    let known: usize = extents.iter().flatten().product();
    if let Some(position) = inferred_at {
        if known == 0 || !total.is_multiple_of(known) {
            return Err(instance_shape(
                format!(
                    "`-1` cannot be inferred at this instance: the other extents multiply to \
                     {known}, which does not divide the receiver's {total} elements"
                ),
                span,
            ));
        }
        extents[position] = Some(total / known);
    }

    let resolved: Vec<usize> = extents.into_iter().flatten().collect();
    let count: usize = resolved.iter().product();
    if count != total {
        return Err(instance_shape(
            format!(
                "`.reshape` would hold {count} elements at this instance but the receiver \
                 holds {total}; a reshape cannot change how many elements a tensor has"
            ),
            span,
        ));
    }
    Ok(ShapeCast {
        names: AxisNames(vec![None; resolved.len()]),
        shape: neuro_hir::static_shape(&resolved),
        permutation: None,
        dims: Vec::new(),
    })
}

fn permute(
    shape: &[Option<usize>],
    names: &AxisNames,
    args: &[Expr],
) -> Result<ShapeCast, LoweringError> {
    let entries = axis_list(args, PERMUTE_METHOD)?;
    let order = resolve_axes(shape.len(), names, entries, PERMUTE_METHOD)?;
    if order.len() != shape.len() {
        return Err(malformed(format!(
            "`.permute` reached lowering with {} axes for a rank-{} tensor",
            order.len(),
            shape.len()
        )));
    }
    Ok(permuted(shape, names, order))
}

fn flatten(
    shape: &[Option<usize>],
    names: &AxisNames,
    args: &[Expr],
) -> Result<ShapeCast, LoweringError> {
    if args.is_empty() {
        let merged: Vec<HirExtent> = (0..shape.len())
            .map(|axis| receiver_extent(shape, axis))
            .collect();
        let total = shape
            .iter()
            .copied()
            .try_fold(1usize, |count, extent| extent.map(|value| count * value));
        let result = vec![total];
        let dims = result_dims(&result, |_| product_extent(merged.clone()));
        return Ok(ShapeCast {
            shape: result,
            names: AxisNames(vec![None]),
            permutation: None,
            dims,
        });
    }
    let entries = axis_list(args, FLATTEN_METHOD)?;
    let axes = resolve_axes(shape.len(), names, entries, FLATTEN_METHOD)?;
    let first = *axes
        .first()
        .ok_or_else(|| malformed("`.flatten` reached lowering with no axes".to_string()))?;
    if axes.iter().enumerate().any(|(step, a)| *a != first + step) {
        return Err(malformed(
            "`.flatten` reached lowering with non-adjacent axes".to_string(),
        ));
    }

    let run = &shape[first..first + axes.len()];
    let merged: Option<usize> = run
        .iter()
        .try_fold(1usize, |count, extent| extent.map(|value| count * value));
    let mut result = shape[..first].to_vec();
    let mut result_names = names.0[..first].to_vec();
    result.push(merged);
    result_names.push(None);
    result.extend_from_slice(&shape[first + axes.len()..]);
    result_names.extend_from_slice(&names.0[first + axes.len()..]);

    // The axes that survive keep their own run-time extent, the merged one the product of
    // its run; a flatten never changes the count, so it needs no check.
    let mut dims = Vec::new();
    for (position, extent) in result.iter().enumerate() {
        if extent.is_some() {
            continue;
        }
        let of = if position == first {
            product_extent(
                (first..first + axes.len())
                    .map(|axis| receiver_extent(shape, axis))
                    .collect(),
            )
        } else if position < first {
            receiver_extent(shape, position)
        } else {
            receiver_extent(shape, position + axes.len() - 1)
        };
        dims.push(of);
    }
    Ok(ShapeCast {
        shape: result,
        names: AxisNames(result_names),
        permutation: None,
        dims,
    })
}

/// The elements of the single array-literal argument these calls take.
fn axis_list<'a>(args: &'a [Expr], method: &str) -> Result<&'a [Expr], LoweringError> {
    match args {
        [Expr::ArrayLiteral { elements, .. }] => Ok(elements),
        _ => Err(malformed(format!(
            "`.{method}` reached lowering without its axis-list literal"
        ))),
    }
}

/// Each entry of an axis list as a receiver axis index: an identifier is a dimension
/// name, an integer literal a position.
fn resolve_axes(
    rank: usize,
    names: &AxisNames,
    entries: &[Expr],
    method: &str,
) -> Result<Vec<usize>, LoweringError> {
    let mut axes = Vec::with_capacity(entries.len());
    for entry in entries {
        let axis = match entry {
            Expr::Identifier(ident) => names.position_of(&ident.name).ok_or_else(|| {
                malformed(format!(
                    "`.{method}` reached lowering naming dimension '{}', which its receiver's \
                     type does not declare",
                    ident.name
                ))
            })?,
            other => {
                let value = const_integer(other).ok_or_else(|| {
                    malformed(format!(
                        "`.{method}` reached lowering with a non-constant axis"
                    ))
                })?;
                usize::try_from(value)
                    .ok()
                    .filter(|a| *a < rank)
                    .ok_or_else(|| {
                        malformed(format!(
                            "`.{method}` reached lowering with axis {value} out of range"
                        ))
                    })?
            }
        };
        axes.push(axis);
    }
    Ok(axes)
}

/// The receiver's names, gathered in the result's axis order.
fn reorder_names(names: &AxisNames, order: &[usize]) -> AxisNames {
    AxisNames(
        order
            .iter()
            .map(|axis| names.0.get(*axis).cloned().flatten())
            .collect(),
    )
}

/// The value of an extent expression written in a type, `D / H`, under the instance's
/// extents. `None` where it has none: a zero divisor, or a negative operand of `/` or
/// `%`, which no extent is. The caller refuses a negative result. `count` is a shape
/// parameter's element count, for `S.count()`.
pub(crate) fn extent_value(
    expr: &Expr,
    const_subst: &HashMap<String, u64>,
    count: &dyn Fn(&str) -> Option<i128>,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => Some(*value),
        Expr::Identifier(ident) => const_subst.get(&ident.name).map(|v| i128::from(*v)),
        Expr::Paren(inner, _) => extent_value(inner, const_subst, count),
        Expr::Call { func, .. } => match &**func {
            Expr::FieldAccess { object, .. } => match &**object {
                Expr::Identifier(shape) => count(&shape.name),
                _ => None,
            },
            _ => None,
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let l = extent_value(left, const_subst, count)?;
            let r = extent_value(right, const_subst, count)?;
            match op {
                BinaryOp::Add => l.checked_add(r),
                BinaryOp::Subtract => l.checked_sub(r),
                BinaryOp::Multiply => l.checked_mul(r),
                BinaryOp::Divide if l >= 0 && r > 0 => l.checked_div(r),
                BinaryOp::Modulo if l >= 0 && r > 0 => l.checked_rem(r),
                _ => None,
            }
        }
        _ => None,
    }
}

/// An extent expression as the program spells it, for a diagnostic.
pub(crate) fn extent_text(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Integer(value, _), _) => value.to_string(),
        Expr::Identifier(ident) => ident.name.clone(),
        Expr::Paren(inner, _) => format!("({})", extent_text(inner)),
        Expr::Call { func, .. } => match &**func {
            Expr::FieldAccess { object, .. } => format!("{}.count()", extent_text(object)),
            _ => "?".to_string(),
        },
        Expr::Binary {
            left, op, right, ..
        } => {
            let symbol = match op {
                BinaryOp::Add => "+",
                BinaryOp::Subtract => "-",
                BinaryOp::Multiply => "*",
                BinaryOp::Divide => "/",
                _ => "%",
            };
            format!("{} {symbol} {}", extent_text(left), extent_text(right))
        }
        _ => "?".to_string(),
    }
}

/// The value of an integer constant expression written in an axis list.
pub(crate) fn const_integer(expr: &Expr) -> Option<i128> {
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
