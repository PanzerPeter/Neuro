// Statement AST nodes

use shared_types::{Identifier, Span};

use super::expressions::{Expr, Pattern};
use super::types::Type;

/// Which transformation a `for`-head adapter applies to the element stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopAdapterKind {
    /// `.map(f)` — replaces each element with `f(element)`.
    Map,
    /// `.filter(p)` — drops each element for which `p(element)` is false.
    Filter,
}

/// One `.map(f)` / `.filter(p)` call peeled off a `for` head.
///
/// The adapter is recognised by the parser rather than resolved as a method
/// because a range is not a first-class value, so `(0..n).map(f)` has no receiver
/// to dispatch against — the same reason `.enumerate()` is a head form.
/// `callee` is the single argument: a closure literal, a function name, or any
/// expression of function type.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopAdapter {
    pub kind: LoopAdapterKind,
    pub callee: Expr,
    pub span: Span,
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
    ///
    /// `index` carries the position binding of `for (i, v) in (a..b).enumerate()`:
    /// a `u64` counting from zero, independent of the range's own bounds and
    /// element type. `None` is a plain `for v in a..b`.
    ///
    /// `adapters` are the `.map(f)` / `.filter(p)` calls the head wore, in source
    /// order; empty for a bare range.
    ForRange {
        label: Option<Identifier>,
        index: Option<Identifier>,
        iterator: Identifier,
        start: Expr,
        end: Expr,
        inclusive: bool,
        adapters: Vec<LoopAdapter>,
        body: Vec<Stmt>,
        span: Span,
    },
    /// For loop over an array value (`for x in arr`).
    ///
    /// `iterable` evaluates to an array (or a borrow of one); `iterator` binds each
    /// element in turn. Lowered directly in codegen as a counted loop over the
    /// array storage — it does not dispatch through an iterator protocol. An
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
    /// `val PATTERN = value else |binding| { ... }` — bind a refutable pattern or
    /// leave the enclosing scope.
    ///
    /// The pattern's bindings are introduced into the *enclosing* block, not just a
    /// nested arm, which is what distinguishes this from a `match`. `else_binding` is
    /// the optional `|name|` after `else`; what it names depends on the scrutinee's
    /// type: a `Result`'s `Err` payload, nothing for an `Option` (only `_` is
    /// accepted), and the whole scrutinee for any other enum. `else_block` must
    /// diverge — semantic analysis rejects one that can fall through.
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
