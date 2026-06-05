// Neuro Programming Language - AST Types
// Top-level item AST nodes

use shared_types::{Identifier, Span};

use super::statements::Stmt;
use super::types::Type;

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Identifier,
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
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

/// The `self` parameter kind in a method signature.
///
/// `RefMut` and `Owned` are parsed but rejected by semantic analysis until
/// ownership semantics are introduced (Phase 1.5).
#[derive(Debug, Clone, PartialEq)]
pub enum SelfParam {
    /// `&self` — immutable borrow; lowered to pass-by-value in codegen
    Ref,
    /// `&mut self` — mutable borrow; not yet supported
    RefMut,
    /// `self` — consuming; not yet supported
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
#[derive(Debug, Clone, PartialEq)]
pub struct ImplDef {
    pub type_name: Identifier,
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

/// Top-level AST item
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
    Impl(ImplDef),
    Const(ConstDef),
}
