//! AST → HIR lowering.
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
mod operator_traits;
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

/// A resolved enum variant for lowering: its name and ordered payload fields. The
/// optional name distinguishes a struct variant's field from a tuple variant's
/// positional element; reordering struct-literal fields into this declared order is
/// how the single [`neuro_hir::HirExprKind::EnumConstruct`] payload is built.
struct EnumVariantData {
    name: String,
    fields: Vec<(Option<String>, HirType)>,
}

/// Per-active-loop state used to resolve `break`/`continue` targets and to type a
/// `loop` used as a value expression, mirroring the checker's loop stack.
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
    /// Enum name → ordered variants. Each variant carries its name and ordered
    /// payload fields `(optional field name, type)`; the index doubles as the tag.
    enums: HashMap<String, Vec<EnumVariantData>>,
    /// Newtype name → its inner surface type. Kept as the AST type so a
    /// newtype annotation resolves recursively (a newtype may wrap another newtype).
    newtypes: HashMap<String, ast_types::Type>,
    /// Structs that support `.clone()` (derive `Clone`, or `Copy` which implies it).
    clone_structs: HashSet<String>,
    /// Struct name → method name → mangled key into [`Self::functions`].
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Declared traits → their methods in declaration order. The order is
    /// the vtable slot order for dynamic dispatch; each entry carries the method's
    /// visible (non-`self`) parameter types and return type so a call through a
    /// `&dyn Trait` receiver types without consulting any implementor.
    traits: HashMap<String, Vec<TraitMethodInfo>>,
    /// Operator-trait dispatch: `(struct name, binary operator)` → how the
    /// operator desugars to a method call. Present when the struct has an operator impl.
    operator_binary_impls: HashMap<(String, ast_types::BinaryOp), OpDispatch>,
    /// Operator-trait dispatch for unary operators: `(struct name, operator)` →
    /// `(method name, result type)`.
    operator_unary_impls: HashMap<(String, ast_types::UnaryOp), (String, HirType)>,
    /// Module- and function-scope constant names → resolved type.
    constants: HashMap<String, HirType>,
    /// Lexical scope stack of `binding name → type`, innermost last.
    scopes: Vec<HashMap<String, HirType>>,
    /// Active loops, innermost last.
    loop_stack: Vec<LoopCtx>,
    /// The current function/method's resolved return type (for `return` typing).
    current_return: HirType,
    /// Generic free-function templates, keyed by name. A generic function is
    /// never lowered as-is; each distinct set of type arguments produces one
    /// monomorphized concrete function instead.
    generic_templates: HashMap<String, ast_types::FunctionDef>,
    /// Generic struct templates, keyed by name. Each distinct set of type
    /// arguments produces one monomorphized concrete struct emitted as an ordinary
    /// [`neuro_hir::HirItem::Struct`], so backends need no generic awareness.
    generic_structs: HashMap<String, ast_types::StructDef>,
    /// Generic `impl` templates keyed by base struct name. Instantiating a
    /// generic struct also emits each matching impl's methods for the instance.
    generic_impls: HashMap<String, Vec<ast_types::ImplDef>>,
    /// Mangled names of generic-struct instances already materialized (registered in
    /// [`Self::structs`] and queued for emission), so each is produced exactly once.
    instantiated_structs: HashSet<String>,
    /// Generic-struct instances discovered but whose HIR items are not yet emitted.
    mono_struct_pending: Vec<MonoStruct>,
    /// Active type-parameter substitution while a monomorphized instance body is being
    /// lowered: parameter name → concrete type. Empty outside instance lowering; a
    /// `Named` annotation matching an entry resolves to the concrete type.
    type_subst: HashMap<String, HirType>,
    /// Active const-parameter substitution while a monomorphized instance body is being
    /// lowered: const parameter name → concrete value. An array length or value
    /// reference naming an entry resolves to that value. Empty outside instance lowering.
    const_subst: HashMap<String, u64>,
    /// The declared integer type of each active const parameter, so a value
    /// reference to one lowers to a correctly-typed integer literal. Parallel to
    /// [`Self::const_subst`].
    const_types: HashMap<String, HirType>,
    /// Monomorphization worklist: instances discovered but not yet lowered.
    mono_pending: Vec<MonoInstance>,
    /// Mangled names already queued or emitted, so each instance is produced once.
    mono_seen: HashSet<String>,
    /// Concrete instance functions produced by monomorphization, appended to the
    /// program after the ordinary items.
    mono_items: Vec<neuro_hir::HirItem>,
}

/// One trait method's lowering-visible signature, in declaration order.
struct TraitMethodInfo {
    name: String,
    /// Parameter types excluding the implicit `self`.
    params: Vec<HirType>,
    ret: HirType,
}

/// How a binary operator desugars to an operator-trait method call.
struct OpDispatch {
    /// The backing method name (`add`, `eq`, `lt`, …).
    method: String,
    /// The method's declared right-hand parameter type. A `Reference` means the operand
    /// is borrowed at the call (the comparison traits take `rhs: &Rhs`).
    rhs_param: HirType,
    /// The operator's result type: the impl's `Output`, or `bool` for comparisons.
    result: HirType,
}

/// One pending monomorphization: the generic template `fn_name`, the concrete type
/// arguments (in the template's type-parameter order), and the mangled instance name
/// the call site refers to.
struct MonoInstance {
    mangled: String,
    fn_name: String,
    subst: HashMap<String, HirType>,
    const_subst: HashMap<String, u64>,
}

/// One concrete generic argument in a monomorphized instance: either a type (for
/// a type parameter) or an integer value (for a const parameter). Positional against the
/// template's generic parameters in declaration order.
#[derive(Clone)]
enum MonoArg {
    Type(HirType),
    Const(u64),
}

/// One pending generic-struct instantiation: the base template name, the
/// mangled instance name, the concrete type arguments (in declaration order), and the
/// type-parameter substitution. Emission produces one `HirItem::Struct` plus one
/// `HirItem::Impl` per matching generic impl.
struct MonoStruct {
    base: String,
    mangled: String,
    subst: HashMap<String, HirType>,
    const_subst: HashMap<String, u64>,
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
            enums: HashMap::new(),
            newtypes: HashMap::new(),
            clone_structs: HashSet::new(),
            impl_methods: HashMap::new(),
            traits: HashMap::new(),
            operator_binary_impls: HashMap::new(),
            operator_unary_impls: HashMap::new(),
            constants: HashMap::new(),
            scopes: Vec::new(),
            loop_stack: Vec::new(),
            current_return: HirType::Void,
            generic_templates: HashMap::new(),
            generic_structs: HashMap::new(),
            generic_impls: HashMap::new(),
            instantiated_structs: HashSet::new(),
            mono_struct_pending: Vec::new(),
            type_subst: HashMap::new(),
            const_subst: HashMap::new(),
            const_types: HashMap::new(),
            mono_pending: Vec::new(),
            mono_seen: HashSet::new(),
            mono_items: Vec::new(),
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
/// matching the checker's contextual-inference predicate.
fn is_full_float(t: &HirType) -> bool {
    matches!(t, HirType::F32 | HirType::F64)
}

/// Whether `t` is `string` or a borrow of `string` (`&string` slice). Used to detect
/// the string-concatenation form of `+`.
fn peels_to_string(t: &HirType) -> bool {
    matches!(t.referent(), HirType::String)
}

/// Unify a generic template's parameter annotation against a concrete argument type,
/// recording each type parameter's binding in `subst`. The program is already
/// well-typed, so the structures always align; positions that mention no type
/// parameter contribute no binding.
fn unify_ast_hir(
    param: &ast_types::Type,
    arg: &HirType,
    gnames: &HashSet<String>,
    cnames: &HashSet<String>,
    subst: &mut HashMap<String, HirType>,
    const_subst: &mut HashMap<String, u64>,
) {
    match (param, arg) {
        (ast_types::Type::Named(ident), _) if gnames.contains(&ident.name) => {
            subst
                .entry(ident.name.clone())
                .or_insert_with(|| arg.clone());
        }
        (ast_types::Type::Reference { inner: pi, .. }, HirType::Reference { inner: ai, .. }) => {
            unify_ast_hir(pi, ai, gnames, cnames, subst, const_subst)
        }
        (
            ast_types::Type::Array {
                element: pe,
                size: psize,
                ..
            },
            HirType::Array {
                element: ae,
                size: asize,
            },
        ) => {
            // A const-parameter length binds that parameter to the argument's length.
            if let ast_types::ArraySize::Const(id) = psize {
                if cnames.contains(&id.name) {
                    const_subst.entry(id.name.clone()).or_insert(*asize as u64);
                }
            }
            unify_ast_hir(pe, ae, gnames, cnames, subst, const_subst)
        }
        (ast_types::Type::Tuple { elements: pe, .. }, HirType::Tuple(ae))
            if pe.len() == ae.len() =>
        {
            for (p, a) in pe.iter().zip(ae) {
                unify_ast_hir(p, a, gnames, cnames, subst, const_subst);
            }
        }
        _ => {}
    }
}

/// Resolve a fixed-size array length annotation to a concrete value. A
/// literal is taken as-is; a `const`-parameter length is looked up in `const_subst`
/// (populated while a monomorphized instance body is lowered).
fn resolve_array_size(
    size: &ast_types::ArraySize,
    const_subst: &HashMap<String, u64>,
) -> Result<usize, LoweringError> {
    match size {
        ast_types::ArraySize::Literal(n) => Ok(*n as usize),
        ast_types::ArraySize::Const(id) => const_subst
            .get(&id.name)
            .map(|v| *v as usize)
            .ok_or_else(|| LoweringError::UnresolvedType {
                name: format!("const array length '{}'", id.name),
            }),
    }
}

/// The mangled symbol name of a monomorphized function instance: the template name, the
/// `_g_` instance marker, and each type argument's mangled form in declaration order.
/// The result is a valid symbol (alphanumerics and `_` only).
///
/// The marker deliberately avoids `__`, which is reserved workspace-wide as the
/// receiver/method separator — codegen recovers a method's receiver struct by splitting
/// its symbol on `__`, so an instance name containing one would be misread as a method
/// of a struct that does not exist. [`mangle_struct_instance`] uses the same marker.
fn mangle_instance(
    name: &str,
    generics: &[ast_types::GenericParam],
    subst: &HashMap<String, HirType>,
    const_subst: &HashMap<String, u64>,
) -> String {
    let mut out = format!("{}_g", name);
    for gp in generics {
        let arg = match &gp.kind {
            ast_types::GenericParamKind::Const(_) => const_subst
                .get(&gp.name.name)
                .map(|v| format!("c{}", v))
                .unwrap_or_else(|| "unresolved".to_string()),
            ast_types::GenericParamKind::Type => subst
                .get(&gp.name.name)
                .map(mangle_type)
                .unwrap_or_else(|| "unresolved".to_string()),
        };
        out.push('_');
        out.push_str(&arg);
    }
    out
}

/// A symbol-safe token for a concrete type, used to build monomorphized instance names.
fn mangle_type(ty: &HirType) -> String {
    match ty {
        HirType::I8 => "i8".to_string(),
        HirType::I16 => "i16".to_string(),
        HirType::I32 => "i32".to_string(),
        HirType::I64 => "i64".to_string(),
        HirType::U8 => "u8".to_string(),
        HirType::U16 => "u16".to_string(),
        HirType::U32 => "u32".to_string(),
        HirType::U64 => "u64".to_string(),
        HirType::F16 => "f16".to_string(),
        HirType::BF16 => "bf16".to_string(),
        HirType::F32 => "f32".to_string(),
        HirType::F64 => "f64".to_string(),
        HirType::Bool => "bool".to_string(),
        HirType::Char => "char".to_string(),
        HirType::String => "string".to_string(),
        HirType::Void => "void".to_string(),
        HirType::Struct(n) => n.clone(),
        HirType::Enum(n) => n.clone(),
        HirType::Newtype { name, .. } => name.clone(),
        HirType::Reference { inner, mutable } => {
            format!(
                "{}ref_{}",
                if *mutable { "mut" } else { "" },
                mangle_type(inner)
            )
        }
        HirType::Array { element, size } => format!("arr{}_{}", size, mangle_type(element)),
        HirType::Tuple(elements) => {
            let parts: Vec<String> = elements.iter().map(mangle_type).collect();
            format!("tup{}_{}", elements.len(), parts.join("_"))
        }
        HirType::DynObject(name) => format!("dyn_{}", name),
        HirType::Function { .. } => "fn".to_string(),
    }
}

/// The mangled symbol name of a monomorphized generic-struct instance: the base
/// name, a `_g_` marker, and each type argument's mangled form.
///
/// Like [`mangle_instance`], this never contains `__`: codegen recovers a method's
/// receiver struct by splitting the method symbol on `__`, so a struct name with `__`
/// in it would corrupt that recovery. Once 1F puts generic methods on generic structs,
/// a method of this instance is keyed `<instance>__<method>` — exactly one `__`, which
/// only holds because neither half can introduce another.
fn mangle_struct_instance(base: &str, args: &[MonoArg]) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| match a {
            MonoArg::Type(t) => mangle_type(t),
            MonoArg::Const(v) => format!("c{}", v),
        })
        .collect();
    format!("{}_g_{}", base, parts.join("_"))
}
