// Top-level item AST nodes

use shared_types::{Identifier, Span};

use super::expressions::Expr;
use super::statements::Stmt;
use super::types::Type;

/// Which module a declaration came from.
///
/// Module resolution merges every loaded file into one flat item list, so the module a
/// declaration was written in is otherwise unrecoverable downstream — and visibility is
/// exactly the rule that needs it. The parser stamps 0 on everything it produces.
pub type ModuleId = u32;

/// What a generic parameter binds.
///
/// A `Type` parameter (`T`) is substituted with a concrete type at each instantiation.
/// A `Const` parameter (`const N: u32`) is a compile-time *value* of the carried
/// integer type, usable in value position and as an array length; each distinct
/// value produces a distinct monomorphized instance.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericParamKind {
    /// A type parameter `T`.
    Type,
    /// A const (value) parameter `const N: T`, carrying its declared integer type.
    Const(Type),
}

/// A single generic parameter in a `<...>` list: `T`, `T: Bound + Bound`, or
/// `const N: u32`.
///
/// `bounds` records the trait names syntactically (from either the inline `T: Bound`
/// form or a `where` clause), but they are **not enforced** in this phase — the trait
/// system does not exist yet, so a bound is parsed for forward compatibility and
/// ignored by later passes. `kind` distinguishes a type parameter from a const (value)
/// parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: Identifier,
    pub kind: GenericParamKind,
    pub bounds: Vec<Identifier>,
    pub span: Span,
}

/// Function definition.
///
/// `generics` is the `<T, U>` type-parameter list; it is empty for an ordinary
/// (non-generic) function. A generic function is a *template* — later passes
/// monomorphize it into one concrete function per distinct set of type arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. Items are private to their module
    /// unless they opt in; the flag is inert in a single-module program.
    pub exported: bool,
    /// The module this declaration was loaded from, stamped during module resolution.
    /// Everything the parser produces is module 0 — a single-file program is one module —
    /// so a program that never reaches the resolver behaves as it always did.
    pub module: ModuleId,
    pub generics: Vec<GenericParam>,
    /// Explicit lifetime parameters, the `'a` names in `func f<'a>(...)`.
    /// Kept separate from `generics` because lifetimes are a distinct namespace and,
    /// unlike type/const parameters, do NOT drive monomorphization — a function with
    /// only lifetime parameters is an ordinary concrete function. Erased after
    /// borrow-check; the elision-based outlives analysis does the real work.
    pub lifetimes: Vec<Identifier>,
    /// Value predicates from a `where` clause, e.g. `where N > 0`. Each is a
    /// boolean expression over the function's const parameters, evaluated at every
    /// instantiation against the concrete values; a violated predicate is an error at
    /// the offending call. Trait bounds in a `where` clause are folded into the
    /// matching parameter's `bounds` instead (they are unenforced this phase).
    pub where_predicates: Vec<Expr>,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// A single `@name(arg1, arg2)` attribute attached to a function or method.
///
/// The semantics of an attribute are interpreted by later passes (e.g. the
/// `@allow(prefer_loop_over_while_true)` lint suppression in semantic analysis).
/// Unknown attributes are accepted by the parser to keep the surface forward
/// compatible with future passes such as `@grad`, `@gpu`, and `@no_prelude`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Identifier,
    pub args: Vec<Identifier>,
    pub span: Span,
}

/// How a parameter may be named at the call site.
///
/// The external label is what a caller writes; the internal name is what the body uses.
/// The three forms are syntactically distinct at the declaration, so a call site's
/// obligations are fixed by the signature alone.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamLabel {
    /// `name: T` — the caller may pass positionally *or* write `name: value`.
    Implicit,
    /// `external name: T` — the caller MUST write `external: value`.
    External(Identifier),
    /// `_ name: T` — the caller MUST pass positionally; the name is not accepted.
    Suppressed,
}

impl ParamLabel {
    /// The name a caller is allowed to write for this parameter, given the parameter's
    /// internal name. `None` means the call site accepts no name at all.
    pub fn call_site_name<'a>(&'a self, internal: &'a Identifier) -> Option<&'a str> {
        match self {
            ParamLabel::Implicit => Some(internal.name.as_str()),
            ParamLabel::External(label) => Some(label.name.as_str()),
            ParamLabel::Suppressed => None,
        }
    }

    /// Whether omitting the name is an error at every call site.
    pub fn is_required(&self) -> bool {
        matches!(self, ParamLabel::External(_))
    }
}

/// Function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// The call-site naming rule for this parameter. Defaults to
    /// [`ParamLabel::Implicit`] for the ordinary `name: T` form.
    pub label: ParamLabel,
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// A single field in a struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: Identifier,
    /// `true` when the field carries `export`, or when it belongs to an enum
    /// struct-variant — a variant's shape is part of the enum it is matched through,
    /// so its fields follow the enum rather than carrying visibility of their own.
    pub exported: bool,
    pub ty: Type,
    pub span: Span,
}

/// Struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. See [`FunctionDef::exported`].
    pub exported: bool,
    /// The module this declaration was loaded from. See [`FunctionDef::module`].
    pub module: ModuleId,
    /// `generics` is the `<T, U>` type-parameter list; empty for a
    /// non-generic struct. A generic struct is a *template* — later passes
    /// monomorphize it into one concrete struct per distinct set of type arguments.
    pub generics: Vec<GenericParam>,
    /// Explicit lifetime parameters, the `'a` names in `struct S<'a> { ... }`.
    /// Distinct from `generics` (see [`FunctionDef::lifetimes`]); erased after
    /// borrow-check.
    pub lifetimes: Vec<Identifier>,
    /// Value predicates from a `where` clause over the struct's const
    /// parameters, checked at each instantiation (see [`FunctionDef::where_predicates`]).
    pub where_predicates: Vec<Expr>,
    pub fields: Vec<FieldDef>,
    /// `@derive(...)` attributes attached to the struct (e.g. `@derive(Copy, Clone)`).
    /// Interpreted by semantic analysis to determine Copy/Clone-ness.
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// The `self` parameter kind in a method signature.
///
/// `Owned` (consuming `self`) is parsed but rejected by semantic analysis until
/// the by-value struct ABI lands; `&self` and `&mut self` are supported.
#[derive(Debug, Clone, PartialEq)]
pub enum SelfParam {
    /// `&self` — immutable borrow; lowered to pass-by-value in codegen.
    Ref,
    /// `&mut self` — mutable borrow; lowered to pass-by-pointer so field writes
    /// in the method body propagate to the caller's value.
    RefMut,
    /// `self` — consuming; not yet supported (needs the by-value struct ABI).
    Owned,
}

/// A method inside an `impl` block.
///
/// Methods with `self_param: None` are associated functions (called via
/// `TypeName::func_name(args)`). Methods with `self_param: Some(_)` are
/// instance methods (called via `instance.method_name(args)`).
#[derive(Debug, Clone, PartialEq)]
pub struct MethodDef {
    pub name: Identifier,
    /// None for associated functions, Some for instance methods.
    pub self_param: Option<SelfParam>,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// An `impl` block associating methods with a named struct type.
///
/// `trait_name` is `Some` for a trait implementation (`impl Drawable for T`) and
/// `None` for a plain inherent block (`impl T`). `Drop` is a compiler-known lang-item
/// Any other trait name must resolve to a user `trait` declaration,
/// against which semantic analysis checks the impl for conformance.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplDef {
    /// The module this block was loaded from. See [`FunctionDef::module`]. An `impl`
    /// declares no name of its own, so it has no `exported` flag: its methods are
    /// reachable wherever the type they extend is.
    pub module: ModuleId,
    pub trait_name: Option<Identifier>,
    pub type_name: Identifier,
    /// `generics` is the impl-level `<T, U>` type-parameter list, as in
    /// `impl<T> Wrapper<T>`; empty for a non-generic impl.
    pub generics: Vec<GenericParam>,
    /// Explicit lifetime parameters, the `'a` names in `impl<'a> S<'a>`.
    /// Distinct from `generics` (see [`FunctionDef::lifetimes`]); erased after
    /// borrow-check.
    pub lifetimes: Vec<Identifier>,
    /// Type arguments applied to `type_name`, as in the `<T>` of `impl<T> Wrapper<T>`.
    /// Empty for a plain `impl Name` block. Each argument typically names an impl
    /// generic parameter; monomorphization maps them positionally to the struct's
    /// concrete type arguments.
    pub type_args: Vec<Type>,
    /// Value predicates from an impl-level `where` clause, checked at each
    /// instantiation (see [`FunctionDef::where_predicates`]).
    pub where_predicates: Vec<Expr>,
    /// Associated-type bindings declared inside the block: the `type Output = Vec2` of
    /// an operator-trait impl, or the `type Item = u32` that answers a user trait's
    /// [`TraitDef::assoc_types`] declaration. Each entry is `(name, bound type)`. Empty
    /// for blocks with no `type` items.
    pub assoc_types: Vec<(Identifier, Type)>,
    pub methods: Vec<MethodDef>,
    pub span: Span,
}

/// A compile-time constant declaration at module scope.
///
/// The type annotation is mandatory; the value must be a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. See [`FunctionDef::exported`].
    pub exported: bool,
    /// The module this declaration was loaded from. See [`FunctionDef::module`].
    pub module: ModuleId,
    pub ty: Type,
    pub value: super::expressions::Expr,
    pub span: Span,
}

/// The data a single enum variant carries.
///
/// A variant is one of three shapes: a bare tag, a positional tuple of payload
/// types, or a set of named fields. The payload types are restricted to scalar
/// `Copy` primitives by semantic analysis (a documented Phase-1E limitation); the
/// AST itself imposes no restriction.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantPayload {
    /// A bare variant with no data: `Red`.
    Unit,
    /// A tuple variant with positional fields: `Move(i32, i32)`.
    Tuple(Vec<Type>),
    /// A struct-like variant with named fields: `Circle { radius: f64 }`.
    Struct(Vec<FieldDef>),
}

/// A single variant in an enum definition.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub payload: VariantPayload,
    pub span: Span,
}

/// Enum definition: a tagged union of named variants, each optionally
/// carrying associated data.
///
/// `generics` is the `<T, E>` type-parameter list; empty for a non-generic enum.
/// A generic enum is a *template* — later passes monomorphize it into one concrete
/// tagged union per distinct set of type arguments, exactly as they do for a generic
/// struct. `Option<T>` and `Result<T, E>` are ordinary generic enums built this way.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. See [`FunctionDef::exported`].
    pub exported: bool,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// A `newtype` declaration: a distinct nominal type wrapping `inner`.
///
/// Unlike a transparent `type` alias (which is expanded away at parse time), a
/// newtype survives to semantic analysis as its own type — the wrapper and the
/// inner type are not interchangeable. Construction is `Name(value)` and the inner
/// value is read via `.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewtypeDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. See [`FunctionDef::exported`].
    pub exported: bool,
    pub inner: Type,
    pub span: Span,
}

/// A single method declaration inside a `trait` block.
///
/// A `default_body` of `None` is a **required** method — implementors must provide
/// one. `Some(body)` is a **provided** (default) method whose body is copied into any
/// implementor that omits it. The signature mirrors [`MethodDef`] minus `attributes`
/// (traits carry no per-method attributes this phase).
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: Identifier,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    /// `None` for a required method, `Some(body)` for a default method.
    pub default_body: Option<Vec<Stmt>>,
    pub span: Span,
}

/// A `trait` declaration: a set of method signatures defining shared behavior.
///
/// Traits are fully monomorphized and erased — there is no vtable and no runtime trait
/// object this phase (`dyn` dispatch is). A trait produces no code on its own;
/// each `impl Trait for Type` block lowers to ordinary inherent methods, and any default
/// method the implementor omits is copied in as a concrete method.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    pub name: Identifier,
    /// `true` when the declaration carries `export`. See [`FunctionDef::exported`].
    pub exported: bool,
    /// Associated type names the trait declares (`type Item`), in declaration order.
    /// A declaration carries the name only: what it stands for is chosen by each
    /// implementor's `type Item = T` binding, which is why the two sides live on
    /// different nodes ([`ImplDef::assoc_types`] holds the bindings).
    pub assoc_types: Vec<Identifier>,
    pub methods: Vec<TraitMethod>,
    pub span: Span,
}

/// What an `import` declaration takes from the path it names.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportSelection {
    /// `import math` — the module itself, brought in under its own name.
    Module,
    /// `import math::matrix as mat` / `import math::sqrt as root` — the last path
    /// segment under a new name. Whether that segment is a module or an item is a
    /// question about the file system, so it is settled during module resolution.
    Alias(Identifier),
    /// `import math::{sqrt, sin}` — a brace list of names taken from the path.
    List(Vec<ImportName>),
}

/// One entry of an `import path::{...}` list, optionally renamed.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportName {
    pub name: Identifier,
    /// The `as` name this entry is bound under, when it is renamed.
    pub alias: Option<Identifier>,
    pub span: Span,
}

/// An `import` declaration.
///
/// The declaration is consumed by module resolution and never reaches semantic
/// analysis: after resolution every name is unqualified and the program is flat.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDef {
    /// `true` for the explicitly relative `import ./utils` form. Every module path is
    /// resolved relative to the importing file either way; the marker records what the
    /// author wrote.
    pub relative: bool,
    /// The `::`-separated segments before any `{...}` list or `as` alias.
    pub path: Vec<Identifier>,
    pub selection: ImportSelection,
    /// `true` for the `export import` re-export form: the names this declaration binds
    /// are also reachable *through* the importing module, as `importer::name`.
    pub exported: bool,
    pub span: Span,
}

/// An inline `module Name { ... }` block.
///
/// A block is a module in every sense a file is one — its items are private unless
/// written with `export`, and a qualified path reaches into it — so module resolution
/// treats it as a module that happens to have no file of its own, and nothing about the
/// block survives that pass.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDef {
    pub name: Identifier,
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top-level AST item
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Const(ConstDef),
    Newtype(NewtypeDef),
    Import(ImportDef),
    Module(ModuleDef),
    /// A file-scope `@no_prelude` marker. Carries only where it was written: module
    /// resolution reads it off the file it opens and drops it, so no later pass sees it.
    NoPrelude(Span),
}
