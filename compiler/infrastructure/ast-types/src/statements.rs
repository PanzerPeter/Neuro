// Neuro Programming Language - AST Types
// Statement AST nodes

use shared_types::{Identifier, Span};

use super::expressions::Expr;
use super::types::Type;

/// Statement AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: Identifier,
        ty: Option<Type>,
        init: Option<Expr>,
        mutable: bool,
        span: Span,
    },
    Assignment {
        target: Identifier,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_if_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    /// While loop statement.
    ///
    /// Executes `body` repeatedly while `condition` evaluates to `true`. An
    /// optional `label` (`outer: while ...`, §3.7) names the loop so a nested
    /// `break label` / `continue label` can target it.
    While {
        label: Option<Identifier>,
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// For loop over a numeric range.
    ///
    /// Executes `body` for each value of `iterator` from `start` up to
    /// `end`. Whether `end` is included depends on `inclusive`. An optional
    /// `label` (§3.7) names the loop for labeled break/continue.
    ForRange {
        label: Option<Identifier>,
        iterator: Identifier,
        start: Expr,
        end: Expr,
        inclusive: bool,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Infinite loop statement (§3.7).
    ///
    /// Executes `body` repeatedly; the only exit is a `break`. `continue`
    /// re-enters the body from the top. Unlike `while`/`for`, a `loop` has no
    /// fall-through exit. The value-producing form (`break value`) is not yet
    /// modelled — a `loop` statement always yields unit. An optional `label`
    /// (§3.7) names the loop for labeled break/continue.
    Loop {
        label: Option<Identifier>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Break out of the nearest enclosing loop, or out of the loop named by
    /// `label` when present (`break outer`, §3.7).
    Break {
        label: Option<Identifier>,
        span: Span,
    },
    /// Continue the nearest enclosing loop, or the loop named by `label` when
    /// present (`continue outer`, §3.7).
    Continue {
        label: Option<Identifier>,
        span: Span,
    },
    /// Field assignment on a struct binding: `object.field = value`
    FieldAssignment {
        object: Identifier,
        field: Identifier,
        value: Expr,
        span: Span,
    },
    /// Assignment through a mutable reference: `*pointer = value` (§2.5).
    ///
    /// `pointer` is the reference expression being dereferenced (the `r` in
    /// `*r = value`); the value is stored at the location it points at. Requires
    /// `pointer` to have a `&mut T` type — enforced in semantic analysis.
    DerefAssignment {
        pointer: Expr,
        value: Expr,
        span: Span,
    },
    /// Compile-time constant declaration inside a function body.
    ///
    /// The type annotation is mandatory; the value must be a constant expression.
    Const {
        name: Identifier,
        ty: Type,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}
