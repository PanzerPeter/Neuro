// Type AST nodes

use shared_types::{Identifier, Span};

/// One generic argument in a type application `Name<...>` or a call-site turbofish
/// `f::<...>(x)`. An argument is either a type (for a type parameter) or an
/// integer value (for a const parameter). Positional: matched to the callee's or
/// constructor's generic parameters in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericArg {
    /// A type argument, e.g. the `i32` in `Pair<i32, f64>` or `parse::<i32>("42")`.
    Type(Type),
    /// A const (value) argument, e.g. the `4` in `Ring<i32, 4>` or `zeros::<4>()`.
    Const { value: i128, span: Span },
}

impl GenericArg {
    /// The source span of this generic argument.
    pub fn span(&self) -> Span {
        match self {
            GenericArg::Type(ty) => ty.span(),
            GenericArg::Const { span, .. } => *span,
        }
    }
}

/// The length of a fixed-size array type `[T; N]`.
///
/// Ordinarily a compile-time integer literal. Inside a generic definition it may
/// instead name a `const` generic parameter (`[T; CAP]`); the symbolic form is
/// resolved to a concrete length by monomorphization before any backend sees it,
/// so it never escapes the frontend.
#[derive(Debug, Clone, PartialEq)]
pub enum ArraySize {
    /// A concrete compile-time length, e.g. the `3` in `[i32; 3]`.
    Literal(u64),
    /// A `const` generic parameter used as the length, e.g. the `CAP` in `[T; CAP]`.
    Const(Identifier),
}

/// Type AST nodes
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// Named type (e.g., i32, f64, String, bool)
    Named(Identifier),

    /// Borrow (reference) type: a non-owning reference to a value of
    /// type `inner`. `span` covers the leading `&` through the referent type.
    /// `mutable` distinguishes `&mut T` (write access) from `&T` (read-only).
    /// `lifetime` is the optional explicit annotation, the `'a` in `&'a T`;
    /// `None` when elided. It is validated for well-formedness then erased — a
    /// reference type's identity does not depend on it.
    Reference {
        inner: Box<Type>,
        mutable: bool,
        lifetime: Option<Identifier>,
        span: Span,
    },

    /// Fixed-size array type `[T; N]`: `N` elements of `element`, with `N`
    /// known at compile time. `span` covers the leading `[` through the closing `]`.
    Array {
        element: Box<Type>,
        size: ArraySize,
        span: Span,
    },

    /// Anonymous tuple type `(T1, T2, ...)`: a fixed-size, heterogeneous,
    /// positionally-indexed aggregate. `span` covers the leading `(` through the
    /// closing `)`. Always has at least two element types — a single
    /// parenthesized type is grouping, and the empty tuple (unit) is a separate
    /// concern not yet produced here.
    Tuple { elements: Vec<Type>, span: Span },

    /// Generic type application `Name<T1, T2, ...>`: a nominal type
    /// constructor applied to type arguments, e.g. `Pair<i32, f64>`. Each argument
    /// is itself a type (and may name an in-scope type parameter inside a generic
    /// definition). `span` covers the name through the closing `>`. Monomorphization
    /// substitutes the arguments to produce a distinct concrete type.
    Generic {
        name: Identifier,
        args: Vec<GenericArg>,
        span: Span,
    },

    /// Static-dispatch trait bound `impl Trait`: anonymous-generic sugar.
    /// In argument position the parser rewrites it into a fresh trait-bounded generic
    /// parameter (so this node never reaches semantic there); in return position it
    /// survives and is resolved transparently to the concrete type the body produces.
    ImplTrait { trait_name: Identifier, span: Span },

    /// Dynamic-dispatch trait object `dyn Trait`: an unsized runtime type that
    /// only ever appears behind a reference (`&dyn Trait` / `&mut dyn Trait`). Method
    /// calls go through a vtable. Semantic analysis rejects a bare (unreferenced) `dyn`
    /// and any non-object-safe trait.
    DynTrait { trait_name: Identifier, span: Span },

    /// Closure / function type `(T1, T2, ...) -> R`: the type of a callable value
    /// (a closure literal or a function-typed parameter). Parentheses around the
    /// parameter types distinguish it from a closure *literal* `|p| body`. `span`
    /// covers the leading `(` through the return type.
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
        span: Span,
    },

    /// Tensor type for multi-dimensional arrays.
    ///
    /// This variant is reserved for future language support and is not yet
    /// produced by the parser.
    /// Example target syntax: `Tensor<f32, [3, 3]>`.
    Tensor {
        element_type: Box<Type>,
        shape: Vec<usize>,
        span: Span,
    },
}

impl Type {
    /// Get the span of this type annotation
    pub fn span(&self) -> Span {
        match self {
            Type::Named(ident) => ident.span,
            Type::Reference { span, .. } => *span,
            Type::Array { span, .. } => *span,
            Type::Tuple { span, .. } => *span,
            Type::Generic { span, .. } => *span,
            Type::ImplTrait { span, .. } => *span,
            Type::DynTrait { span, .. } => *span,
            Type::Function { span, .. } => *span,
            Type::Tensor { span, .. } => *span,
        }
    }
}
