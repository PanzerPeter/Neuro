// `pool` block escape rules.
//
// A pool's arena is released in one step at the block's closing brace, so anything
// that carries arena memory past that brace, or jumps over it, is rejected here.
// Three shapes reach past the brace: a store into a binding declared before the
// block, a store through a reference, and a `return` / `break` / `continue` that
// leaves the block. Everything else the block allocates dies with it.
//
// A store is judged by the place's type AND by the value's provenance. The type test
// alone refused a value that never touched the arena, so `off_arena` answers the second
// half: it returns true only where the source *proves* the value carries no arena
// memory. Everything it cannot prove is arena memory by assumption: the conservative
// fallback the language rule demands, which fails toward the heap, never toward the arena.
//
// One rule here is not an escape rule: a `Drop`-only value the block owns is refused
// outright. Nothing about it escapes; the arena simply cannot run a destructor per
// object without giving up the single-store release it exists for. `PoolAware` is the
// opt-in that says otherwise.

use std::collections::HashSet;

use ast_types::Expr;
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

    /// Reject storing `value`, of type `ty`, into the binding `name` when `name` was
    /// declared before the innermost open pool and the value may carry arena memory.
    /// Inert outside a pool.
    pub(crate) fn check_pool_store(&mut self, name: &str, ty: &Type, value: &Expr, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        if pool_safe(ty) || self.off_arena(value) {
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
    /// allocation and the value is not provably off the arena. Which place the
    /// reference denotes is not known here, so the referent type stands in for it.
    /// Inert outside a pool.
    pub(crate) fn check_pool_ref_store(&mut self, referent: &Type, value: &Expr, span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        if pool_safe(referent) || self.off_arena(value) {
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

    /// Whether the source proves `value` holds no memory from any open pool's arena.
    ///
    /// False is the answer for everything not enumerated here, including every
    /// expression shape a later phase might add: an allocation whose owner cannot be
    /// proven belongs to the heap, never to the arena, so the unproven answer has to be
    /// the one that keeps the value inside the block.
    fn off_arena(&self, value: &Expr) -> bool {
        match value {
            // A scalar literal carries no pointer, and a string literal's bytes live in
            // `.rodata` for the program's lifetime rather than in any allocation.
            Expr::Literal(_, _) => true,
            // A binding of pointerless type has nothing to carry; otherwise it must
            // predate every open arena mark, since a store into such a binding from
            // inside a pool is exactly what this rule rejects.
            Expr::Identifier(id) => {
                self.symbols
                    .lookup(&id.name)
                    .is_some_and(|symbol| pool_safe(&symbol.ty))
                    || self.declared_before_pools(&id.name)
            }
            Expr::Paren(inner, _) => self.off_arena(inner),
            Expr::Cast { expr, .. } => self.off_arena(expr),
            Expr::Unary { operand, .. } => self.off_arena(operand),
            Expr::Reference { operand, .. } => self.off_arena(operand),
            Expr::Deref { operand, .. } => self.off_arena(operand),
            // A callee's allocations are emitted while its own body is generated, with
            // the backend's pool depth back at zero, so they come from libc however deep
            // inside a pool the call sits. The result is therefore heap memory unless the
            // callee was handed arena memory to give back, which is what the operand walk
            // rules out. It holds only for a callee the compiler can name: a builtin
            // method is emitted inline at the call site and does take the bump path.
            Expr::Call { func, args, .. } => {
                self.callee_is_user_code(func)
                    && self
                        .callee_operand(func)
                        .is_none_or(|obj| self.off_arena(obj))
                    && args.iter().all(|arg| self.off_arena(arg))
            }
            _ => false,
        }
    }

    /// Whether `name` resolves to a binding declared before the OUTERMOST open pool.
    ///
    /// The outermost, not the innermost: a binding made in an enclosing pool's body may
    /// hold that arena's memory, and carrying it out of the inner block would leave it
    /// live past the outer block's release just the same.
    fn declared_before_pools(&self, name: &str) -> bool {
        let Some(outermost) = self.pool_stack.first() else {
            return true;
        };
        self.symbols
            .defining_depth(name)
            .is_some_and(|depth| depth < outermost.scope_floor)
    }

    /// Whether `func` names a function this program declares, whose body the backend
    /// emits on its own outside every pool.
    ///
    /// A trait object's callee is not known until runtime, so `dyn` dispatch answers
    /// false: it is the case the conservative fallback exists for. A builtin or
    /// collection method answers false too, for the opposite reason — its body is not a
    /// function at all, but instructions inlined where the call was written.
    fn callee_is_user_code(&self, func: &Expr) -> bool {
        self.callee_key(func).is_some()
    }

    /// The key into `functions` for a callee the program declares, or `None` where the
    /// callee is a trait object, a builtin, or a generic template (which is not in
    /// `functions` at all, since each call site monomorphizes its own instance).
    fn callee_key(&self, func: &Expr) -> Option<String> {
        match func {
            Expr::Identifier(id) => self
                .functions
                .contains_key(&id.name)
                .then(|| id.name.clone()),
            Expr::Path {
                type_name, member, ..
            } => self.method_key(&type_name.name, &member.name).cloned(),
            Expr::FieldAccess { object, field, .. } => {
                let Expr::Identifier(receiver) = &**object else {
                    return None;
                };
                let symbol = self.symbols.lookup(&receiver.name)?;
                let Type::Struct(struct_name) = symbol.ty.referent() else {
                    return None;
                };
                self.method_key(struct_name, &field.name).cloned()
            }
            _ => None,
        }
    }

    /// Reject a call inside a pool that hands a value the block may have allocated to a
    /// callee able to store it somewhere outliving the block.
    ///
    /// The store rules above watch places written in the block's own source; this one
    /// covers the store a callee performs on the caller's behalf, which no amount of
    /// looking at the block's text reveals. Inert outside a pool.
    pub(crate) fn check_pool_retention(&mut self, func: &Expr, args: &[Expr], span: Span) {
        let Some(pool) = self.pool_stack.last() else {
            return;
        };
        // Nothing the block owns is being handed over, so there is nothing to retain.
        if args.iter().all(|arg| self.off_arena(arg)) {
            return;
        }
        let Some(place) = self.retaining_place(func, args) else {
            return;
        };
        let pool = pool.pool.clone();
        let callee = callee_label(func);
        self.record_error(TypeError::PoolValueRetainedByCallee {
            callee,
            place,
            pool,
            span,
        });
    }

    /// The outliving place this call gives the callee write access to, if any.
    ///
    /// Write access is read from the signature, not from the callee's body: a `&mut`
    /// parameter, or a `&mut self` receiver, is the whole set of channels through which
    /// a callee can leave something behind in its caller. Whether the body actually
    /// stores through one is not asked, for the same reason `off_arena` does not ask
    /// what a callee allocates — an unproven answer has to be the one that keeps the
    /// value inside the block.
    fn retaining_place(&self, func: &Expr, args: &[Expr]) -> Option<String> {
        let key = self.callee_key(func)?;

        // The receiver is not in `args`; it rides inside a field-access callee, and its
        // mutability lives in `mut_self_methods` rather than in the signature, where
        // `self` is recorded as the bare struct type.
        let receiver = self.callee_operand(func);
        if let Some(object) = receiver {
            if self.mut_self_methods.contains(&key) {
                if let Some(root) = self.outliving_root(object) {
                    return Some(root);
                }
            }
        }

        let Some(Type::Function { params, .. }) = self.functions.get(&key) else {
            return None;
        };
        // An instance method's signature carries the implicit `self` in front of the
        // declared parameters, so the positional arguments start one later.
        let declared = match receiver {
            Some(_) => params.get(1..)?,
            None => params.as_slice(),
        };
        declared
            .iter()
            .zip(args)
            .filter(|(param, _)| matches!(param, Type::Reference { mutable: true, .. }))
            .find_map(|(_, arg)| self.outliving_root(arg))
    }

    /// The binding a place expression is rooted in, quoted for a diagnostic, when that
    /// binding was declared before every open pool. `None` when the expression is not
    /// rooted in such a binding, which includes every expression that is not a place.
    fn outliving_root(&self, place: &Expr) -> Option<String> {
        let name = root_binding(place)?;
        self.declared_before_pools(name)
            .then(|| format!("'{name}'"))
    }

    /// The receiver a method call passes as `self`, which the operand walk must clear
    /// alongside the explicit arguments. `None` for a call that has none.
    fn callee_operand<'a>(&self, func: &'a Expr) -> Option<&'a Expr> {
        match func {
            Expr::FieldAccess { object, .. } => Some(object),
            _ => None,
        }
    }

    fn method_key(&self, struct_name: &str, method: &str) -> Option<&String> {
        self.impl_methods.get(struct_name)?.get(method)
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
        let Some(name) = self.drop_only_within(ty, &mut HashSet::new()) else {
            return;
        };
        let origin = match produced_by {
            Some(func) => format!(" returned by '{func}'"),
            None => String::new(),
        };
        let pool = pool.pool.clone();
        self.record_error(TypeError::PoolDropOnlyValue {
            type_name: name,
            origin,
            pool,
            span,
        });
    }

    /// The first `Drop`-only type the pool would own through `ty`, if any.
    ///
    /// A destructor costs the arena the same whether the value wearing it is named
    /// directly or reached through an aggregate, so the rule looks through arrays,
    /// tuples, newtypes, enum payloads and struct fields rather than at the annotation
    /// alone. A `PoolAware` type answers for everything it holds, so the walk stops
    /// there. `seen` keeps a nominal cycle from recursing forever.
    fn drop_only_within(&self, ty: &Type, seen: &mut HashSet<String>) -> Option<String> {
        match ty {
            Type::Struct(name) => {
                if self
                    .trait_impls
                    .contains(&(POOL_AWARE_TRAIT.to_string(), name.clone()))
                {
                    return None;
                }
                if self.drop_structs.contains(name) {
                    return Some(name.clone());
                }
                if !seen.insert(name.clone()) {
                    return None;
                }
                self.struct_defs
                    .get(name)?
                    .iter()
                    .find_map(|(_, field)| self.drop_only_within(field, seen))
            }
            Type::Enum(name) => {
                if !seen.insert(name.clone()) {
                    return None;
                }
                self.enum_defs
                    .get(name)?
                    .iter()
                    .flat_map(|variant| variant.fields.iter())
                    .find_map(|(_, payload)| self.drop_only_within(payload, seen))
            }
            Type::Newtype(name) => {
                if !seen.insert(name.clone()) {
                    return None;
                }
                self.drop_only_within(self.newtype_defs.get(name)?, seen)
            }
            Type::Array { element, .. } => self.drop_only_within(element, seen),
            Type::Tuple(elements) => elements
                .iter()
                .find_map(|element| self.drop_only_within(element, seen)),
            _ => None,
        }
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

/// The binding at the base of a place expression: the one a write through the
/// expression ultimately lands in.
fn root_binding(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Identifier(id) => Some(&id.name),
        Expr::Paren(inner, _) => root_binding(inner),
        Expr::Reference { operand, .. } | Expr::Deref { operand, .. } => root_binding(operand),
        Expr::FieldAccess { object, .. } | Expr::Index { object, .. } => root_binding(object),
        _ => None,
    }
}

/// The callee's own name, quoted for a diagnostic. A callee with no name to quote is
/// already excluded by [`TypeChecker::callee_key`].
fn callee_label(func: &Expr) -> String {
    let name = match func {
        Expr::Identifier(id) => &id.name,
        Expr::Path { member, .. } => &member.name,
        Expr::FieldAccess { field, .. } => &field.name,
        _ => return "the callee".to_string(),
    };
    format!("'{name}'")
}

/// Whether a value of this type can cross a pool boundary whatever its provenance.
///
/// Only types that carry no pointer at all qualify. A `string`, collection, tensor,
/// reference or struct MAY hold an address into the arena, so one of those is admitted
/// only when [`TypeChecker::off_arena`] proves this particular value does not.
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

    /// The rejection reads through an aggregate: a value returned as a struct field, a
    /// tuple element or an array element costs the arena the same per-object destructor
    /// as a bare one.
    #[test]
    fn a_drop_only_value_inside_an_aggregate_is_rejected() {
        let errs = errors(&format!(
            "{DROP_ONLY}
struct Wrapper {{ inner: Handle }}
enum Slot {{ Full(Handle), Empty }}
func make_wrapper() -> Wrapper {{ Wrapper {{ inner: Handle {{ id: 1 }} }} }}
func make_pair() -> (Handle, i32) {{ (Handle {{ id: 2 }}, 7) }}
func make_arr() -> [Handle; 2] {{ [Handle {{ id: 3 }}, Handle {{ id: 4 }}] }}
func make_slot() -> Slot {{ Slot::Full(Handle {{ id: 5 }}) }}
func main() -> i32 {{
    pool scratch {{
        val w = make_wrapper()
        val p = make_pair()
        val a = make_arr()
        val s = make_slot()
    }}
    return 0
}}"
        ));
        assert_eq!(
            errs.iter()
                .filter(|e| e.contains("'Handle' implements 'Drop'"))
                .count(),
            4,
            "expected every aggregate to be rejected, got {errs:?}"
        );
    }

    /// An aggregate of types that own nothing keeps crossing the boundary.
    #[test]
    fn an_aggregate_without_a_drop_type_is_untouched() {
        let errs = errors(&format!(
            "{DROP_ONLY}
struct Plain {{ n: i32 }}
func make_plain() -> Plain {{ Plain {{ n: 1 }} }}
func make_pair() -> (i32, i32) {{ (1, 2) }}
func main() -> i32 {{
    pool {{
        val p = make_plain()
        val q = make_pair()
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

    /// The prior rule read the place's type and nothing else, so a value the block never
    /// allocated was refused along with one it did. A call to a declared function is
    /// emitted as its own body outside every arena, which is provable from the source.
    #[test]
    fn a_value_a_declared_function_built_may_cross_the_boundary() {
        let errs = errors(
            "func label(n: i32) -> string { \"row {n}\" }
func main() -> i32 {
    mut out: string = \"\"
    pool {
        out = label(1)
    }
    return 0
}",
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// The same call with an operand the block allocated: the callee can hand that very
    /// pointer back, so the result is arena memory again.
    #[test]
    fn a_call_taking_an_arena_operand_is_still_rejected() {
        let errs = errors(
            "func echo(s: string) -> string { s }
func main() -> i32 {
    mut out: string = \"\"
    pool scratch {
        out = echo(\"a\" + \"b\")
    }
    return 0
}",
        );
        assert!(
            errs.iter().any(|e| e.contains("outlives")),
            "expected an escape rejection, got {errs:?}"
        );
    }

    /// The language rule's own example of an unprovable owner: behind a trait object the
    /// callee is not known until runtime, so neither is what it allocates.
    #[test]
    fn a_value_from_a_dyn_call_may_not_cross_the_boundary() {
        let errs = errors(
            "trait Namer { func name(&self) -> string }
struct Plain { tag: i32 }
impl Namer for Plain {
    func name(&self) -> string { \"plain\" }
}
func main() -> i32 {
    val p = Plain { tag: 1 }
    val d: &dyn Namer = &p
    mut out: string = \"\"
    pool scratch {
        out = d.name()
    }
    return 0
}",
        );
        assert!(
            errs.iter().any(|e| e.contains("outlives")),
            "expected the dyn call to be refused, got {errs:?}"
        );
    }

    /// The same method reached on the concrete type is provable, which is what makes the
    /// test above a statement about dispatch rather than about the method.
    #[test]
    fn the_same_method_on_a_concrete_receiver_is_accepted() {
        let errs = errors(
            "trait Namer { func name(&self) -> string }
struct Plain { tag: i32 }
impl Namer for Plain {
    func name(&self) -> string { \"plain\" }
}
func main() -> i32 {
    val p = Plain { tag: 1 }
    mut out: string = \"\"
    pool scratch {
        out = p.name()
    }
    return 0
}",
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// A receiver the block itself declared may hold arena memory, so a method on it
    /// proves nothing about its result even though the method is declared code.
    #[test]
    fn a_method_on_a_receiver_the_block_built_is_rejected() {
        let errs = errors(
            "struct Wrap { tag: i32 }
impl Wrap {
    func name(&self) -> string { \"wrapped\" }
}
func main() -> i32 {
    mut out: string = \"\"
    pool scratch {
        val w = Wrap { tag: 1 }
        out = w.name()
    }
    return 0
}",
        );
        assert!(
            errs.iter().any(|e| e.contains("outlives")),
            "expected an escape rejection, got {errs:?}"
        );
    }

    /// A scalar operand carries no pointer at all, so a call taking one proves as much
    /// as a call taking none. Without this the rule would refuse every summary line a
    /// loop counter feeds, which is the shape it exists to permit.
    #[test]
    fn a_scalar_the_block_declared_is_a_safe_operand() {
        let errs = errors(
            "func label(n: i32) -> string { \"row {n}\" }
func main() -> i32 {
    mut out: string = \"\"
    pool {
        val step = 3
        out = label(step)
    }
    return 0
}",
        );
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// A `&mut self` method can stash its argument in a receiver that outlives the
    /// block, which no rule watching the block's own stores can see. This is the shape
    /// the retention rule exists for.
    const STASH: &str = "\
struct Box { s: string }
impl Box {
    func stash(&mut self, v: string) { self.s = v }
    func peek(&self, v: string) -> i32 { 1 }
}
";

    #[test]
    fn a_callee_storing_an_arena_argument_is_rejected() {
        let errs = errors(&format!(
            "{STASH}
func main() -> i32 {{
    mut b = Box {{ s: \"\" }}
    pool scratch {{
        b.stash(\"a\" + \"b\")
    }}
    return 0
}}"
        ));
        assert!(
            errs.iter()
                .any(|e| e.contains("'stash'") && e.contains("'b'") && e.contains("'scratch'")),
            "expected the callee, the place and the pool named, got {errs:?}"
        );
    }

    /// The rule turns on the argument's provenance, not on the method: a value the
    /// block never allocated may be stashed anywhere.
    #[test]
    fn a_callee_storing_an_off_arena_argument_is_accepted() {
        let errs = errors(&format!(
            "{STASH}
func label() -> string {{ \"fixed\" }}
func main() -> i32 {{
    mut b = Box {{ s: \"\" }}
    pool {{
        b.stash(\"literal\")
        b.stash(label())
    }}
    return 0
}}"
        ));
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// A `&self` method has no channel to write back through, so it keeps taking
    /// arena arguments. Without this the rule would refuse every read of a temporary.
    #[test]
    fn a_read_only_method_on_an_outer_receiver_is_accepted() {
        let errs = errors(&format!(
            "{STASH}
func main() -> i32 {{
    val b = Box {{ s: \"\" }}
    pool {{
        val n = b.peek(\"a\" + \"b\")
    }}
    return 0
}}"
        ));
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// A receiver the block itself declared dies with the arena, so a store into it is
    /// exactly as safe as the value being stored.
    #[test]
    fn a_callee_storing_into_a_receiver_the_block_built_is_accepted() {
        let errs = errors(&format!(
            "{STASH}
func main() -> i32 {{
    pool {{
        mut b = Box {{ s: \"\" }}
        b.stash(\"a\" + \"b\")
    }}
    return 0
}}"
        ));
        assert!(errs.is_empty(), "expected no errors, got {errs:?}");
    }

    /// Place expressions made a plain `&mut` parameter assignable, so `&mut self` on a
    /// method stopped being the only spelling that reaches the caller's memory.
    #[test]
    fn a_mut_reference_parameter_is_the_same_channel() {
        let errs = errors(&format!(
            "{STASH}
func stash_into(target: &mut Box, v: string) {{ target.s = v }}
func main() -> i32 {{
    mut b = Box {{ s: \"\" }}
    pool scratch {{
        stash_into(&mut b, \"a\" + \"b\")
    }}
    return 0
}}"
        ));
        assert!(
            errs.iter()
                .any(|e| e.contains("'stash_into'") && e.contains("'b'")),
            "expected a plain &mut parameter to be caught, got {errs:?}"
        );
    }

    /// A shared reference parameter is not a channel back, so it stays open.
    #[test]
    fn a_shared_reference_parameter_is_not_a_channel() {
        let errs = errors(
            "struct Box { s: string }
func read_from(target: &Box, v: string) -> i32 { 1 }
func main() -> i32 {
    val b = Box { s: \"\" }
    pool {
        val n = read_from(&b, \"a\" + \"b\")
    }
    return 0
}",
        );
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
