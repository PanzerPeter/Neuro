// Statement AST nodes

use shared_types::{Identifier, Span};

use super::expressions::{BinaryOp, Expr, Pattern, TensorIndexArg};
use super::types::Type;

/// Which transformation a `for`-head adapter applies to the element stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAdapterKind {
    /// `.map(f)`: replaces each element with `f(element)`.
    Map,
    /// `.filter(p)`: drops each element for which `p(element)` is false.
    Filter,
}

/// One `.map(f)` / `.filter(p)` call peeled off a `for` head.
///
/// The adapter is recognised by the parser rather than resolved as a method
/// because a range is not a first-class value, so `(0..n).map(f)` has no receiver
/// to dispatch against: the same reason `.enumerate()` is a head form.
/// `callee` is the single argument: a closure literal, a function name, or any
/// expression of function type.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopAdapter {
    pub kind: LoopAdapterKind,
    pub callee: Expr,
    pub span: Span,
}

/// The storage an assignment writes into.
///
/// A place is rooted at a binding and reached through any number of field
/// accesses, one index, or a dereference. The base of each form is kept as an
/// [`Expr`] rather than a nested `Place` because the base is *read* on the way to
/// the location — `self.inner.data[i] = v` loads `self`, projects `inner`, and only
/// the last step is a write — so every stage already has the machinery for it.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// A binding: `x`.
    Var(Identifier),
    /// A struct field: `object.field`.
    Field {
        object: Box<Expr>,
        field: Identifier,
        span: Span,
    },
    /// One element of an array, slice, `Vec`, or rank-1 tensor: `object[index]`.
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// One element of a tensor named by every axis: `object[i, j]`.
    TensorIndex {
        object: Box<Expr>,
        indices: Vec<TensorIndexArg>,
        span: Span,
    },
    /// The referent of a mutable reference: `*pointer`.
    Deref { pointer: Box<Expr>, span: Span },
}

impl Place {
    pub fn span(&self) -> Span {
        match self {
            Place::Var(ident) => ident.span,
            Place::Field { span, .. }
            | Place::Index { span, .. }
            | Place::TensorIndex { span, .. }
            | Place::Deref { span, .. } => *span,
        }
    }

    /// The binding the place is rooted at, or `None` when the root is a temporary
    /// or a dereference of one. Mutability, borrow tracking, move state and pool
    /// residency are all keyed by binding, so every stage asks for this.
    pub fn root(&self) -> Option<&Identifier> {
        match self {
            Place::Var(ident) => Some(ident),
            Place::Field { object, .. }
            | Place::Index { object, .. }
            | Place::TensorIndex { object, .. }
            | Place::Deref {
                pointer: object, ..
            } => expr_root(object),
        }
    }

    /// The expression that reads the place. A compound assignment desugars through
    /// it, and any stage that needs the place's type types this instead.
    pub fn to_expr(&self) -> Expr {
        match self {
            Place::Var(ident) => Expr::Identifier(ident.clone()),
            Place::Field {
                object,
                field,
                span,
            } => Expr::FieldAccess {
                object: object.clone(),
                field: field.clone(),
                span: *span,
            },
            Place::Index {
                object,
                index,
                span,
            } => Expr::Index {
                object: object.clone(),
                index: index.clone(),
                span: *span,
            },
            Place::TensorIndex {
                object,
                indices,
                span,
            } => Expr::TensorIndex {
                object: object.clone(),
                indices: indices.clone(),
                span: *span,
            },
            Place::Deref { pointer, span } => Expr::Deref {
                operand: pointer.clone(),
                span: *span,
            },
        }
    }
}

/// The binding an expression bottoms out at, peeling the projections that keep a
/// place rooted where it started.
fn expr_root(expr: &Expr) -> Option<&Identifier> {
    match expr {
        Expr::Identifier(ident) => Some(ident),
        Expr::Paren(inner, _) => expr_root(inner),
        Expr::FieldAccess { object, .. }
        | Expr::Index { object, .. }
        | Expr::TensorIndex { object, .. }
        | Expr::TupleIndex { object, .. }
        | Expr::Deref {
            operand: object, ..
        } => expr_root(object),
        _ => None,
    }
}

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
    /// `place = value`, or `place OP= value` when `op` is `Some`.
    ///
    /// The compound form is kept as written rather than desugared in the parser:
    /// the choice between `place = place OP value` and an in-place update is
    /// type-directed (a type implementing the matching `*Assign` trait takes the
    /// in-place path), and the parser has no types.
    Assign {
        place: Place,
        op: Option<BinaryOp>,
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
    ///
    /// `index` carries the position binding of `for (i, v) in (a..b).enumerate()`:
    /// a `u64` counting from zero, independent of the range's own bounds and
    /// element type. `None` is a plain `for v in a..b`.
    ///
    /// `adapters` are the `.map(f)` / `.filter(p)` calls the head wore, in source
    /// order; empty for a bare range.
    ///
    /// `reversed` is the `.rev()` head form: the same bounds, walked from the last
    /// value down to `start`. It decorates the range itself rather than the element
    /// stream, so it sits beneath every adapter in `adapters` and beneath `index`.
    ForRange {
        label: Option<Identifier>,
        index: Option<Identifier>,
        iterator: Identifier,
        start: Expr,
        end: Expr,
        inclusive: bool,
        reversed: bool,
        adapters: Vec<LoopAdapter>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// For loop over an array value (`for x in arr`).
    ///
    /// `iterable` evaluates to an array (or a borrow of one); `iterator` binds each
    /// element in turn. Lowered directly in codegen as a counted loop over the
    /// array storage: it does not dispatch through an iterator protocol. An
    /// optional `label` names the loop for labeled break/continue.
    ///
    /// `index` carries the position binding of `for (i, x) in xs.enumerate()`:
    /// a `u64` counting from zero, which for a counted loop over contiguous
    /// storage is the same value the lowering already needs. `None` is a plain
    /// `for x in xs`.
    ///
    /// `adapters` are the `.map(f)` / `.filter(p)` calls the head wore, in source
    /// order; empty for a bare `for x in xs`.
    ForEach {
        label: Option<Identifier>,
        index: Option<Identifier>,
        iterator: Identifier,
        iterable: Expr,
        adapters: Vec<LoopAdapter>,
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
    /// `val PATTERN = value else |binding| { ... }`: bind a refutable pattern or
    /// leave the enclosing scope.
    ///
    /// The pattern's bindings are introduced into the *enclosing* block, not just a
    /// nested arm, which is what distinguishes this from a `match`. `else_binding` is
    /// the optional `|name|` after `else`; what it names depends on the scrutinee's
    /// type: a `Result`'s `Err` payload, nothing for an `Option` (only `_` is
    /// accepted), and the whole scrutinee for any other enum. `else_block` must
    /// diverge; semantic analysis rejects one that can fall through.
    ValElse {
        pattern: Pattern,
        value: Expr,
        else_binding: Option<Identifier>,
        else_block: Vec<Stmt>,
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
