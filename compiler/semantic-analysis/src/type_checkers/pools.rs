// `pool` block escape rules.
//
// A pool's arena is released in one step at the block's closing brace, so anything
// that carries arena memory past that brace, or jumps over it, is rejected here.
// Three shapes reach past the brace: a store into a binding declared before the
// block, a store through a reference, and a `return` / `break` / `continue` that
// leaves the block. Everything else the block allocates dies with it.
//
// The rule is deliberately about the *place written to*, not about where the value
// came from: proving which allocation a value carries is the ownership analysis a
// later item builds, and until it exists the conservative test is the sound one.

use shared_types::Span;

use crate::errors::TypeError;
use crate::types::Type;

use super::{PoolContext, TypeChecker};

impl TypeChecker {
    /// Open a pool region for the body about to be checked. `label` is the pool's
    /// source label, used only to name it in a diagnostic.
    ///
    /// Call it after the body's scope has been pushed: the scope index recorded
    /// here is the floor that separates the block's own bindings from the ones it
    /// inherits.
    pub(super) fn push_pool(&mut self, label: Option<&str>) {
        let pool = match label {
            Some(name) => format!("the pool '{name}'"),
            None => "this 'pool' block".to_string(),
        };
        self.pool_stack.push(PoolContext {
            pool,
            scope_floor: self.symbols.depth().saturating_sub(1),
            loop_floor: self.loop_stack.len(),
        });
    }

    pub(super) fn pop_pool(&mut self) {
        let _ = self.pool_stack.pop();
    }

    /// Reject storing a value of type `ty` into the binding `name` when `name` was
    /// declared before the innermost open pool. Inert outside a pool.
    pub(crate) fn check_pool_store(&mut self, name: &str, ty: &Type, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        if pool_safe(ty) {
            return;
        }
        // A name the symbol table does not know is already an undefined-variable
        // error; reporting a second diagnostic about it would only add noise.
        let Some(depth) = self.symbols.defining_depth(name) else {
            return;
        };
        if depth >= pool.scope_floor {
            return;
        }
        let place = format!("'{name}'");
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolStoreEscapes {
            place,
            ty: ty.clone(),
            pool,
            span,
        });
    }

    /// Reject `*pointer = value` inside a pool when the referent can carry an
    /// allocation. Which place the reference denotes is not known here, so the
    /// referent type alone decides. Inert outside a pool.
    pub(crate) fn check_pool_ref_store(&mut self, referent: &Type, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        if pool_safe(referent) {
            return;
        }
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolStoreEscapes {
            place: "the place behind this reference".to_string(),
            ty: referent.clone(),
            pool,
            span,
        });
    }

    /// Reject a `return` written inside a pool block. Inert outside a pool.
    pub(crate) fn check_pool_return(&mut self, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolControlFlowEscapes {
            keyword: "return".to_string(),
            pool,
            span,
        });
    }

    /// Reject a `break` / `continue` whose target loop encloses the innermost pool.
    /// A jump to a loop opened *inside* the block stays inside it and is allowed.
    /// Inert outside a pool.
    pub(crate) fn check_pool_loop_jump(&mut self, keyword: &str, label: Option<&str>, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        let target = match label {
            Some(name) => self
                .loop_stack
                .iter()
                .rposition(|ctx| ctx.label.as_deref() == Some(name)),
            None => self.loop_stack.len().checked_sub(1),
        };
        // No target at all is `break` outside a loop, reported on its own.
        let Some(target) = target else {
            return;
        };
        if target >= pool.loop_floor {
            return;
        }
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolControlFlowEscapes {
            keyword: keyword.to_string(),
            pool,
            span,
        });
    }
}

/// Whether a value of this type can be stored into a place that outlives a pool.
///
/// Only types that carry no pointer at all qualify. A `string`, collection, tensor,
/// reference or struct may hold an address into the arena, and which one it holds is
/// exactly what this phase cannot prove, so every one of them is refused.
fn pool_safe(ty: &Type) -> bool {
    match ty {
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::F16
        | Type::BF16
        | Type::F32
        | Type::F64
        | Type::Bool
        | Type::Char
        | Type::Void
        | Type::Enum(_)
        | Type::Newtype(_)
        | Type::ConstValue(_)
        | Type::Unknown => true,
        Type::Array { element, .. } => pool_safe(element),
        Type::Tuple(elements) => elements.iter().all(pool_safe),
        _ => false,
    }
}
