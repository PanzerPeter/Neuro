// The functional traversals `.map(f)`, `.zip(other, f)`, and `.reduce(init, f)` on a
// tensor.
//
// Reached from the builtin-method arm of `check_call_expr`. All three walk the receiver's
// elements once and call a function per element; they differ only in what that function is
// given and what becomes of its answer. Every argument here IS a value, unlike the axis of
// a reduction or a selection, so each one is checked in the ordinary way.
//
// The receiver is READ. `.map` and `.zip` allocate their own result and `.reduce`
// allocates nothing, so a borrowed receiver is accepted and nothing is moved: mapping over
// a shared weight must not consume it.
//
// There is deliberately no `.filter`. A filter's output length depends on the values in
// the buffer, so its result has no shape the type system can name.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, TensorAxis, Type};
use ast_types::Expr;
use shared_types::Span;

pub(crate) const MAP_METHOD: &str = "map";
pub(crate) const ZIP_METHOD: &str = "zip";
pub(crate) const REDUCE_METHOD: &str = "reduce";

/// Whether `method` names one of the three functional traversals.
pub(crate) fn is_apply_method(method: &str) -> bool {
    matches!(method, MAP_METHOD | ZIP_METHOD | REDUCE_METHOD)
}

/// How many arguments each traversal takes, the function included.
fn arity(method: &str) -> usize {
    match method {
        MAP_METHOD => 1,
        _ => 2,
    }
}

impl TypeChecker {
    /// Type-check one traversal and return the type it produces: a tensor of the
    /// receiver's shape over the function's return type for `.map` and `.zip`, and the
    /// seed's own type for `.reduce`.
    pub(crate) fn check_tensor_apply(
        &mut self,
        element: &Type,
        shape: &[TensorAxis],
        method: &str,
        args: &[Expr],
        call_span: Span,
    ) -> Type {
        // Every extent has to be concrete: the traversal's length is the product of them
        // and the result buffer is allocated from them, both before any element exists.
        if let Some(symbolic) = shape
            .iter()
            .find(|axis| !matches!(axis.extent, ArrayLen::Fixed(_)))
        {
            self.record_error(TypeError::TensorShapeCastSymbolicExtent {
                method: method.to_string(),
                name: symbolic.extent.to_string(),
                span: call_span,
            });
            return Type::Unknown;
        }
        let expected = arity(method);
        if args.len() != expected {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected,
                found: args.len(),
                span: call_span,
            });
            return Type::Unknown;
        }

        // `.zip`'s operand decides what the function's second parameter is handed, so it
        // is checked before the function. A `.reduce`'s seed is checked AFTER, against the
        // accumulator parameter: `t.reduce(0.0, |acc: f32, x: f32| ...)` folds in `f32`,
        // and an untyped literal has nothing but that parameter to say so.
        let zipped = match method {
            ZIP_METHOD => match self.zip_operand(element, shape, &args[0]) {
                Some(operand) => Some(operand),
                None => return Type::Unknown,
            },
            _ => None,
        };

        let callee = &args[expected - 1];
        // The function value is READ, not moved: it is evaluated once and called per
        // element, and stays usable after the traversal.
        let callee_ty = self.check_expr(callee, None).unwrap_or(Type::Unknown);
        let Type::Function { params, ret } = &callee_ty else {
            if !matches!(callee_ty, Type::Unknown) {
                self.record_error(TypeError::TensorApplyNotCallable {
                    method: method.to_string(),
                    found: callee_ty.clone(),
                    span: callee.span(),
                });
            }
            return Type::Unknown;
        };

        let carried = match method {
            ZIP_METHOD => zipped,
            REDUCE_METHOD => Some(
                self.check_expr(&args[0], params.first())
                    .unwrap_or(Type::Unknown),
            ),
            _ => None,
        };

        let wanted: Vec<Type> = match method {
            MAP_METHOD => vec![element.clone()],
            ZIP_METHOD => vec![element.clone(), carried.clone().unwrap_or(Type::Unknown)],
            // A fold is called with the carried accumulator first and the element second,
            // which is the order `|acc, x|` is written in.
            _ => vec![carried.clone().unwrap_or(Type::Unknown), element.clone()],
        };
        if params.len() != wanted.len() {
            self.record_error(TypeError::TensorApplyArity {
                method: method.to_string(),
                expected: wanted.len(),
                found: params.len(),
                span: callee.span(),
            });
            return Type::Unknown;
        }
        for (position, (found, expected)) in params.iter().zip(wanted.iter()).enumerate() {
            if matches!(found, Type::Unknown) || matches!(expected, Type::Unknown) {
                continue;
            }
            if !expected.is_compatible_with(found) {
                self.record_error(TypeError::TensorApplyParamType {
                    method: method.to_string(),
                    position,
                    expected: expected.clone(),
                    found: found.clone(),
                    span: callee.span(),
                });
                return Type::Unknown;
            }
        }

        let ret = (**ret).clone();
        if method == REDUCE_METHOD {
            let seed = carried.unwrap_or(Type::Unknown);
            if !matches!(seed, Type::Unknown)
                && !matches!(ret, Type::Unknown)
                && !seed.is_compatible_with(&ret)
            {
                self.record_error(TypeError::TensorReduceAccumulator {
                    expected: seed,
                    found: ret,
                    span: callee.span(),
                });
                return Type::Unknown;
            }
            return seed;
        }

        if !ret.is_integer() && !ret.is_float() {
            self.record_error(TypeError::TensorApplyResultElement {
                method: method.to_string(),
                found: ret,
                span: callee.span(),
            });
            return Type::Unknown;
        }
        Type::Tensor {
            element: Box::new(ret),
            shape: shape.to_vec(),
        }
    }

    /// The element type of `.zip`'s second tensor, once it is known to be a tensor
    /// carrying the receiver's shape.
    fn zip_operand(&mut self, element: &Type, shape: &[TensorAxis], entry: &Expr) -> Option<Type> {
        let operand = self.check_expr(entry, None)?;
        let Type::Tensor {
            element: other_element,
            shape: other_shape,
        } = operand.referent().clone()
        else {
            if !matches!(operand, Type::Unknown) {
                self.record_error(TypeError::TensorZipOperandNotTensor {
                    found: operand,
                    span: entry.span(),
                });
            }
            return None;
        };
        let agrees = other_shape.len() == shape.len()
            && other_shape
                .iter()
                .zip(shape.iter())
                .all(|(found, expected)| found.extent == expected.extent);
        if !agrees {
            self.record_error(TypeError::TensorZipShapeMismatch {
                receiver: Type::Tensor {
                    element: Box::new(element.clone()),
                    shape: shape.to_vec(),
                },
                other: operand,
                span: entry.span(),
            });
            return None;
        }
        Some(*other_element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_traversals_are_recognised_and_nothing_else_is() {
        assert!(is_apply_method(MAP_METHOD));
        assert!(is_apply_method(ZIP_METHOD));
        assert!(is_apply_method(REDUCE_METHOD));
        assert!(!is_apply_method("filter"));
        assert!(!is_apply_method("sum"));
    }

    #[test]
    fn a_fold_takes_its_seed_alongside_its_function() {
        assert_eq!(arity(MAP_METHOD), 1);
        assert_eq!(arity(ZIP_METHOD), 2);
        assert_eq!(arity(REDUCE_METHOD), 2);
    }
}
