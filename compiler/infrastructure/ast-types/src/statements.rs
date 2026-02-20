// NEURO Programming Language - AST Types
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
    /// Executes `body` repeatedly while `condition` evaluates to `true`.
    While {
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// For loop over an exclusive numeric range.
    ///
    /// Executes `body` for each value of `iterator` from `start` up to (but not including)
    /// `end`.
    ForRange {
        iterator: Identifier,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// Break out of the nearest enclosing loop.
    Break {
        span: Span,
    },
    /// Continue to the next iteration of the nearest enclosing loop.
    Continue {
        span: Span,
    },
    Expr(Expr),
}
