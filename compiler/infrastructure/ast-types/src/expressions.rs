// NEURO Programming Language - AST Types
// Expression AST nodes

use std::fmt;

use shared_types::{Identifier, Literal, Span};

use super::statements::Stmt;

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
    /// Struct literal: `Point { x: 1.0, y: 2.0 }`.
    ///
    /// `base` carries the functional-update source: `Point { x: 1.0, ..p }`
    /// supplies every field not listed in `fields` from `base`. `None` means a
    /// plain literal where all fields must be listed explicitly.
    StructLiteral {
        name: Identifier,
        fields: Vec<FieldInit>,
        base: Option<Box<Expr>>,
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
    /// if-expression producing a value: `if cond { expr } else { expr }`
    If {
        condition: Box<Expr>,
        then_block: Vec<Stmt>,
        else_if_blocks: Vec<(Expr, Vec<Stmt>)>,
        else_block: Option<Vec<Stmt>>,
        span: Span,
    },
    /// Bare block expression: `{ stmts; trailing_expr }`
    Block {
        stmts: Vec<Stmt>,
        span: Span,
    },
    /// Unsafe block expression: `unsafe { stmts; trailing_expr }`.
    ///
    /// Phase 1.7 groundwork: the keyword is reserved and the block parses, but
    /// `unsafe` carries no special semantics yet — it is inert outside `@kernel`
    /// bodies (which do not exist until Phase 5). It type-checks and lowers
    /// exactly like a bare [`Expr::Block`]; the distinct node lets later phases
    /// attach the kernel-aliasing relaxation without reparsing.
    Unsafe {
        stmts: Vec<Stmt>,
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
            Expr::If { span, .. } => *span,
            Expr::Block { span, .. } => *span,
            Expr::Unsafe { span, .. } => *span,
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
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    NullCoalesce,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
    BitNot,
}

impl BinaryOp {
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            BinaryOp::Less
                | BinaryOp::Greater
                | BinaryOp::LessEqual
                | BinaryOp::GreaterEqual
                | BinaryOp::Equal
                | BinaryOp::NotEqual
        )
    }
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
            BinaryOp::BitAnd => write!(f, "&"),
            BinaryOp::BitOr => write!(f, "|"),
            BinaryOp::BitXor => write!(f, "^"),
            BinaryOp::Shl => write!(f, "<<"),
            BinaryOp::NullCoalesce => write!(f, "??"),
        }
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Negate => write!(f, "-"),
            UnaryOp::Not => write!(f, "!"),
            UnaryOp::BitNot => write!(f, "~"),
        }
    }
}
