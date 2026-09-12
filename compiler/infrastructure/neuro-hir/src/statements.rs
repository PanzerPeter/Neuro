// Statement nodes

use ast_types::BinaryOp;
use shared_types::Span;

use crate::expressions::{HirExpr, HirMatchBinding, HirMatchTest};
use crate::types::HirType;

/// A typed HIR statement.
///
/// Mirrors [`ast_types::Stmt`] one-to-one. A variable declaration's type is
/// always resolved here (`ty`): in the AST it is an optional annotation that
/// the type checker may have had to infer.
#[derive(Debug, Clone, PartialEq)]
pub enum HirStmt {
    VarDecl {
        name: String,
        ty: HirType,
        init: Option<HirExpr>,
        mutable: bool,
        span: Span,
    },
    Assignment {
        target: String,
        value: HirExpr,
        span: Span,
    },
    /// `target OP= value` on a tensor: an in-place element-wise update of the buffer
    /// `target`'s DLPack handle already addresses.
    ///
    /// Only the types that update in place reach this node; every other compound
    /// assignment is desugared to [`HirStmt::Assignment`] over a binary expression
    /// during lowering, so a backend that ignores this variant loses tensors and
    /// nothing else. `ty` is the target's tensor type, carrying the element type and
    /// the extents the update loops over. `value` is either that same tensor type or a
    /// reference to it: an owned operand is consumed by the update, a borrowed one is
    /// only read.
    TensorCompoundAssign {
        target: String,
        op: BinaryOp,
        value: HirExpr,
        ty: HirType,
        span: Span,
    },
    Return {
        value: Option<HirExpr>,
        span: Span,
    },
    If {
        condition: HirExpr,
        then_block: Vec<HirStmt>,
        else_if_blocks: Vec<(HirExpr, Vec<HirStmt>)>,
        else_block: Option<Vec<HirStmt>>,
        span: Span,
    },
    While {
        label: Option<String>,
        condition: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    /// `index` is the `u64` position binding of an enumerated loop; `None` for a
    /// plain `for v in a..b`.
    /// `reversed` walks the same bounds from the last value down to `start`; an
    /// enumerated loop's `index` still counts up from zero either way.
    ForRange {
        label: Option<String>,
        index: Option<String>,
        iterator: String,
        start: HirExpr,
        end: HirExpr,
        inclusive: bool,
        reversed: bool,
        body: Vec<HirStmt>,
        span: Span,
    },
    /// `index` is the `u64` position binding of an enumerated loop; `None` for a
    /// plain `for x in xs`.
    ForEach {
        label: Option<String>,
        index: Option<String>,
        iterator: String,
        iterable: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    Break {
        label: Option<String>,
        value: Option<HirExpr>,
        span: Span,
    },
    Continue {
        label: Option<String>,
        span: Span,
    },
    /// Struct field assignment `object.field = value`.
    FieldAssignment {
        object: String,
        field: String,
        value: HirExpr,
        span: Span,
    },
    /// Assignment through a mutable reference `*pointer = value`.
    DerefAssignment {
        pointer: HirExpr,
        value: HirExpr,
        span: Span,
    },
    /// Array element assignment `target[index] = value`.
    IndexAssignment {
        target: String,
        index: HirExpr,
        value: HirExpr,
        span: Span,
    },
    /// `val PATTERN = scrutinee else |binding| { ... }`, fully resolved.
    ///
    /// `test` decides the success path; `bindings` are then materialized into the
    /// ENCLOSING scope and stay live for every statement after this one: the
    /// difference from a [`HirExprKind::Match`](crate::HirExprKind::Match) arm, whose
    /// bindings die with the arm. `else_binding` is scoped to `else_block` alone. The
    /// frontend has verified that `else_block` diverges, so control leaves the scope
    /// on the failure path and never rejoins the success path.
    ValElse {
        scrutinee: HirExpr,
        test: HirMatchTest,
        bindings: Vec<HirMatchBinding>,
        else_binding: Option<HirMatchBinding>,
        else_block: Vec<HirStmt>,
        span: Span,
    },
    /// Function-body compile-time constant.
    Const {
        name: String,
        ty: HirType,
        value: HirExpr,
        span: Span,
    },
    Expr(HirExpr),
}
