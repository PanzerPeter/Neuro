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
//
// One rule here is not an escape rule: a `Drop`-only value the block owns is refused
// outright. Nothing about it escapes; the arena simply cannot run a destructor per
// object without giving up the single-store release it exists for. `PoolAware` is the
// opt-in that says otherwise.

use shared_types::Span;

use crate::errors::TypeError;
use crate::types::Type;

use super::{PoolContext, TypeChecker};

/// The trait a type implements to say its external resource can be released by the
/// arena's single sweep instead of by a per-object destructor. Declared in the
/// prelude and matched here by name, the way `Drop` is.
const POOL_AWARE_TRAIT: &str = "PoolAware";

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

    /// Reject a value of a `Drop`-only type that the innermost open pool would own.
    ///
    /// `produced_by` names the function that built it, when a call did; a struct literal
    /// was written in the block itself and names nothing. Inert outside a pool.
    pub(crate) fn check_pool_construction(
        &mut self,
        ty: &Type,
        produced_by: Option<&str>,
        span: Span,
    ) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        let Type::Struct(name) = ty else {
            return;
        };
        if !self.drop_structs.contains(name) {
            return;
        }
        if self
            .trait_impls
            .contains(&(POOL_AWARE_TRAIT.to_string(), name.clone()))
        {
            return;
        }
        let origin = match produced_by {
            Some(func) => format!(" returned by '{func}'"),
            None => String::new(),
        };
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolDropOnlyValue {
            type_name: name.clone(),
            origin,
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

#[cfg(test)]
mod tests {
    use crate::type_check;
    use syntax_parsing::parse;

    /// The driver prepends the prelude; `type_check` does not, so these declare the two
    /// prelude items the rule reads exactly as `prelude.nr` spells them.
    const POOL_AWARE_DECL: &str = "\
struct PoolHandle { id: u64 }
trait PoolAware {
    func register_with_pool(&self, arena: &PoolHandle)
    func bulk_release(&mut self)
}
";

    fn errors(source: &str) -> Vec<String> {
        let ast = parse(&format!("{POOL_AWARE_DECL}{source}")).expect("source should parse");
        match type_check(&ast) {
            Ok(_) => Vec::new(),
            Err(errs) => errs.iter().map(|e| e.to_string()).collect(),
        }
    }

    const DROP_ONLY: &str = "\
struct Handle { id: i32 }
impl Drop for Handle {
    func drop(&mut self) { }
}
";

    #[test]
    fn a_drop_only_literal_in_a_pool_is_rejected() {
        let errs = errors(&format!(
            "{DROP_ONLY}
func main() -> i32 {{
    pool scratch {{
        val h = Handle {{ id: 1 }}
    }}
    return 0
}}"
        ));
        assert!(
            errs.iter()
                .any(|e| e.contains("'Handle' implements 'Drop'") && e.contains("'scratch'")),
            "expected a Drop-only rejection naming the pool, got {errs:?}"
        );
    }

    #[test]
    fn the_diagnostic_names_the_constructing_function() {
        let errs = errors(&format!(
            "{DROP_ONLY}
func open(id: i32) -> Handle {{ Handle {{ id: id }} }}
func main() -> i32 {{
    pool {{
        val h = open(3)
    }}
    return 0
}}"
        ));
        assert!(
            errs.iter().any(|e| e.contains("returned by 'open'")),
            "expected the callee named, got {errs:?}"
        );
    }

    #[test]
    fn a_pool_aware_type_is_accepted_in_a_pool() {
        let errs = errors(&format!(
            "{DROP_ONLY}
impl PoolAware for Handle {{
    func register_with_pool(&self, arena: &PoolHandle) {{ }}
    func bulk_release(&mut self) {{ }}
}}
func main() -> i32 {{
    pool {{
        val h = Handle {{ id: 1 }}
    }}
    return 0
}}"
        ));
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// The rule is about what the block OWNS. A value built before the block keeps its
    /// ordinary destructor and never enters the arena, so reading it inside is fine.
    #[test]
    fn a_drop_only_value_built_outside_the_pool_is_untouched() {
        let errs = errors(&format!(
            "{DROP_ONLY}
func main() -> i32 {{
    val outer = Handle {{ id: 1 }}
    pool {{
        val n = outer.id
    }}
    return 0
}}"
        ));
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// A struct with no destructor is plain data: the bulk arena free reclaims it and
    /// nothing else is owed, which is the row the rule must not widen onto.
    #[test]
    fn a_plain_struct_in_a_pool_is_untouched() {
        let errs = errors(
            "struct Point { x: i32 }
func main() -> i32 {
    pool {
        val p = Point { x: 1 }
    }
    return 0
}",
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }
}
