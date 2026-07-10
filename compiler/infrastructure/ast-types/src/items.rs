// Top-level item AST nodes

use shared_types::{Identifier, Span};

use super::expressions::Expr;
use super::statements::Stmt;
use super::types::Type;

/// What a generic parameter binds (§3.8).
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

/// A single generic parameter in a `<...>` list (§3.8): `T`, `T: Bound + Bound`, or
/// `const N: u32`.
///
/// `bounds` records the trait names syntactically (from either the inline `T: Bound`
/// form or a `where` clause), but they are **not enforced** in this phase — the trait
/// system (§3.9) does not exist yet, so a bound is parsed for forward compatibility and
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
/// `generics` is the `<T, U>` type-parameter list (§3.8); it is empty for an ordinary
/// (non-generic) function. A generic function is a *template* — later passes
/// monomorphize it into one concrete function per distinct set of type arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Identifier,
    pub generics: Vec<GenericParam>,
    /// Value predicates from a `where` clause (§3.8), e.g. `where N > 0`. Each is a
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

/// Function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// A single field in a struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// Struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: Identifier,
    /// `generics` is the `<T, U>` type-parameter list (§3.8); empty for a
    /// non-generic struct. A generic struct is a *template* — later passes
    /// monomorphize it into one concrete struct per distinct set of type arguments.
    pub generics: Vec<GenericParam>,
    /// Value predicates from a `where` clause (§3.8) over the struct's const
    /// parameters, checked at each instantiation (see [`FunctionDef::where_predicates`]).
    pub where_predicates: Vec<Expr>,
    pub fields: Vec<FieldDef>,
    /// `@derive(...)` attributes attached to the struct (e.g. `@derive(Copy, Clone)`).
    /// Interpreted by semantic analysis to determine Copy/Clone-ness (§2.3).
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

/// The `self` parameter kind in a method signature.
///
/// `Owned` (consuming `self`) is parsed but rejected by semantic analysis until
/// the by-value struct ABI lands; `&self` and `&mut self` are supported (§2.5).
#[derive(Debug, Clone, PartialEq)]
pub enum SelfParam {
    /// `&self` — immutable borrow; lowered to pass-by-value in codegen.
    Ref,
    /// `&mut self` — mutable borrow; lowered to pass-by-pointer so field writes
    /// in the method body propagate to the caller's value (§2.5).
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
/// `trait_name` is `Some` for a trait implementation (`impl Drop for T`) and
/// `None` for a plain inherent block (`impl T`). Today the only recognized
/// trait is the compiler-known `Drop` lang-item (§2.1); other trait names parse
/// but are validated against the unknown-trait set in semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplDef {
    pub trait_name: Option<Identifier>,
    pub type_name: Identifier,
    /// `generics` is the impl-level `<T, U>` type-parameter list (§3.8), as in
    /// `impl<T> Wrapper<T>`; empty for a non-generic impl.
    pub generics: Vec<GenericParam>,
    /// Type arguments applied to `type_name`, as in the `<T>` of `impl<T> Wrapper<T>`.
    /// Empty for a plain `impl Name` block. Each argument typically names an impl
    /// generic parameter; monomorphization maps them positionally to the struct's
    /// concrete type arguments.
    pub type_args: Vec<Type>,
    /// Value predicates from an impl-level `where` clause (§3.8), checked at each
    /// instantiation (see [`FunctionDef::where_predicates`]).
    pub where_predicates: Vec<Expr>,
    pub methods: Vec<MethodDef>,
    pub span: Span,
}

/// A compile-time constant declaration at module scope.
///
/// The type annotation is mandatory; the value must be a constant expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDef {
    pub name: Identifier,
    pub ty: Type,
    pub value: super::expressions::Expr,
    pub span: Span,
}

/// The data a single enum variant carries (§3.5).
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

/// A single variant in an enum definition (§3.5).
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: Identifier,
    pub payload: VariantPayload,
    pub span: Span,
}

/// Enum definition (§3.5): a tagged union of named variants, each optionally
/// carrying associated data. Non-generic in Phase 1E — generic enums (`Option<T>`)
/// arrive with the generics system (1F).
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: Identifier,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

/// A `newtype` declaration (§3.15): a distinct nominal type wrapping `inner`.
///
/// Unlike a transparent `type` alias (which is expanded away at parse time), a
/// newtype survives to semantic analysis as its own type — the wrapper and the
/// inner type are not interchangeable. Construction is `Name(value)` and the inner
/// value is read via `.0`.
#[derive(Debug, Clone, PartialEq)]
pub struct NewtypeDef {
    pub name: Identifier,
    pub inner: Type,
    pub span: Span,
}

/// Top-level AST item
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Enum(EnumDef),
    Impl(ImplDef),
    Const(ConstDef),
    Newtype(NewtypeDef),
}
