// Expression nodes

use shared_types::{Literal, Span};

use ast_types::{BinaryOp, UnaryOp};

use crate::statements::HirStmt;
use crate::types::HirType;

/// A typed HIR expression.
///
/// Every expression carries its resolved `ty` — the defining difference from
/// the surface [`ast_types::Expr`], whose types are still unresolved name
/// annotations. Backends read `ty` directly instead of re-deriving it.
#[derive(Debug, Clone, PartialEq)]
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
    pub span: Span,
}

impl HirExpr {
    /// Construct an expression node from its kind, resolved type, and span.
    pub fn new(kind: HirExprKind, ty: HirType, span: Span) -> Self {
        Self { kind, ty, span }
    }
}

/// A single field initializer in a struct literal: `field_name: value`.
#[derive(Debug, Clone, PartialEq)]
pub struct HirFieldInit {
    pub name: String,
    pub value: Box<HirExpr>,
    pub span: Span,
}

/// The shape of a HIR expression.
///
/// The variant set mirrors [`ast_types::Expr`] one-to-one, with two
/// normalizations the HIR performs over the AST:
/// - `Paren` is dropped — grouping is already encoded by the tree structure, so
///   a typed IR has no need for an explicit parenthesis node.
/// - Identifiers are resolved to their `String` name; the binding's source span
///   lives on the enclosing [`HirExpr`].
#[derive(Debug, Clone, PartialEq)]
pub enum HirExprKind {
    Literal(Literal),
    /// A resolved reference to a binding, parameter, or constant by name.
    Variable(String),
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<HirExpr>,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirExpr>,
    },
    /// Struct literal `Name { field: value, ..base }` (§3.3). `base` carries the
    /// functional-update source when present.
    StructLiteral {
        name: String,
        fields: Vec<HirFieldInit>,
        base: Option<Box<HirExpr>>,
    },
    FieldAccess {
        object: Box<HirExpr>,
        field: String,
    },
    /// `TypeName::member` path, the callee of an associated-function call.
    Path {
        type_name: String,
        member: String,
    },
    /// `value as T` cast (§1.4). The target type is the expression's `ty`.
    Cast {
        value: Box<HirExpr>,
    },
    If {
        condition: Box<HirExpr>,
        then_block: Vec<HirStmt>,
        else_if_blocks: Vec<(HirExpr, Vec<HirStmt>)>,
        else_block: Option<Vec<HirStmt>>,
    },
    Block {
        stmts: Vec<HirStmt>,
    },
    /// Value-producing infinite loop `loop { ... break v }` (§3.7).
    Loop {
        label: Option<String>,
        body: Vec<HirStmt>,
    },
    /// `unsafe { ... }` block (§3). Inert outside `@kernel` bodies; the distinct
    /// node preserves the boundary for later phases.
    Unsafe {
        stmts: Vec<HirStmt>,
    },
    /// Borrow `&place` / `&mut place` (§2.4, §2.5).
    Reference {
        operand: Box<HirExpr>,
        mutable: bool,
    },
    /// Dereference `*operand` (§2.5).
    Deref {
        operand: Box<HirExpr>,
    },
    /// Range `start..end` / `start..=end`. Only valid as a `string.slice`
    /// argument (§2.7); never produced for `for`-range loops.
    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
        inclusive: bool,
    },
    ArrayLiteral {
        elements: Vec<HirExpr>,
    },
    Index {
        object: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    /// Tuple literal `(e0, e1, ...)` (§3.2). The element types live on the elements;
    /// this expression's `ty` is the [`HirType::Tuple`] of them.
    TupleLiteral {
        elements: Vec<HirExpr>,
    },
    /// Tuple element access `object.N` (§3.2) by constant index.
    TupleIndex {
        object: Box<HirExpr>,
        index: usize,
    },
    /// Enum construction (§3.5): builds the `variant`-th value of `enum_name` from
    /// the lowered `payload`, which is in the variant's declared field order (the
    /// three surface forms — unit `E::V`, tuple `E::V(..)`, and struct `E::V { .. }`
    /// — all normalize to this single node). `tag` is the variant's discriminant
    /// (its declaration index). The expression's `ty` is [`HirType::Enum`].
    EnumConstruct {
        enum_name: String,
        variant: String,
        tag: u32,
        payload: Vec<HirExpr>,
    },
    /// Trailing remainder of an array destructuring pattern (§3.2): a fresh
    /// `[T; N - start]` array copying elements `start..N` of `array` (a `[T; N]`).
    /// The arity rules are validated before lowering; this node's `ty` carries
    /// the resulting array type.
    ArrayRest {
        array: Box<HirExpr>,
        start: usize,
    },
    /// Pattern-matching expression `match scrutinee { ... }` (§3.6), fully resolved.
    ///
    /// Each arm carries its refutable tests (already keyed to variant tags / literal
    /// values / ranges), the payload/scrutinee bindings it introduces, an optional
    /// guard, and a body. The frontend has verified exhaustiveness, so a value with no
    /// matching arm cannot occur at runtime.
    Match {
        scrutinee: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
}

/// One resolved arm of a [`HirExprKind::Match`] (§3.6).
#[derive(Debug, Clone, PartialEq)]
pub struct HirMatchArm {
    /// OR-alternatives: the arm's pattern part fires when any test matches. A
    /// catch-all (`_` / bare binding) is a single [`HirMatchTest::Wildcard`].
    pub tests: Vec<HirMatchTest>,
    /// Bindings the arm introduces, in scope for its guard and body.
    pub bindings: Vec<HirMatchBinding>,
    pub guard: Option<HirExpr>,
    pub body: HirExpr,
}

/// A single refutable test of the scrutinee (§3.6). Payload sub-patterns never
/// contribute a test (they are irrefutable binding/`_` forms), so only the tag /
/// scalar value is examined here.
#[derive(Debug, Clone, PartialEq)]
pub enum HirMatchTest {
    /// Matches unconditionally (`_` or a bare binding).
    Wildcard,
    /// Enum-variant tag equals `tag`.
    Tag { tag: u32 },
    /// Scalar (integer / `char` / `bool`) equals `value` (encoded as the low bits of
    /// an `i64`).
    IntEq { value: i64 },
    /// Ordered scalar lies in `lo..=hi` (an exclusive `..` is normalized to `hi - 1`).
    IntRange { lo: i64, hi: i64 },
}

/// A binding introduced by a match arm (§3.6), in scope for its guard and body.
#[derive(Debug, Clone, PartialEq)]
pub struct HirMatchBinding {
    pub name: String,
    pub ty: HirType,
    pub source: HirBindingSource,
}

/// Where a [`HirMatchBinding`]'s value comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum HirBindingSource {
    /// The whole scrutinee value (a bare binding `n => ...`).
    Scrutinee,
    /// Enum payload slot `slot`, decoded back to the binding's type (§3.5 layout).
    EnumPayload { slot: usize },
}
