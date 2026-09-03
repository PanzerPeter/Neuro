//! Type-checking for the `.map(f)` / `.filter(p)` chain a `for` head may wear.
//!
//! The chain transforms the element type the loop binding receives: `.map(f)`
//! replaces it with `f`'s return type, `.filter(p)` leaves it alone. Each adapter's
//! argument is an ordinary expression of function type, checked in the scope
//! *outside* the loop — it cannot see the loop binding it feeds.

use ast_types::{LoopAdapter, LoopAdapterKind};

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;

/// What `.filter()`'s predicate must answer, named for the diagnostic.
const FILTER_RESULT: &str = "bool";
/// What `.map()`'s function must answer, named for the diagnostic.
const MAP_RESULT: &str = "a value";

impl TypeChecker {
    /// Fold an adapter chain over the element type the head produces.
    ///
    /// `element` is `None` when the base head failed to type, in which case the
    /// adapters are still checked — their arguments may hold errors of their own —
    /// but no element-type mismatch is reported against a type nobody knows.
    pub(crate) fn check_loop_adapters(
        &mut self,
        element: Option<Type>,
        adapters: &[LoopAdapter],
    ) -> Option<Type> {
        let mut current = element;
        for adapter in adapters {
            current = self.check_loop_adapter(current, adapter);
        }
        current
    }

    fn check_loop_adapter(&mut self, element: Option<Type>, adapter: &LoopAdapter) -> Option<Type> {
        let name = adapter_name(adapter.kind).to_string();
        // The function value is READ, not moved: it is evaluated once, called per
        // element, and stays usable after the loop.
        let callee_ty = self
            .check_expr(&adapter.callee, None)
            .unwrap_or(Type::Unknown);

        let (param, ret) = match &callee_ty {
            Type::Function { params, ret } if params.len() == 1 => {
                (params[0].clone(), (**ret).clone())
            }
            Type::Unknown => return None,
            found => {
                self.record_error(TypeError::LoopAdapterNotCallable {
                    adapter: name,
                    found: found.clone(),
                    span: adapter.span,
                });
                return None;
            }
        };

        if let Some(element) = &element {
            if !matches!(element, Type::Unknown) && !self.assignable(element, &param) {
                self.record_error(TypeError::LoopAdapterInput {
                    adapter: name.clone(),
                    expected: element.clone(),
                    found: param,
                    span: adapter.span,
                });
            }
        }

        match adapter.kind {
            LoopAdapterKind::Filter => {
                if !matches!(ret, Type::Bool | Type::Unknown) {
                    self.record_error(TypeError::LoopAdapterOutput {
                        adapter: name,
                        expected: FILTER_RESULT.to_string(),
                        found: ret,
                        span: adapter.span,
                    });
                }
                element
            }
            LoopAdapterKind::Map => {
                if matches!(ret, Type::Void) {
                    self.record_error(TypeError::LoopAdapterOutput {
                        adapter: name,
                        expected: MAP_RESULT.to_string(),
                        found: ret,
                        span: adapter.span,
                    });
                    return None;
                }
                Some(ret)
            }
        }
    }
}

/// The method spelling of an adapter, as it appears in a diagnostic.
pub(crate) fn adapter_name(kind: LoopAdapterKind) -> &'static str {
    match kind {
        LoopAdapterKind::Map => "map",
        LoopAdapterKind::Filter => "filter",
    }
}
