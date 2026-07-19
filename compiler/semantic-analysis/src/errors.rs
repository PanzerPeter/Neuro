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

    #[error("generic type parameter '{name}' at {span:?} shadows a built-in type name")]
    GenericParamShadowsBuiltin { name: String, span: Span },

    #[error("generic parameter '{name}' at {span:?} cannot be inferred from the call arguments; supply it explicitly with a turbofish, e.g. `f::<...>(...)`")]
    GenericParamNotInferable { name: String, span: Span },

    #[error("array length '{name}' at {span:?} is not a known constant; use an integer literal or an in-scope `const` generic parameter")]
    UnknownArrayLength { name: String, span: Span },

    #[error("undeclared lifetime `'{name}` at {span:?}; declare it in the generic parameter list, e.g. `func f<'{name}>(...)`")]
    UndeclaredLifetime { name: String, span: Span },

    #[error("const generic parameter '{name}' at {span:?} has non-integer type '{ty}'; const parameters must be an integer type")]
    ConstParamNotInteger { name: String, ty: Type, span: Span },

    #[error("`where` predicate at {span:?} is not satisfied for this instantiation")]
    ConstPredicateViolated { span: Span },

    #[error("turbofish at {span:?} supplies {found} generic argument(s), but '{name}' declares {expected}")]
    TurbofishCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("turbofish argument for parameter '{param}' at {span:?} has the wrong kind: a {expected} argument was expected")]
    TurbofishKindMismatch {
        param: String,
        expected: String,
        span: Span,
    },

    #[error("type argument '{ty}' for generic parameter '{param}' at {span:?} is not Copy; generic type arguments are restricted to Copy types in this phase")]
    GenericArgumentNotCopy { param: String, ty: Type, span: Span },

    #[error("generic struct '{name}' at {span:?} requires type arguments, e.g. `{name}<...>`")]
    GenericStructNeedsArgs { name: String, span: Span },

    #[error(
        "generic struct '{name}' at {span:?} expects {expected} type argument(s), found {found}"
    )]
    GenericArgCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("type argument list applied to non-generic type '{name}' at {span:?}")]
    NotAGenericType { name: String, span: Span },

    #[error("nested generic type argument at {span:?} is not yet supported: a generic type may not be instantiated with an enclosing type parameter in this phase")]
    NestedGenericTypeArg { span: Span },

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

    #[error("type '{type_name}' implements Drop at {span:?} and so cannot be Copy: a type with a destructor must be moved, not duplicated")]
    DropTypeCannotBeCopy { type_name: String, span: Span },

    #[error("invalid `impl Drop for {type_name}` at {span:?}: {reason}")]
    InvalidDropImpl {
        type_name: String,
        reason: String,
        span: Span,
    },

    #[error("unknown trait '{trait_name}' at {span:?}: no `trait {trait_name}` is declared")]
    UnknownTrait { trait_name: String, span: Span },

    #[error("trait '{trait_name}' is already defined at {span:?}")]
    TraitAlreadyDefined { trait_name: String, span: Span },

    #[error("`dyn {trait_name}` at {span:?} is unsized and must appear behind a reference — write `&dyn {trait_name}` or `&mut dyn {trait_name}`")]
    DynTraitNotBehindReference { trait_name: String, span: Span },

    #[error("trait '{trait_name}' is not object-safe and cannot be used as `dyn {trait_name}` at {span:?}: {reason}")]
    TraitNotObjectSafe {
        trait_name: String,
        reason: String,
        span: Span,
    },

    #[error("`impl Trait` at {span:?} is only allowed in a function parameter or return type")]
    ImplTraitNotAllowedHere { span: Span },

    #[error("cannot infer the concrete type of the `impl {trait_name}` return at {span:?}: return a direct constructor (a struct literal or enum value); other forms await closures/iterators")]
    ImplReturnNotInferable { trait_name: String, span: Span },

    #[error("the `impl {trait_name}` return type at {span:?} resolves to `{ty}`, which does not implement '{trait_name}'")]
    ImplReturnDoesNotImplement {
        trait_name: String,
        ty: Type,
        span: Span,
    },

    #[error(
        "`impl {trait_name} for {type_name}` at {span:?} is missing required method '{method}'"
    )]
    MissingTraitMethod {
        trait_name: String,
        type_name: String,
        method: String,
        span: Span,
    },

    #[error("method '{method}' at {span:?} is not a member of trait '{trait_name}'")]
    NotATraitMethod {
        trait_name: String,
        method: String,
        span: Span,
    },

    #[error("method '{method}' in `impl {trait_name} for {type_name}` at {span:?} does not match the trait signature: {detail}")]
    TraitMethodSignatureMismatch {
        trait_name: String,
        type_name: String,
        method: String,
        detail: String,
        span: Span,
    },

    #[error("type argument `{ty}` for '{param}' does not implement required trait '{trait_name}' at {span:?}")]
    TraitBoundNotSatisfied {
        param: String,
        ty: Type,
        trait_name: String,
        span: Span,
    },

    #[error("cannot apply binary operator {op} to types {left} and {right} at {span:?}")]
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("operator trait '{trait_name}' can only be implemented for a `Copy` type; '{type_name}' at {span:?} is not `Copy`")]
    OperatorTraitRequiresCopy {
        trait_name: String,
        type_name: String,
        span: Span,
    },

    #[error("in `impl {trait_name}`, `type Output = {expected}` does not match method return type {found} at {span:?}")]
    AssociatedTypeMismatch {
        trait_name: String,
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("`impl {trait_name} for {type_name}` at {span:?} requires `impl {supertrait} for {type_name}`")]
    MissingSupertraitImpl {
        trait_name: String,
        supertrait: String,
        type_name: String,
        span: Span,
    },

    #[error("arithmetic operator {op} is not defined on half-precision type {ty} at {span:?}: compute in f32, e.g. `(a as f32 {op} b as f32)`")]
    HalfFloatArithmetic { op: String, ty: Type, span: Span },

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

    #[error("cannot borrow '{name}' as mutable at {span:?}: it is already borrowed; a `&mut` borrow is exclusive — no other borrow of '{name}' may be live at the same time")]
    CannotMutablyBorrowWhileBorrowed { name: String, span: Span },

    #[error("cannot borrow '{name}' as immutable at {span:?}: it is already mutably borrowed; an active `&mut` borrow excludes all other borrows of '{name}'")]
    CannotBorrowWhileMutablyBorrowed { name: String, span: Span },

    #[error("cannot return a reference to '{name}' at {span:?}: it is local to this function and does not outlive the call; return a reference derived from a parameter instead")]
    ReturnsReferenceToLocal { name: String, span: Span },

    #[error("a range expression `a..b` is only valid as the argument to `.slice()` at {span:?}")]
    RangeNotAllowed { span: Span },

    #[error("`.slice()` expects a range argument `a..b` or `a..=b` at {span:?}")]
    SliceExpectsRange { span: Span },

    #[error("array element type {ty} is not Copy at {span:?}: arrays of non-Copy element types (strings, non-Copy structs) are not yet supported")]
    NonCopyArrayElement { ty: Type, span: Span },

    #[error("cannot index a value of type {found} at {span:?}: indexing applies only to arrays `[T; N]`")]
    NotIndexable { found: Type, span: Span },

    #[error("array index must be an integer, found {found} at {span:?}")]
    IndexNotInteger { found: Type, span: Span },

    #[error(
        "array literal has {found} elements but type annotation expects {expected} at {span:?}"
    )]
    ArrayLengthMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("tuple element type {ty} is not Copy at {span:?}: tuples of non-Copy element types (strings, non-Copy structs) are not yet supported")]
    NonCopyTupleElement { ty: Type, span: Span },

    #[error("cannot index a value of type {found} at {span:?}: `.N` tuple indexing applies only to tuples `(T1, T2, ...)`")]
    NotATuple { found: Type, span: Span },

    #[error("tuple index {index} is out of range at {span:?}: the tuple has {arity} elements")]
    TupleIndexOutOfBounds {
        index: usize,
        arity: usize,
        span: Span,
    },

    #[error("cannot infer the element type of an empty array literal at {span:?}: add a type annotation like `[i32; 0]`")]
    CannotInferEmptyArray { span: Span },

    #[error("array destructuring pattern binds {expected} element(s) but the array has {found} at {span:?}: list every element or add a `..rest` pattern")]
    ArrayPatternLengthMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("enum '{name}' is already defined at {span:?}")]
    EnumAlreadyDefined { name: String, span: Span },

    #[error("type name '{name}' is already defined at {span:?}: a newtype may not reuse the name of an existing type")]
    NewtypeAlreadyDefined { name: String, span: Span },

    #[error("newtype '{name}' wraps non-Copy inner type {inner} at {span:?}: newtype inner types are restricted to Copy types in this phase")]
    NewtypeInnerNotCopy {
        name: String,
        inner: Type,
        span: Span,
    },

    #[error("newtype '{name}' is cyclic at {span:?}: a newtype may not wrap itself directly or transitively")]
    CyclicNewtype { name: String, span: Span },

    #[error("enum variant payload type {ty} is not supported at {span:?}: enum variants may only carry scalar Copy primitives (integers, floats, bool, char) in this phase")]
    UnsupportedEnumPayload { ty: Type, span: Span },

    #[error("enum '{enum_name}' has no variant '{variant}' at {span:?}")]
    UnknownEnumVariant {
        enum_name: String,
        variant: String,
        span: Span,
    },

    #[error("enum variant '{enum_name}::{variant}' is a {expected} variant at {span:?}: {hint}")]
    EnumVariantFormMismatch {
        enum_name: String,
        variant: String,
        expected: String,
        hint: String,
        span: Span,
    },

    #[error("enum variant '{enum_name}::{variant}' takes {expected} field(s) but {found} were provided at {span:?}")]
    EnumVariantArityMismatch {
        enum_name: String,
        variant: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("enum variant '{enum_name}::{variant}' has no field '{field}' at {span:?}")]
    UnknownEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("missing field '{field}' for enum variant '{enum_name}::{variant}' at {span:?}")]
    MissingEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("field '{field}' is set more than once for enum variant '{enum_name}::{variant}' at {span:?}")]
    DuplicateEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("non-exhaustive match at {span:?}: {reason} — add the missing pattern(s) or a `_` wildcard arm")]
    NonExhaustiveMatch { reason: String, span: Span },

    #[error("cannot match on a value of type {ty} at {span:?}: `match` supports enums, integers, `char`, and `bool` in this phase")]
    UnsupportedMatchScrutinee { ty: Type, span: Span },

    #[error("this pattern matches {pattern_ty} but the value being matched has type {scrutinee_ty} at {span:?}")]
    PatternTypeMismatch {
        pattern_ty: String,
        scrutinee_ty: Type,
        span: Span,
    },

    #[error("match arms have incompatible types at {span:?}: expected {expected}, found {found}")]
    MatchArmTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("a range pattern requires an ordered scalar (integer or `char`) at {span:?}")]
    InvalidRangePattern { span: Span },

    #[error("enum variant '{enum_name}::{variant}' is a {expected} variant; its pattern must match that form at {span:?}")]
    VariantPatternFormMismatch {
        enum_name: String,
        variant: String,
        expected: String,
        span: Span,
    },

    #[error("an alternative (`|`) pattern may not bind a variable at {span:?}: move the binding to a separate arm")]
    OrPatternBinding { span: Span },

    #[error("a payload sub-pattern must be a binding or `_` at {span:?}: match a payload value with a guard instead (e.g. `Some(n) if n == 0`)")]
    RefutablePayloadPattern { span: Span },
}
