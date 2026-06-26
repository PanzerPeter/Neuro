// Neuro Programming Language - Typed HIR
// Expression nodes

use shared_types::{Literal, Span};

use ast_types::{BinaryOp, UnaryOp};

use crate::statements::HirStmt;
use crate::types::HirType;

/// A typed HIR expression.
///
/// Every expression carries its resolved `ty` — the defining difference from
/// the surface [`ast_types::Expr`], whose types are still unresolved name
/// annotations. Backends read `ty` directly instead of re-deriving it.
#[derive(Debug, Clone, PartialEq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
    pub span: Span,
}

impl HirExpr {
    /// Construct an expression node from its kind, resolved type, and span.
    pub fn new(kind: HirExprKind, ty: HirType, span: Span) -> Self {
        Self { kind, ty, span }
    }
}

/// A single field initializer in a struct literal: `field_name: value`.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFieldInit {
    pub name: String,
    pub value: Box<HirExpr>,
    pub span: Span,
}

/// The shape of a HIR expression.
///
/// The variant set mirrors [`ast_types::Expr`] one-to-one, with two
/// normalizations the HIR performs over the AST:
/// - `Paren` is dropped — grouping is already encoded by the tree structure, so
///   a typed IR has no need for an explicit parenthesis node.
/// - Identifiers are resolved to their `String` name; the binding's source span
///   lives on the enclosing [`HirExpr`].
#[derive(Debug, Clone, PartialEq)]
pub enum HirExprKind {
    Literal(Literal),
    /// A resolved reference to a binding, parameter, or constant by name.
    Variable(String),
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<HirExpr>,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    /// Struct literal `Name { field: value, ..base }` (§3.3). `base` carries the
    /// functional-update source when present.
    StructLiteral {
        name: String,
        fields: Vec<HirFieldInit>,
        base: Option<Box<HirExpr>>,
    },
    FieldAccess {
        object: Box<HirExpr>,
        field: String,
    },
    /// `TypeName::member` path, the callee of an associated-function call.
    Path {
        type_name: String,
        member: String,
    },
    /// `value as T` cast (§1.4). The target type is the expression's `ty`.
    Cast {
        value: Box<HirExpr>,
    },
    If {
        condition: Box<HirExpr>,
        then_block: Vec<HirStmt>,
        else_if_blocks: Vec<(HirExpr, Vec<HirStmt>)>,
        else_block: Option<Vec<HirStmt>>,
    },
    Block {
        stmts: Vec<HirStmt>,
    },
    /// Value-producing infinite loop `loop { ... break v }` (§3.7).
    Loop {
        label: Option<String>,
        body: Vec<HirStmt>,
    },
    /// `unsafe { ... }` block (§3). Inert outside `@kernel` bodies; the distinct
    /// node preserves the boundary for later phases.
    Unsafe {
        stmts: Vec<HirStmt>,
    },
    /// Borrow `&place` / `&mut place` (§2.4, §2.5).
    Reference {
        operand: Box<HirExpr>,
        mutable: bool,
    },
    /// Dereference `*operand` (§2.5).
    Deref {
        operand: Box<HirExpr>,
    },
    /// Range `start..end` / `start..=end`. Only valid as a `string.slice`
    /// argument (§2.7); never produced for `for`-range loops.
    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
        inclusive: bool,
    },
    ArrayLiteral {
        elements: Vec<HirExpr>,
    },
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
    },
}
