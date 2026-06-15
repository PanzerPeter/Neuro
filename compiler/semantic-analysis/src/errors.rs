// Neuro Programming Language - Semantic Analysis
// Type checking error definitions

use shared_types::Span;
use thiserror::Error;

use crate::types::Type;

/// Type checking errors with source location information
#[derive(Debug, Error, Clone, PartialEq)]
pub enum TypeError {
    #[error("type mismatch at {span:?}: expected {expected}, found {found}")]
    Mismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("undefined variable '{name}' at {span:?}")]
    UndefinedVariable { name: String, span: Span },

    #[error("undefined function '{name}' at {span:?}")]
    UndefinedFunction { name: String, span: Span },

    #[error("variable '{name}' already defined in this scope at {span:?}")]
    VariableAlreadyDefined { name: String, span: Span },

    #[error("function '{name}' already defined at {span:?}")]
    FunctionAlreadyDefined { name: String, span: Span },

    #[error("incorrect number of arguments at {span:?}: expected {expected}, found {found}")]
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("cannot apply operator {op} to type {ty} at {span:?}")]
    InvalidOperator { op: String, ty: Type, span: Span },

    #[error("struct '{struct_name}' cannot derive Copy at {span:?}: field '{field_name}' has type {field_type}, which is not Copy")]
    CopyDeriveNonCopyField {
        struct_name: String,
        field_name: String,
        field_type: Type,
        span: Span,
    },

    #[error("cannot apply binary operator {op} to types {left} and {right} at {span:?}")]
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("return type mismatch at {span:?}: expected {expected}, found {found}")]
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("missing return statement in function returning {expected} at {span:?}")]
    MissingReturn { expected: Type, span: Span },

    #[error("unknown type name '{name}' at {span:?}")]
    UnknownTypeName { name: String, span: Span },

    #[error("cannot call non-function type {ty} at {span:?}")]
    NotCallable { ty: Type, span: Span },

    #[error("variable '{name}' used without initialization at {span:?}")]
    UninitializedVariable { name: String, span: Span },

    #[error("cannot assign to immutable variable '{name}' at {span:?}")]
    AssignToImmutable { name: String, span: Span },

    #[error("integer literal {value} out of range for type {ty} at {span:?}")]
    IntegerLiteralOutOfRange { value: i64, ty: Type, span: Span },

    #[error("'break' used outside of a loop at {span:?}")]
    BreakOutsideLoop { span: Span },

    #[error("'continue' used outside of a loop at {span:?}")]
    ContinueOutsideLoop { span: Span },

    #[error("use of undefined loop label '{name}' at {span:?}")]
    UndefinedLabel { name: String, span: Span },

    #[error("'break' with a value is only allowed in a 'loop'; 'while' and 'for' always yield unit, at {span:?}")]
    BreakValueInUnitLoop { span: Span },

    #[error("for-range bound must be an integer type, found {found} at {span:?}")]
    InvalidForRangeType { found: Type, span: Span },

    #[error("struct '{name}' already defined at {span:?}")]
    StructAlreadyDefined { name: String, span: Span },

    #[error("unknown struct '{name}' at {span:?}")]
    UnknownStruct { name: String, span: Span },

    #[error("struct '{struct_name}' has no field '{field_name}' at {span:?}")]
    UnknownField {
        struct_name: String,
        field_name: String,
        span: Span,
    },

    #[error("missing field '{field_name}' in struct literal for '{struct_name}' at {span:?}")]
    MissingStructField {
        struct_name: String,
        field_name: String,
        span: Span,
    },

    #[error("field '{field_name}' provided more than once in struct literal at {span:?}")]
    DuplicateStructField { field_name: String, span: Span },

    #[error("cannot assign to field '{field_name}' of immutable binding '{var_name}' at {span:?}")]
    AssignToImmutableField {
        var_name: String,
        field_name: String,
        span: Span,
    },

    #[error("struct '{struct_name}' has no method '{method_name}' at {span:?}")]
    MethodNotFound {
        struct_name: String,
        method_name: String,
        span: Span,
    },

    #[error("impl block for '{type_name}' at {span:?}: '{self_param}' methods are not yet supported (ownership semantics pending)")]
    UnsupportedSelfParam {
        type_name: String,
        self_param: String,
        span: Span,
    },

    #[error("unknown type '{type_name}' in path expression '{type_name}::{member}' at {span:?}")]
    UnknownPathType {
        type_name: String,
        member: String,
        span: Span,
    },

    #[error("'{type_name}' has no associated function '{member}' at {span:?}")]
    UnknownAssociatedFunction {
        type_name: String,
        member: String,
        span: Span,
    },

    #[error("constant '{name}' already defined at {span:?}")]
    ConstAlreadyDefined { name: String, span: Span },

    #[error("constant expression required at {span:?}: only literals, arithmetic on literals, and references to other constants are allowed")]
    InvalidConstExpr { span: Span },

    #[error("const '{name}' references undefined constant '{referenced}' at {span:?}")]
    UndefinedConst {
        name: String,
        referenced: String,
        span: Span,
    },

    #[error("operator '{op}' is not yet supported at {span:?}: {hint}")]
    OperatorNotYetSupported {
        op: String,
        hint: String,
        span: Span,
    },

    #[error("comparison operators cannot be chained at {span:?}: use `&&` to combine separate comparisons")]
    ComparisonChain { span: Span },

    #[error("use of moved value '{name}' at {span:?}: it was moved at {moved_at:?}; bind a `.clone()` if you need an independent copy")]
    UseOfMovedValue {
        name: String,
        span: Span,
        moved_at: Span,
    },

    #[error("cannot borrow this expression at {span:?}: `&` requires a place (a variable); bind it to a `val` first")]
    CannotBorrowValue { span: Span },

    #[error("cannot mutably borrow '{name}' at {span:?}: `&mut` requires a `mut` binding; declare it with `mut`")]
    CannotBorrowMutably { name: String, span: Span },

    #[error("cannot dereference a non-reference value of type `{found}` at {span:?}: `*` applies only to `&T` / `&mut T`")]
    CannotDereference { found: Type, span: Span },

    #[error("cannot assign through an immutable reference `&{inner}` at {span:?}: writing through `*` requires a `&mut {inner}`")]
    CannotAssignThroughRef { inner: Type, span: Span },
}
