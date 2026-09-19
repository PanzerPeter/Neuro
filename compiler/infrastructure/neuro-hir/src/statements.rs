// Statement nodes

use ast_types::BinaryOp;
use shared_types::Span;

use crate::expressions::{HirExpr, HirExprKind, HirMatchBinding, HirMatchTest, HirTensorAxis};
use crate::types::HirType;

/// The storage a lowered assignment writes into.
///
/// Mirrors [`ast_types::Place`]. Each form carries the type of the location itself,
/// which is what a backend stores through; the base stays an [`HirExpr`] because
/// reaching the location is an ordinary read of everything up to the last step.
#[derive(Debug, Clone, PartialEq)]
pub enum HirPlace {
    /// A binding: `x`.
    Var { name: String, ty: HirType },
    /// A struct field: `object.field`.
    Field {
        object: Box<HirExpr>,
        field: String,
        ty: HirType,
    },
    /// One element of an array, slice, `Vec`, or rank-1 tensor: `object[index]`.
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
        ty: HirType,
    },
    /// One element of a tensor named by every axis: `object[i, j]`.
    TensorIndex {
        object: Box<HirExpr>,
        axes: Vec<HirTensorAxis>,
        ty: HirType,
    },
    /// The referent of a mutable reference: `*pointer`.
    Deref { pointer: Box<HirExpr>, ty: HirType },
}

impl HirPlace {
    /// The expression that reads the place. A backend that already lowers a read of
    /// this shape reaches the same storage through it.
    pub fn to_expr(&self, span: Span) -> HirExpr {
        match self {
            HirPlace::Var { name, ty } => {
                HirExpr::new(HirExprKind::Variable(name.clone()), ty.clone(), span)
            }
            HirPlace::Field { object, field, ty } => HirExpr::new(
                HirExprKind::FieldAccess {
                    object: object.clone(),
                    field: field.clone(),
                },
                ty.clone(),
                span,
            ),
            HirPlace::Index { object, index, ty } => HirExpr::new(
                HirExprKind::Index {
                    object: object.clone(),
                    index: index.clone(),
                },
                ty.clone(),
                span,
            ),
            HirPlace::TensorIndex { object, axes, ty } => HirExpr::new(
                HirExprKind::TensorIndex {
                    object: object.clone(),
                    axes: axes.clone(),
                },
                ty.clone(),
                span,
            ),
            HirPlace::Deref { pointer, ty } => HirExpr::new(
                HirExprKind::Deref {
                    operand: pointer.clone(),
                },
                ty.clone(),
                span,
            ),
        }
    }

    /// The type of the location, which is the type a stored value must have.
    pub fn ty(&self) -> &HirType {
        match self {
            HirPlace::Var { ty, .. }
            | HirPlace::Field { ty, .. }
            | HirPlace::Index { ty, .. }
            | HirPlace::TensorIndex { ty, .. }
            | HirPlace::Deref { ty, .. } => ty,
        }
    }
}

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
    Assign {
        place: HirPlace,
        value: HirExpr,
        span: Span,
    },
    /// `place OP= value` on a tensor: an in-place element-wise update of the buffer
    /// the place's DLPack handle already addresses.
    ///
    /// Only the types that update in place reach this node; every other compound
    /// assignment is desugared to [`HirStmt::Assign`] over a binary expression
    /// during lowering, so a backend that ignores this variant loses tensors and
    /// nothing else. `ty` is the target's tensor type, carrying the element type and
    /// the extents the update loops over. `value` is either that same tensor type or a
    /// reference to it: an owned operand is consumed by the update, a borrowed one is
    /// only read.
    TensorCompoundAssign {
        place: HirPlace,
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
