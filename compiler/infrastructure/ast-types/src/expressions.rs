// NEURO Programming Language - AST Types
// Expression AST nodes

use std::fmt;

use shared_types::{Identifier, Literal, Span};

/// A single field initializer in a struct literal: `field_name: expr`
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: Identifier,
    pub value: Box<Expr>,
    pub span: Span,
}

/// Abstract Syntax Tree node for expressions
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal, Span),
    Identifier(Identifier),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Paren(Box<Expr>, Span),
    /// Struct literal: `Point { x: 1.0, y: 2.0 }`
    StructLiteral {
        name: Identifier,
        fields: Vec<FieldInit>,
        span: Span,
    },
    /// Field access: `expr.field_name`
    FieldAccess {
        object: Box<Expr>,
        field: Identifier,
        span: Span,
    },
    /// Path expression: `TypeName::member` — used for associated function references.
    /// Appears as the `func` of `Expr::Call` when calling `Point::new(args)`.
    Path {
        type_name: Identifier,
        member: Identifier,
        span: Span,
    },
    /// Type cast (`expr as type`)
    Cast {
        expr: Box<Expr>,
        target_type: crate::Type,
        span: Span,
    },
}

impl Expr {
    /// Get the span of this expression
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(_, span) => *span,
            Expr::Identifier(ident) => ident.span,
            Expr::Binary { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::Unary { span, .. } => *span,
            Expr::Paren(_, span) => *span,
            Expr::StructLiteral { span, .. } => *span,
            Expr::FieldAccess { span, .. } => *span,
            Expr::Path { span, .. } => *span,
            Expr::Cast { span, .. } => *span,
        }
    }
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Subtract => write!(f, "-"),
            BinaryOp::Multiply => write!(f, "*"),
            BinaryOp::Divide => write!(f, "/"),
            BinaryOp::Modulo => write!(f, "%"),
            BinaryOp::Equal => write!(f, "=="),
            BinaryOp::NotEqual => write!(f, "!="),
            BinaryOp::Less => write!(f, "<"),
            BinaryOp::Greater => write!(f, ">"),
            BinaryOp::LessEqual => write!(f, "<="),
            BinaryOp::GreaterEqual => write!(f, ">="),
            BinaryOp::And => write!(f, "&&"),
            BinaryOp::Or => write!(f, "||"),
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Negate => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
        }
    }
}
