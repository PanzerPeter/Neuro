// Array and tuple literals and their element reads.
//
// Reached from the `check_expr` dispatch in this module's `mod.rs`. Every file
// here adds methods to the same `impl TypeChecker` block.

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};
use ast_types::Expr;
use shared_types::Span;

impl TypeChecker {
    /// Array literal `[e0, ...]`: all elements share one type, fixed by
    /// the first and required of the rest. An empty literal needs a `[T; N]`
    /// annotation to know its element type.
    pub(super) fn check_array_literal_expr(
        &mut self,
        elements: &[Expr],
        span: &Span,
        expected: Option<&Type>,
    ) -> Option<Type> {
        // A `Tensor<T, [...]>` annotation reaching an array literal makes it a tensor
        // literal of that type. Without one the literal is a plain array, which
        // is what keeps `val arr = [1.0, 2.0, 3.0]` a `[f64; 3]`.
        if let Some(Type::Tensor { element, shape }) = expected {
            let element = (**element).clone();
            let shape = shape.clone();
            return Some(self.check_tensor_literal(elements, &element, &shape, *span));
        }

        let expected_element = match expected {
            Some(Type::Array { element, .. }) => Some((**element).clone()),
            _ => None,
        };

        if elements.is_empty() {
            return match expected {
                Some(Type::Array { element, size }) => {
                    if let ArrayLen::Fixed(n) = size {
                        if *n != 0 {
                            self.record_error(TypeError::ArrayLengthMismatch {
                                expected: *n,
                                found: 0,
                                span: *span,
                            });
                        }
                    }
                    Some(Type::Array {
                        element: element.clone(),
                        size: ArrayLen::Fixed(0),
                    })
                }
                _ => {
                    self.record_error(TypeError::CannotInferEmptyArray { span: *span });
                    Some(Type::Unknown)
                }
            };
        }

        let element_ty = self
            .check_expr(&elements[0], expected_element.as_ref())
            .unwrap_or(Type::Unknown);
        for el in &elements[1..] {
            let el_ty = self
                .check_expr(el, Some(&element_ty))
                .unwrap_or(Type::Unknown);
            if !matches!(element_ty, Type::Unknown)
                && !matches!(el_ty, Type::Unknown)
                && !el_ty.is_compatible_with(&element_ty)
            {
                self.record_error(TypeError::Mismatch {
                    expected: element_ty.clone(),
                    found: el_ty,
                    span: el.span(),
                });
            }
        }

        if matches!(element_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }

        if !self.is_type_copy(&element_ty) {
            self.record_error(TypeError::NonCopyArrayElement {
                ty: element_ty,
                span: *span,
            });
            return Some(Type::Unknown);
        }

        let size = elements.len();
        if let Some(Type::Array {
            size: ArrayLen::Fixed(expected_size),
            ..
        }) = expected
        {
            if *expected_size != size {
                self.record_error(TypeError::ArrayLengthMismatch {
                    expected: *expected_size,
                    found: size,
                    span: *span,
                });
            }
        }

        Some(Type::Array {
            element: Box::new(element_ty),
            size: ArrayLen::Fixed(size),
        })
    }

    /// Array rest pattern remainder `..rest`: the compiler-internal node
    /// a `val [a, b, ..rest] = arr` desugar produces. The source must be an
    /// array; the result is the `[T; N - start]` tail. `exact` (no rest binding
    /// in the pattern) requires the lengths to match precisely.
    pub(super) fn check_array_rest_expr(
        &mut self,
        array: &Expr,
        start: usize,
        exact: bool,
        span: &Span,
    ) -> Option<Type> {
        let arr_ty = self.check_expr(array, None).unwrap_or(Type::Unknown);
        if matches!(arr_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }
        match arr_ty.referent() {
            Type::Array {
                element,
                size: ArrayLen::Fixed(n),
            } => {
                let n = *n;
                let mismatch = if exact { n != start } else { start > n };
                if mismatch {
                    self.record_error(TypeError::ArrayPatternLengthMismatch {
                        expected: start,
                        found: n,
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                Some(Type::Array {
                    element: element.clone(),
                    size: ArrayLen::Fixed(n - start),
                })
            }
            // A rest pattern over a const-generic-sized array `[T; N]` cannot be
            // split at compile time inside the template; it is resolved once
            // monomorphized. Not supported as a template-body pattern this phase.
            Type::Array { .. } => Some(Type::Unknown),
            other => {
                self.record_error(TypeError::NotIndexable {
                    found: other.clone(),
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }

    /// Tuple literal `(e0, e1, ...)`: each element is checked against the
    /// corresponding element type of an expected tuple annotation, when present.
    pub(super) fn check_tuple_literal_expr(
        &mut self,
        elements: &[Expr],
        expected: Option<&Type>,
    ) -> Option<Type> {
        let expected_elems = match expected {
            Some(Type::Tuple(es)) if es.len() == elements.len() => Some(es.clone()),
            _ => None,
        };
        let mut tys = Vec::with_capacity(elements.len());
        for (i, el) in elements.iter().enumerate() {
            let hint = expected_elems.as_ref().map(|es| &es[i]);
            let el_ty = self.check_expr(el, hint).unwrap_or(Type::Unknown);
            if !self.is_type_copy(&el_ty) && !matches!(el_ty, Type::Unknown) {
                self.record_error(TypeError::NonCopyTupleElement {
                    ty: el_ty.clone(),
                    span: el.span(),
                });
            }
            tys.push(el_ty);
        }
        Some(Type::Tuple(tys))
    }

    /// Tuple index `object.N`: the object must be a tuple (or a borrow of
    /// one); `N` must be within bounds; the result is the N-th element type.
    pub(super) fn check_tuple_index_expr(
        &mut self,
        object: &Expr,
        index: usize,
        span: &Span,
    ) -> Option<Type> {
        let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
        if matches!(obj_ty, Type::Unknown) {
            return Some(Type::Unknown);
        }
        match obj_ty.referent() {
            Type::Tuple(elements) => {
                if let Some(el) = elements.get(index) {
                    Some(el.clone())
                } else {
                    self.record_error(TypeError::TupleIndexOutOfBounds {
                        index,
                        arity: elements.len(),
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }
            // `.0` on a newtype reads its single inner value. A newtype
            // has exactly one field, so any index other than 0 is out of range.
            Type::Newtype(nt_name) => {
                if index == 0 {
                    Some(
                        self.lookup_newtype_inner(nt_name)
                            .cloned()
                            .unwrap_or(Type::Unknown),
                    )
                } else {
                    self.record_error(TypeError::TupleIndexOutOfBounds {
                        index,
                        arity: 1,
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }
            other => {
                self.record_error(TypeError::NotATuple {
                    found: other.clone(),
                    span: *span,
                });
                Some(Type::Unknown)
            }
        }
    }
}
