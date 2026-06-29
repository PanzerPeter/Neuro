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
    /// Infinite loop in value position: `loop { ... break v }` (§3.7).
    ///
    /// Distinct from [`Stmt::Loop`]: only `loop` can yield a non-unit value
    /// (it has no fall-through exit, so it leaves solely via `break`). The loop
    /// evaluates to the value carried by its value-producing `break`s, which must
    /// all agree on type. `while`/`for` always yield unit and have no expression
    /// form. The expression form is unlabeled; labels are a statement-loop concern.
    Loop {
        label: Option<Identifier>,
        body: Vec<Stmt>,
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
    /// Borrow expression `&place` / `&mut place` (§2.4, §2.5): takes a non-owning
    /// reference to `operand` without moving it. The operand is a place expression
    /// (an identifier); the result has type `&T` (or `&mut T` when `mutable`).
    Reference {
        operand: Box<Expr>,
        mutable: bool,
        span: Span,
    },
    /// Dereference expression `*operand` (§2.5): reads the value the reference
    /// `operand` points at. The operand must have a reference type `&T`/`&mut T`;
    /// the result has type `T`.
    Deref {
        operand: Box<Expr>,
        span: Span,
    },
    /// Range expression `start..end` (exclusive) or `start..=end` (inclusive).
    ///
    /// Ranges are not a first-class value type: this node is only valid as the
    /// argument to `string.slice` (§2.7). `for`-range loops carry their bounds
    /// directly on [`Stmt::ForRange`] and never produce this node.
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        span: Span,
    },
    /// Array literal `[e0, e1, ...]` (§3.1). All elements must share one type; the
    /// array's length is the element count.
    ArrayLiteral {
        elements: Vec<Expr>,
        span: Span,
    },
    /// Array indexing `object[index]` (§3.1). `object` evaluates to an array (or a
    /// borrow of one); `index` is an integer. Out-of-bounds access panics in debug
    /// builds (§1.2).
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// Tuple literal `(e0, e1, ...)` (§3.2). Always has at least two elements; a
    /// single parenthesized expression is [`Expr::Paren`] grouping instead.
    TupleLiteral {
        elements: Vec<Expr>,
        span: Span,
    },
    /// Tuple element access `object.N` (§3.2), where `N` is a constant
    /// non-negative index. Distinct from [`Expr::FieldAccess`], which names a
    /// struct field by identifier.
    TupleIndex {
        object: Box<Expr>,
        index: usize,
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
            Expr::Loop { span, .. } => *span,
            Expr::Unsafe { span, .. } => *span,
            Expr::Reference { span, .. } => *span,
            Expr::Deref { span, .. } => *span,
            Expr::Range { span, .. } => *span,
            Expr::ArrayLiteral { span, .. } => *span,
            Expr::Index { span, .. } => *span,
            Expr::TupleLiteral { span, .. } => *span,
            Expr::TupleIndex { span, .. } => *span,
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
