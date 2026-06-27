//! Neuro Programming Language - AST → HIR Lowering
//!
//! Feature slice implementing the Phase 1.8 lowering strategy: it consumes the
//! type-checked surface AST ([`ast_types`]) and produces the typed High-Level IR
//! ([`neuro_hir`]) that every backend lowers from.
//!
//! ```text
//! AST  ─►  hir-lowering  ─►  neuro-hir (typed HIR)  ─►  llvm-backend / mlir-backend
//! ```
//!
//! # Why a self-contained re-derivation
//!
//! The HIR's defining property is that **every expression carries its resolved
//! type** ([`neuro_hir::HirExpr::ty`]). The frontend type checker computes those
//! types but does not expose them — and importing its internal `Type` would couple
//! two feature slices, which VSA forbids. This slice therefore re-derives each
//! expression's type from the AST, mirroring the checker's rules. Because lowering
//! only runs on a program that already type-checked, it assumes well-typedness:
//! it computes types rather than validating them, and a shape that "cannot happen"
//! in a checked program surfaces as a [`LoweringError`] instead of a panic.
//!
//! # Entry point
//!
//! [`lower_program`] is the only public function. It returns a [`neuro_hir::HirProgram`].

use std::collections::{HashMap, HashSet};

use ast_types::Item;
use neuro_hir::{HirProgram, HirType};

mod expressions;
mod items;
mod statements;
mod types;

#[cfg(test)]
mod tests;

/// A failure encountered while lowering a type-checked program to HIR.
///
/// Every variant denotes a state the type checker should have already rejected;
/// lowering surfaces it as an error rather than panicking so the pipeline never
/// aborts the process on malformed input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoweringError {
    /// A type annotation named a type that does not resolve (unknown name, or a
    /// `Tensor<...>` that the current surface does not support).
    #[error("cannot resolve type annotation '{name}' during lowering")]
    UnresolvedType { name: String },

    /// An identifier referenced no in-scope binding or constant.
    #[error("unresolved binding '{name}' during lowering")]
    UnresolvedBinding { name: String },

    /// A method, builtin intrinsic, or associated function could not be resolved
    /// on the receiver/type it was called on.
    #[error("unresolved call target '{target}' during lowering")]
    UnresolvedCall { target: String },

    /// An expression appeared in a position whose type the well-typed contract
    /// guarantees against (e.g. a `??` operator the checker rejects, or indexing a
    /// non-array). Carries a short description for diagnosis.
    #[error("malformed expression reached lowering: {detail}")]
    Malformed { detail: String },
}

/// Per-active-loop state used to resolve `break`/`continue` targets and to type a
/// `loop` used as a value expression (§3.7), mirroring the checker's loop stack.
struct LoopCtx {
    /// The loop's label (`outer:`), or `None` when unlabeled.
    label: Option<String>,
    /// Whether the loop can yield a value via `break v` — only a `loop` can.
    is_value: bool,
    /// The agreed type of value-carrying `break`s seen so far, `None` until the
    /// first one. The loop expression evaluates to this (or `void` when absent).
    value_ty: Option<HirType>,
}

/// Lowering state: the global symbol tables built in a pre-pass plus the
/// per-body scope and loop stacks. Mirrors the subset of the type checker's state
/// needed to re-derive expression types.
struct Lowerer {
    /// Free functions and mangled methods → (parameter types, return type).
    functions: HashMap<String, (Vec<HirType>, HirType)>,
    /// Struct name → ordered `(field_name, field_type)` list.
    structs: HashMap<String, Vec<(String, HirType)>>,
    /// Structs that support `.clone()` (derive `Clone`, or `Copy` which implies it).
    clone_structs: HashSet<String>,
    /// Struct name → method name → mangled key into [`Self::functions`].
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Module- and function-scope constant names → resolved type.
    constants: HashMap<String, HirType>,
    /// Lexical scope stack of `binding name → type`, innermost last.
    scopes: Vec<HashMap<String, HirType>>,
    /// Active loops, innermost last.
    loop_stack: Vec<LoopCtx>,
    /// The current function/method's resolved return type (for `return` typing).
    current_return: HirType,
}

/// Lower a type-checked program to typed HIR.
///
/// The `items` must come from a program that passed `semantic_analysis::type_check`;
/// lowering assumes well-typedness and re-derives each expression's resolved type.
///
/// # Errors
///
/// Returns a [`LoweringError`] if an AST shape the type checker should have rejected
/// reaches lowering (an unresolved type, binding, or call target). A well-typed
/// program never triggers these.
pub fn lower_program(items: &[Item]) -> Result<HirProgram, LoweringError> {
    let mut lowerer = Lowerer::new();
    lowerer.register_items(items)?;
    lowerer.lower_program(items)
}

impl Lowerer {
    fn new() -> Self {
        Self {
            functions: HashMap::new(),
            structs: HashMap::new(),
            clone_structs: HashSet::new(),
            impl_methods: HashMap::new(),
            constants: HashMap::new(),
            scopes: Vec::new(),
            loop_stack: Vec::new(),
            current_return: HirType::Void,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Define a binding in the innermost scope. A missing scope is a lowering bug
    /// (every body opens a scope first), so the define is silently dropped rather
    /// than panicking — the subsequent lookup would surface it as an error.
    fn define(&mut self, name: String, ty: HirType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Resolve a binding's type: innermost scope outward, then module constants —
    /// matching the checker's "locals shadow constants" precedence.
    fn lookup(&self, name: &str) -> Option<HirType> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        self.constants.get(name).cloned()
    }
}

/// Whether `t` is a signed or unsigned integer type.
fn is_integer(t: &HirType) -> bool {
    matches!(
        t,
        HirType::I8
            | HirType::I16
            | HirType::I32
            | HirType::I64
            | HirType::U8
            | HirType::U16
            | HirType::U32
            | HirType::U64
    )
}

/// Whether `t` is a full-precision float (`f32`/`f64`). Half-precision is excluded,
/// matching the checker's contextual-inference predicate (§1.2).
fn is_full_float(t: &HirType) -> bool {
    matches!(t, HirType::F32 | HirType::F64)
}

/// Whether `t` is `string` or a borrow of `string` (`&string` slice). Used to detect
/// the string-concatenation form of `+` (§2.7).
fn peels_to_string(t: &HirType) -> bool {
    matches!(t.referent(), HirType::String)
}
