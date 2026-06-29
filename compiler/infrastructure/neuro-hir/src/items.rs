// Top-level item nodes

use shared_types::Span;

use crate::expressions::HirExpr;
use crate::statements::HirStmt;
use crate::types::HirType;

/// A complete type-checked program: the ordered list of top-level items.
///
/// This is the HIR root — the stable hand-off from the frontend (parser + type
/// checker) to every backend.
#[derive(Debug, Clone, PartialEq)]
pub struct HirProgram {
    pub items: Vec<HirItem>,
}

/// A top-level HIR item.
#[derive(Debug, Clone, PartialEq)]
pub enum HirItem {
    Function(HirFunction),
    Struct(HirStruct),
    Impl(HirImpl),
    Const(HirConst),
}

/// A free function. The return type is always resolved — `HirType::Void` when
/// the source declared none.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<HirParam>,
    pub return_type: HirType,
    pub body: Vec<HirStmt>,
    pub span: Span,
}

/// A function or method parameter with its resolved type.
#[derive(Debug, Clone, PartialEq)]
pub struct HirParam {
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

/// A struct definition.
#[derive(Debug, Clone, PartialEq)]
pub struct HirStruct {
    pub name: String,
    pub fields: Vec<HirField>,
    pub span: Span,
}

/// A single field of a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct HirField {
    pub name: String,
    pub ty: HirType,
    pub span: Span,
}

/// An `impl` block. `trait_name` is `Some` for a trait implementation
/// (`impl Drop for T`, §2.1) and `None` for an inherent block.
#[derive(Debug, Clone, PartialEq)]
pub struct HirImpl {
    pub type_name: String,
    pub trait_name: Option<String>,
    pub methods: Vec<HirMethod>,
    pub span: Span,
}

/// The `self` receiver of a method (§2.5). `None` on a [`HirMethod`] marks an
/// associated function.
#[derive(Debug, Clone, PartialEq)]
pub enum HirSelfParam {
    /// `&self` — immutable borrow.
    Ref,
    /// `&mut self` — mutable borrow.
    RefMut,
    /// `self` — consuming receiver.
    Owned,
}

/// A method inside an `impl` block. `self_param: None` is an associated
/// function (`TypeName::f(args)`); `Some(_)` is an instance method.
#[derive(Debug, Clone, PartialEq)]
pub struct HirMethod {
    pub name: String,
    pub self_param: Option<HirSelfParam>,
    pub params: Vec<HirParam>,
    pub return_type: HirType,
    pub body: Vec<HirStmt>,
    pub span: Span,
}

/// A module-level compile-time constant (§1.3).
#[derive(Debug, Clone, PartialEq)]
pub struct HirConst {
    pub name: String,
    pub ty: HirType,
    pub value: HirExpr,
    pub span: Span,
}
