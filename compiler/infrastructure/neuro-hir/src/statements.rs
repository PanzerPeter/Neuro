// Statement nodes

use shared_types::Span;

use crate::expressions::HirExpr;
use crate::types::HirType;

/// A typed HIR statement.
///
/// Mirrors [`ast_types::Stmt`] one-to-one. A variable declaration's type is
/// always resolved here (`ty`) — in the AST it is an optional annotation that
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
    ForRange {
        label: Option<String>,
        iterator: String,
        start: HirExpr,
        end: HirExpr,
        inclusive: bool,
        body: Vec<HirStmt>,
        span: Span,
    },
    ForEach {
        label: Option<String>,
        iterator: String,
        iterable: HirExpr,
        body: Vec<HirStmt>,
        span: Span,
    },
    Loop {
        label: Option<String>,
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
    /// Assignment through a mutable reference `*pointer = value` (§2.5).
    DerefAssignment {
        pointer: HirExpr,
        value: HirExpr,
        span: Span,
    },
    /// Array element assignment `target[index] = value` (§3.1).
    IndexAssignment {
        target: String,
        index: HirExpr,
        value: HirExpr,
        span: Span,
    },
    /// Function-body compile-time constant (§1.3).
    Const {
        name: String,
        ty: HirType,
        value: HirExpr,
        span: Span,
    },
    Expr(HirExpr),
}
