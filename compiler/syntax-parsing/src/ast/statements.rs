// NEURO Programming Language - Syntax Parsing
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
    Expr(Expr),
}
