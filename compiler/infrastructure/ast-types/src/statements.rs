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
    /// optional `label` (`outer: while ...`) names the loop so a nested
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
    /// `label` names the loop for labeled break/continue.
    ForRange {
        label: Option<Identifier>,
        iterator: Identifier,
        start: Expr,
        end: Expr,
        inclusive: bool,
        body: Vec<Stmt>,
        span: Span,
    },
    /// For loop over an array value (`for x in arr`).
    ///
    /// `iterable` evaluates to an array (or a borrow of one); `iterator` binds each
    /// element in turn. Lowered directly in codegen as a counted loop over the
    /// array storage — it does not dispatch through an iterator protocol. An
    /// optional `label` names the loop for labeled break/continue.
    ForEach {
        label: Option<Identifier>,
        iterator: Identifier,
        iterable: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Infinite loop statement.
    ///
    /// Executes `body` repeatedly; the only exit is a `break`. `continue`
    /// re-enters the body from the top. Unlike `while`/`for`, a `loop` has no
    /// fall-through exit. In statement position the loop's value is discarded;
    /// the value-producing form lives in [`Expr::Loop`] (`break value`).
    /// An optional `label` names the loop for labeled break/continue.
    Loop {
        label: Option<Identifier>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Break out of the nearest enclosing loop, or out of the loop named by
    /// `label` when present (`break outer`).
    ///
    /// `value` carries the loop-expression result for a value-producing `break v`
    /// The targeted `loop` evaluates to it. Only `loop` accepts a value;
    /// `while`/`for` always yield unit, so a value here targeting them is rejected
    /// in semantic analysis. `None` is a plain `break` / `break label`.
    Break {
        label: Option<Identifier>,
        value: Option<Expr>,
        span: Span,
    },
    /// Continue the nearest enclosing loop, or the loop named by `label` when
    /// present (`continue outer`).
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
    /// Assignment through a mutable reference: `*pointer = value`.
    ///
    /// `pointer` is the reference expression being dereferenced (the `r` in
    /// `*r = value`); the value is stored at the location it points at. Requires
    /// `pointer` to have a `&mut T` type — enforced in semantic analysis.
    DerefAssignment {
        pointer: Expr,
        value: Expr,
        span: Span,
    },
    /// Array element assignment `target[index] = value`. `target` is a
    /// mutable array binding; `index` is an integer. Out-of-bounds access panics
    /// in debug builds.
    IndexAssignment {
        target: Identifier,
        index: Expr,
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
