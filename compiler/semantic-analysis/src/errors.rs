// Type checking error definitions

use shared_types::Span;
use thiserror::Error;

use crate::types::Type;

/// Type checking errors with source location information
#[derive(Debug, Error, Clone, PartialEq)]
pub enum TypeError {
    #[error("type mismatch: expected {expected}, found {found}")]
    Mismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("undefined variable '{name}'")]
    UndefinedVariable { name: String, span: Span },

    #[error("undefined function '{name}'")]
    UndefinedFunction { name: String, span: Span },

    #[error("'{name}' is a function, not a value; functions are not first-class here; wrap it in a closure with annotated parameters, e.g. `|x: T| -> R {{ {name}(x) }}`")]
    FunctionUsedAsValue { name: String, span: Span },

    #[error("generic type parameter '{name}' shadows a built-in type name")]
    GenericParamShadowsBuiltin { name: String, span: Span },

    #[error("generic parameter '{name}' cannot be inferred from the call arguments; supply it explicitly with a turbofish, e.g. `f::<...>(...)`")]
    GenericParamNotInferable { name: String, span: Span },

    #[error("array length '{name}' is not a known constant; use an integer literal or an in-scope `const` generic parameter")]
    UnknownArrayLength { name: String, span: Span },

    #[error("undeclared lifetime `'{name}`; declare it in the generic parameter list, e.g. `func f<'{name}>(...)`")]
    UndeclaredLifetime { name: String, span: Span },

    #[error("const generic parameter '{name}' has non-integer type '{ty}'; const parameters must be an integer type")]
    ConstParamNotInteger { name: String, ty: Type, span: Span },

    #[error("`where` predicate is not satisfied for this instantiation")]
    ConstPredicateViolated { span: Span },

    #[error("turbofish supplies {found} generic argument(s), but '{name}' declares {expected}")]
    TurbofishCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("turbofish argument for parameter '{param}' has the wrong kind: a {expected} argument was expected")]
    TurbofishKindMismatch {
        param: String,
        expected: String,
        span: Span,
    },

    #[error("generic struct '{name}' requires type arguments, e.g. `{name}<...>`")]
    GenericStructNeedsArgs { name: String, span: Span },

    #[error("generic enum '{name}' requires type arguments, e.g. `{name}<...>`")]
    GenericEnumNeedsArgs { name: String, span: Span },

    #[error("cannot infer the type arguments of generic enum '{name}'; annotate the target, e.g. `val x: {name}<...> = ...`, or construct a variant that carries them")]
    GenericEnumNotInferable { name: String, span: Span },

    #[error("generic type '{name}' expects {expected} type argument(s), found {found}")]
    GenericArgCountMismatch {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("type argument list applied to non-generic type '{name}'")]
    NotAGenericType { name: String, span: Span },

    #[error("nested generic type argument is not yet supported: a generic type may not be instantiated with an enclosing type parameter in this phase")]
    NestedGenericTypeArg { span: Span },

    #[error("variable '{name}' already defined in this scope")]
    VariableAlreadyDefined { name: String, span: Span },

    #[error("function '{name}' already defined")]
    FunctionAlreadyDefined { name: String, span: Span },

    #[error("incorrect number of arguments: expected {expected}, found {found}")]
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("cannot apply operator {op} to type {ty}")]
    InvalidOperator { op: String, ty: Type, span: Span },

    #[error("struct '{struct_name}' cannot derive Copy: field '{field_name}' has type {field_type}, which is not Copy")]
    CopyDeriveNonCopyField {
        struct_name: String,
        field_name: String,
        field_type: Type,
        span: Span,
    },

    #[error("`@derive({name})` names no derivable trait: the derivable set is {derivable}")]
    UnknownDerive {
        name: String,
        derivable: String,
        span: Span,
    },

    #[error(
        "`@derive({name})` is specified but not implemented yet: write the impl by hand for now"
    )]
    UnimplementedDerive { name: String, span: Span },

    #[error("`@derive({name})` is listed twice on struct '{struct_name}'")]
    DuplicateDerive {
        struct_name: String,
        name: String,
        span: Span,
    },

    #[error("struct '{struct_name}' both derives `{trait_name}` and declares `impl {trait_name} for {struct_name}`: keep one of them")]
    DeriveConflictsWithImpl {
        struct_name: String,
        trait_name: String,
        span: Span,
    },

    #[error("struct '{struct_name}' cannot derive `{trait_name}`: field '{field_name}' has type {field_type}, which {reason}")]
    DeriveFieldUnsupported {
        struct_name: String,
        trait_name: String,
        field_name: String,
        field_type: Type,
        reason: String,
        span: Span,
    },

    #[error("type '{type_name}' implements Drop and so cannot be Copy: a type with a destructor must be moved, not duplicated")]
    DropTypeCannotBeCopy { type_name: String, span: Span },

    #[error("invalid `impl Drop for {type_name}`: {reason}")]
    InvalidDropImpl {
        type_name: String,
        reason: String,
        span: Span,
    },

    #[error("unknown trait '{trait_name}': no `trait {trait_name}` is declared")]
    UnknownTrait { trait_name: String, span: Span },

    #[error("trait '{trait_name}' is already defined")]
    TraitAlreadyDefined { trait_name: String, span: Span },

    #[error("`dyn {trait_name}` is unsized and must appear behind a reference: write `&dyn {trait_name}` or `&mut dyn {trait_name}`")]
    DynTraitNotBehindReference { trait_name: String, span: Span },

    #[error("`[{element}]` is unsized and must appear behind a reference: write `&[{element}]` or `&mut [{element}]`")]
    SliceNotBehindReference { element: String, span: Span },

    #[error("tensor element type {ty} is not a numeric scalar; a tensor's element must be an integer, a floating-point type, or `bool`")]
    NonScalarTensorElement { ty: Type, span: Span },

    #[error("`Tensor` needs a shape: write `Tensor<{element}, [3, 3]>`, or `Tensor<{element}, []>` for a rank-0 scalar tensor")]
    TensorShapeRequired { element: String, span: Span },

    #[error("this literal has {found} element(s) where the tensor's shape declares {expected}")]
    TensorExtentMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("this literal is nested {found} deep, but the tensor has rank {expected}; a nested tensor literal must be rectangular and as deep as the shape is long")]
    TensorRankMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("a rank-0 tensor has no elements to write; build it with `Tensor::scalar(value)` instead of an array literal")]
    TensorScalarNeedsConstructor { span: Span },

    #[error("the tensor type of `Tensor::{ctor}` cannot be inferred here; annotate the binding with `Tensor<T, [...]>`, or name it with a turbofish: `Tensor::<f32, [3, 3]>::{ctor}(...)`")]
    TensorTypeNotInferable { ctor: String, span: Span },

    #[error("`Tensor` has no constructor named '{ctor}'; it provides `zeros`, `ones`, `identity`, `random_normal`, `scalar`, and `from`")]
    UnknownTensorConstructor { ctor: String, span: Span },

    #[error("compound assignment `{op}=` is not defined on a tensor of {element}: `{op}` requires an element type with arithmetic, so use an integer, `f32`, or `f64` tensor")]
    TensorElementNotArithmetic {
        op: String,
        element: Type,
        span: Span,
    },

    #[error("the operands of `{op}` do not broadcast: {left} against {right}; shapes align at the trailing axis, and an axis stretches only where its extent is 1")]
    TensorBroadcastMismatch {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("the operands of `@` do not multiply: {left} against {right}; matrix multiplication takes two rank-2 tensors whose inner axes agree, `[M, K] @ [K, N]` giving `[M, N]`")]
    TensorMatMulMismatch { left: Type, right: Type, span: Span },

    #[error("this index names {found} axis/axes, but the tensor has rank {expected}; a tensor index gives one argument per axis")]
    TensorIndexRankMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("index {index} is outside axis {axis}, whose extent is {extent}")]
    TensorIndexOutOfBounds {
        index: i128,
        axis: usize,
        extent: usize,
        span: Span,
    },

    #[error("the bounds of a tensor slice must be compile-time constants, because the sliced shape is part of the result's type; write literal bounds, or index one position with a runtime value")]
    TensorSliceBoundNotConstant { span: Span },

    #[error("the slice `{start}..{end}` does not name a sub-range of axis {axis}, whose extent is {extent}; a slice runs forward and stops at the extent")]
    TensorSliceOutOfRange {
        start: i128,
        end: i128,
        axis: usize,
        /// The axis's extent as written: a number, or a shape parameter's name.
        extent: String,
        span: Span,
    },

    #[error("{found} is not a tensor, so it takes one index and no range: index an array or a `Vec` with `xs[i]`, and take a sub-range of one with `xs.slice(a..b)`")]
    TensorIndexOnNonTensor { found: Type, span: Span },

    #[error("cannot assign to a tensor slice: an index that leaves an axis standing produces a fresh tensor, not storage; name every axis to write one element")]
    AssignToTensorSlice { span: Span },

    #[error("tensor dimension '{name}' is not a known extent; use a non-negative integer, or declare it as a shape parameter of the enclosing function, e.g. `func f<{name}>(t: Tensor<f32, [{name}]>)`")]
    UnknownTensorDimension { name: String, span: Span },

    #[error("shape parameter '{name}' is already {expected} here, but this argument makes it {found}; one shape parameter names one extent, so every position that writes it must agree")]
    TensorShapeParamConflict {
        name: String,
        expected: u64,
        found: u64,
        span: Span,
    },

    #[error("tensor dimension name '{name}' is used twice in one shape; each axis of a tensor needs its own name")]
    DuplicateTensorAxisName { name: String, span: Span },

    #[error("tensor axis {axis} is named '{expected}' here but '{found}'; the two shapes name the same axis differently, which is the transposition named dimensions exist to catch")]
    TensorAxisNameMismatch {
        axis: usize,
        expected: String,
        found: String,
        span: Span,
    },

    #[error("this literal is written against a shape whose extent '{name}' is a shape parameter, so its length cannot be checked here; build the tensor with a constructor instead, e.g. `Tensor::<f32, [{name}]>::zeros()`")]
    TensorLiteralSymbolicExtent { name: String, span: Span },

    #[error("`.t()` transposes a matrix, but this tensor has rank {rank}; use `.permute([...])` to reorder the axes of a rank-{rank} tensor")]
    TensorTransposeRank { rank: usize, span: Span },

    #[error(
        "`.{method}` needs its axes written as an array literal, e.g. `.{method}([{example}])`"
    )]
    TensorShapeArgNotLiteral {
        method: String,
        example: String,
        span: Span,
    },

    #[error("this extent is not a constant; `.reshape` takes an array of integer literals, with `-1` in at most one position to infer that extent")]
    TensorReshapeExtentNotConstant { span: Span },

    #[error("`.reshape` writes `-1` more than once; only one extent can be inferred, because the rest have to determine it")]
    TensorReshapeRepeatedInference { span: Span },

    #[error("`.reshape` would hold {found} elements but the receiver holds {expected}; a reshape rearranges a tensor's extents and cannot change how many elements it has")]
    TensorReshapeElementCount {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("`-1` cannot be inferred: the other extents multiply to {known}, which does not divide the receiver's {total} elements")]
    TensorReshapeIndivisible {
        known: usize,
        total: usize,
        span: Span,
    },

    #[error("this tensor has no dimension named '{name}'; its shape declares {declared}")]
    UnknownTensorAxisName {
        name: String,
        declared: String,
        span: Span,
    },

    #[error("axis {axis} is out of range for a rank-{rank} tensor; axes are numbered 0 to {}", rank.saturating_sub(1))]
    TensorAxisOutOfRange {
        axis: usize,
        rank: usize,
        span: Span,
    },

    #[error("`.{method}` names axis {axis} twice; each axis may be named once")]
    TensorAxisRepeated {
        method: String,
        axis: usize,
        span: Span,
    },

    #[error("`.permute` was given {found} axes for a rank-{rank} tensor; a permutation names every axis exactly once")]
    TensorPermuteRank {
        found: usize,
        rank: usize,
        span: Span,
    },

    #[error("`.flatten` was given axes that are not adjacent; flattening merges a contiguous run of axes into one, so name them in shape order")]
    TensorFlattenNotAdjacent { span: Span },

    #[error("`.flatten` was given an empty axis list; name the axes to merge, or call `.flatten()` to merge them all")]
    TensorFlattenNoAxes { span: Span },

    #[error("{operation} needs every extent of `{ty}` at compile time, but an axis is `?`; a dynamic extent is not known until run time, so build or read the tensor at a static shape and pass it where a `?` is expected")]
    TensorDynamicExtent {
        operation: String,
        ty: Type,
        span: Span,
    },

    #[error("`.{method}` needs every extent of the receiver to be known here, but '{name}' is a shape parameter; a shape-generic tensor's extents are not numbers until it is instantiated")]
    TensorShapeCastSymbolicExtent {
        method: String,
        name: String,
        span: Span,
    },

    #[error("`.{method}()` reduces a tensor's elements, which requires an integer or `f32`/`f64` element type; this tensor holds {element}")]
    TensorReduceElementType {
        method: String,
        element: Type,
        span: Span,
    },

    #[error("`.mean()` averages {element} elements, which has no rounding rule; sum with `.sum()` and divide, or build the tensor at `f32`/`f64`")]
    TensorReduceMeanNotFloat { element: Type, span: Span },

    #[error("`.{method}()` reduces over no elements, so it has no value to produce; reduce a tensor whose reduced axis is non-empty")]
    TensorReduceEmpty { method: String, span: Span },

    #[error("`.{method}()` orders a tensor's elements, which requires an integer or `f32`/`f64` element type; this tensor holds {element}")]
    TensorSortElementType {
        method: String,
        element: Type,
        span: Span,
    },

    #[error("`.{method}()` orders one axis of a tensor, and a rank-0 tensor has none; order a tensor with at least one axis")]
    TensorSortRankZero { method: String, span: Span },

    #[error("`.{method}()` orders an axis with no elements, so there is no ordering to produce; order a tensor whose sorted axis is non-empty")]
    TensorSortEmpty { method: String, span: Span },

    #[error("the `{label}:` argument of `.{method}()` has to be a constant, because it decides the result's shape and the comparator before any element is read; write it as a literal, e.g. `{example}`")]
    TensorSortArgNotConstant {
        method: String,
        label: String,
        example: String,
        span: Span,
    },

    #[error("`.topk(k: {k})` selects more elements than the sorted axis holds, which is {extent}; ask for between 1 and {extent}")]
    TensorTopKOutOfRange { k: usize, extent: usize, span: Span },

    #[error("`einsum` reads its subscripts at compile time, so they have to be a string literal, e.g. `einsum(\"ij,jk->ik\", a, b)`; a value computed at run time cannot decide the result's shape")]
    EinsumSubscriptNotLiteral { span: Span },

    #[error("`einsum(\"{subscripts}\", ...)` is not a subscript string: {reason}")]
    EinsumMalformedSubscripts {
        subscripts: String,
        reason: String,
        span: Span,
    },

    #[error("`einsum` was given {found} operands for {expected} comma-separated subscripts; write one subscript per operand")]
    EinsumOperandCount {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("`einsum` operand {position} is {ty}, not a tensor; every operand of a contraction is a tensor whose rank its subscript names")]
    EinsumOperandNotTensor {
        position: usize,
        ty: Type,
        span: Span,
    },

    #[error("subscript '{subscript}' names {expected} axes but `einsum` operand {position} has rank {found}; a subscript carries one letter per axis")]
    EinsumOperandRank {
        position: usize,
        subscript: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("`einsum` operand {position} holds {found} elements but the first holds {expected}; every operand of one contraction shares an element type")]
    EinsumElementMismatch {
        position: usize,
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("`einsum` contracts a tensor's elements, which requires an integer or `f32`/`f64` element type; this tensor holds {element}")]
    EinsumElementType { element: Type, span: Span },

    #[error("`einsum` binds '{letter}' to extent {first} and then to {second}; a repeated letter names one axis length, which is what makes the contraction well defined")]
    EinsumExtentConflict {
        letter: char,
        first: usize,
        second: usize,
        span: Span,
    },

    #[error("`einsum` writes '{letter}' on the right of `->` but no operand's subscript binds it; an output letter takes its extent from an input")]
    EinsumOutputLetterUnbound { letter: char, span: Span },

    #[error("`einsum` writes '{letter}' twice on the right of `->`; each result axis is a distinct letter, because a repeated one would name two extents at once")]
    EinsumOutputLetterRepeated { letter: char, span: Span },

    #[error("`einsum` needs every extent of operand {position} at compile time, but '{name}' is not a number here; contract tensors whose shapes are known where the call is written")]
    EinsumSymbolicExtent {
        position: usize,
        name: String,
        span: Span,
    },

    #[error("`Tensor::{ctor}` does not apply to {ty}: {reason}")]
    TensorConstructorNotApplicable {
        ctor: String,
        ty: Type,
        reason: String,
        span: Span,
    },

    #[error("trait '{trait_name}' is not object-safe and cannot be used as `dyn {trait_name}`: {reason}")]
    TraitNotObjectSafe {
        trait_name: String,
        reason: String,
        span: Span,
    },

    #[error("`impl Trait` is only allowed in a function parameter or return type")]
    ImplTraitNotAllowedHere { span: Span },

    #[error("cannot infer the concrete type of the `impl {trait_name}` return: return a direct constructor (a struct literal or enum value); other forms await closures/iterators")]
    ImplReturnNotInferable { trait_name: String, span: Span },

    #[error("the `impl {trait_name}` return type resolves to `{ty}`, which does not implement '{trait_name}'")]
    ImplReturnDoesNotImplement {
        trait_name: String,
        ty: Type,
        span: Span,
    },

    #[error("`impl {trait_name} for {type_name}` is missing required method '{method}'")]
    MissingTraitMethod {
        trait_name: String,
        type_name: String,
        method: String,
        span: Span,
    },

    #[error("method '{method}' is not a member of trait '{trait_name}'")]
    NotATraitMethod {
        trait_name: String,
        method: String,
        span: Span,
    },

    #[error("method '{method}' in `impl {trait_name} for {type_name}` does not match the trait signature: {detail}")]
    TraitMethodSignatureMismatch {
        trait_name: String,
        type_name: String,
        method: String,
        detail: String,
        span: Span,
    },

    #[error("type argument `{ty}` for '{param}' does not implement required trait '{trait_name}'")]
    TraitBoundNotSatisfied {
        param: String,
        ty: Type,
        trait_name: String,
        span: Span,
    },

    #[error("cannot apply binary operator {op} to types {left} and {right}")]
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("cannot compare values of type '{type_name}' with `{op}`: '{type_name}' implements no `PartialEq`; add `impl PartialEq for {type_name}` (the struct must derive `Copy`) or compare the fields")]
    MissingPartialEqImpl {
        type_name: String,
        op: String,
        span: Span,
    },

    #[error("operator trait '{trait_name}' can only be implemented for a `Copy` type; '{type_name}' is not `Copy`")]
    OperatorTraitRequiresCopy {
        trait_name: String,
        type_name: String,
        span: Span,
    },

    #[error("in `impl {trait_name}`, `type Output = {expected}` does not match method return type {found}")]
    AssociatedTypeMismatch {
        trait_name: String,
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("`Self::{name}` has no binding here: an associated type is named inside the trait that declares it or an `impl` that binds it")]
    UnboundAssociatedType { name: String, span: Span },

    #[error("trait '{trait_name}' declares no associated type '{name}'")]
    UnknownAssociatedType {
        trait_name: String,
        name: String,
        span: Span,
    },

    #[error("`impl {trait_name} for {type_name}` does not bind associated type '{name}': add `type {name} = <type>`")]
    MissingAssociatedType {
        trait_name: String,
        type_name: String,
        name: String,
        span: Span,
    },

    #[error("method '{method}' of trait '{trait_name}' names associated type `Self::{assoc}`, which this bound leaves open: write `{trait_name}<{assoc} = T>` to say what it is")]
    UnconstrainedAssociatedType {
        trait_name: String,
        method: String,
        assoc: String,
        span: Span,
    },

    #[error("bound `{trait_name}<{assoc} = {expected}>` is not satisfied by {ty}, which binds `{assoc}` to {found}")]
    AssociatedTypeBoundMismatch {
        trait_name: String,
        assoc: String,
        expected: Type,
        found: Type,
        ty: Type,
        span: Span,
    },

    #[error("`impl {trait_name} for {type_name}` requires `impl {supertrait} for {type_name}`")]
    MissingSupertraitImpl {
        trait_name: String,
        supertrait: String,
        type_name: String,
        span: Span,
    },

    #[error("arithmetic operator {op} is not defined on half-precision type {ty}: compute in f32, e.g. `(a as f32 {op} b as f32)`")]
    HalfFloatArithmetic { op: String, ty: Type, span: Span },

    #[error("return type mismatch: expected {expected}, found {found}")]
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("missing return statement in function returning {expected}")]
    MissingReturn { expected: Type, span: Span },

    #[error("unknown type name '{name}'")]
    UnknownTypeName { name: String, span: Span },

    #[error("cannot call non-function type {ty}")]
    NotCallable { ty: Type, span: Span },

    #[error("`>>` composes named functions: '{name}' names no function in this program")]
    ComposeUndefined { name: String, span: Span },

    #[error(
        "`>>` composes named functions: '{name}' is a binding, so name the `func` it holds instead"
    )]
    ComposeNotANamedFunction { name: String, span: Span },

    #[error(
        "`>>` cannot compose the generic function '{name}': a composition fixes no type arguments"
    )]
    ComposeGenericFunction { name: String, span: Span },

    #[error("`>>` composes functions of one parameter: '{name}' takes {found}")]
    ComposeArity {
        name: String,
        found: usize,
        span: Span,
    },

    #[error("'{left}' returns {found}, which '{right}' cannot take: it expects {expected}")]
    ComposeStageMismatch {
        left: String,
        right: String,
        found: Type,
        expected: Type,
        span: Span,
    },

    #[error("variable '{name}' used without initialization")]
    UninitializedVariable { name: String, span: Span },

    #[error("cannot bind '{name}': the initializer has type void, which is not a value")]
    VoidBinding { name: String, span: Span },

    #[error("cannot assign to immutable variable '{name}'")]
    AssignToImmutable { name: String, span: Span },

    #[error("integer literal {value} out of range for type {ty}")]
    IntegerLiteralOutOfRange { value: i128, ty: Type, span: Span },

    #[error("literal -{magnitude} is negative and {ty} is unsigned: use a signed type, or `0{ty}.wrapping_sub({magnitude}{ty})` if the two's-complement wrap was intended")]
    NegativeLiteralForUnsignedType {
        magnitude: i128,
        ty: Type,
        span: Span,
    },

    #[error("'break' used outside of a loop")]
    BreakOutsideLoop { span: Span },

    #[error("'continue' used outside of a loop")]
    ContinueOutsideLoop { span: Span },

    #[error("use of undefined loop label '{name}'")]
    UndefinedLabel { name: String, span: Span },

    #[error(
        "'break' with a value is only allowed in a 'loop'; 'while' and 'for' always yield unit,"
    )]
    BreakValueInUnitLoop { span: Span },

    #[error("for-range bound must be an integer type, found {found}")]
    InvalidForRangeType { found: Type, span: Span },

    #[error("name '{name}' contains '__', which is reserved for compiler-generated symbols; use a single underscore")]
    ReservedNameSeparator { name: String, span: Span },

    #[error("struct '{name}' already defined")]
    StructAlreadyDefined { name: String, span: Span },

    #[error("unknown struct '{name}'")]
    UnknownStruct { name: String, span: Span },

    #[error("struct '{struct_name}' has no field '{field_name}'")]
    UnknownField {
        struct_name: String,
        field_name: String,
        span: Span,
    },

    #[error("field '{field_name}' of struct '{struct_name}' is private to the module that declares it; write `export` before the field to make it reachable from another module")]
    PrivateField {
        struct_name: String,
        field_name: String,
        span: Span,
    },

    #[error("missing field '{field_name}' in struct literal for '{struct_name}'")]
    MissingStructField {
        struct_name: String,
        field_name: String,
        span: Span,
    },

    #[error("field '{field_name}' provided more than once in struct literal")]
    DuplicateStructField { field_name: String, span: Span },

    #[error("cannot assign to field '{field_name}' of immutable binding '{var_name}'")]
    AssignToImmutableField {
        var_name: String,
        field_name: String,
        span: Span,
    },

    #[error("struct '{struct_name}' has no method '{method_name}'")]
    MethodNotFound {
        struct_name: String,
        method_name: String,
        span: Span,
    },

    #[error("unknown type '{type_name}' in path expression '{type_name}::{member}'")]
    UnknownPathType {
        type_name: String,
        member: String,
        span: Span,
    },

    #[error("'{type_name}' has no associated function '{member}'")]
    UnknownAssociatedFunction {
        type_name: String,
        member: String,
        span: Span,
    },

    #[error("constant '{name}' already defined")]
    ConstAlreadyDefined { name: String, span: Span },

    #[error("constant expression required: only literals, arithmetic on literals, and references to other constants are allowed")]
    InvalidConstExpr { span: Span },

    #[error("comparison operators cannot be chained: use `&&` to combine separate comparisons")]
    ComparisonChain { span: Span },

    #[error("`??` expects an `Option<T>` or `Result<T, E>` on the left, found {found}: `??` unwraps a fallible value or falls back")]
    NullCoalesceOnNonFallible { found: Type, span: Span },

    #[error("`?` expects an `Option<T>` or `Result<T, E>`, found {found}: `?` unwraps a fallible value or propagates its failure")]
    TryOnNonFallible { found: Type, span: Span },

    #[error("`?` on a {operand} has nowhere to propagate: the enclosing function returns {found}, but it must return an `{expected}` for the failure to be forwarded; otherwise handle the value with `match`, `??`, or `val-else`")]
    TryOutsideFallibleFunction {
        operand: Type,
        expected: String,
        found: Type,
        span: Span,
    },

    #[error("the `else` branch of a `val-else` can fall through: it must exit the scope with `return`, `break`, `continue`, `panic(...)`, or `unreachable()`")]
    ValElseMustDiverge { span: Span },

    #[error("`else |{name}|` has nothing to bind: `Option::None` carries no payload; write `else` or `else |_|`")]
    ValElseBindingOnOption { name: String, span: Span },

    #[error("use of moved value '{name}': bind a `.clone()` if you need an independent copy")]
    UseOfMovedValue {
        name: String,
        span: Span,
        moved_at: Span,
    },

    #[error("'{name}' is moved out inside a loop body, but it is bound outside the loop: the next iteration would move it again. Move a `.clone()` instead, borrow it with `&`, or give the binding a fresh value before the iteration ends")]
    MovedInLoopBody { name: String, span: Span },

    #[error("cannot move out of '{name}': it is reached through a `&` borrow, which owns nothing to give away; bind a `.clone()` instead, or take the value by a binding that owns it")]
    CannotMoveOutOfBorrow { name: String, span: Span },

    #[error("cannot borrow this expression: `&` requires a place (a variable); bind it to a `val` first")]
    CannotBorrowValue { span: Span },

    #[error(
        "cannot mutably borrow '{name}': `&mut` requires a `mut` binding; declare it with `mut`"
    )]
    CannotBorrowMutably { name: String, span: Span },

    #[error("cannot dereference a non-reference value of type `{found}`: `*` applies only to `&T` / `&mut T`")]
    CannotDereference { found: Type, span: Span },

    #[error("cannot assign through an immutable reference `&{inner}`: writing through `*` requires a `&mut {inner}`")]
    CannotAssignThroughRef { inner: Type, span: Span },

    #[error("cannot borrow '{name}' as mutable: it is already borrowed; a `&mut` borrow is exclusive, so no other borrow of '{name}' may be live at the same time")]
    CannotMutablyBorrowWhileBorrowed { name: String, span: Span },

    #[error("cannot borrow '{name}' as immutable: it is already mutably borrowed; an active `&mut` borrow excludes all other borrows of '{name}'")]
    CannotBorrowWhileMutablyBorrowed { name: String, span: Span },

    #[error("cannot use '{name}' while it is mutably borrowed: a `&mut` borrow is exclusive, so every access to '{name}' must go through the borrow until it ends")]
    CannotUseWhileMutablyBorrowed { name: String, span: Span },

    #[error("cannot move out of '{name}' while it is borrowed: the borrow would be left pointing at a value '{name}' no longer owns; end the borrow first, or move a `.clone()`")]
    CannotMoveWhileBorrowed { name: String, span: Span },

    #[error("cannot assign to '{name}' while it is borrowed: the borrow would be left pointing at the replaced value; end the borrow first, or write through the borrow with `*`")]
    CannotAssignWhileBorrowed { name: String, span: Span },

    #[error("cannot return a reference to '{name}': it is local to this function and does not outlive the call; return a reference derived from a parameter instead")]
    ReturnsReferenceToLocal { name: String, span: Span },

    #[error(
        "a range expression `a..b` is only valid as the argument to `.slice()` or `.char_slice()`"
    )]
    RangeNotAllowed { span: Span },

    #[error("`.slice()` / `.char_slice()` expects a range argument `a..b` or `a..=b`")]
    SliceExpectsRange { span: Span },

    #[error("cannot index a value of type {found}: indexing applies only to arrays `[T; N]` and `Vec<T>`")]
    NotIndexable { found: Type, span: Span },

    #[error("cannot iterate over a value of type {found}: a `for` head must be a range, an array, a `Vec<T>`, a `&[T]`, or a type implementing `IntoIterator` or `Iterator`")]
    NotIterable { found: Type, span: Span },

    #[error("cannot iterate over a value of type {found}: the `IntoIterator` / `Iterator` protocol is implemented on the owned type, and a borrow of it is not a `for` head; iterate the value itself")]
    BorrowedIterableHead { found: Type, span: Span },

    #[error("`.{adapter}()` needs a function of one parameter, but was given {found}")]
    LoopAdapterNotCallable {
        adapter: String,
        found: Type,
        span: Span,
    },

    #[error(
        "`.{adapter}()` is applied to elements of type {expected}, but its function takes {found}"
    )]
    LoopAdapterInput {
        adapter: String,
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error(
        "`.{adapter}()` needs a function returning {expected}, but its function returns {found}"
    )]
    LoopAdapterOutput {
        adapter: String,
        expected: String,
        found: Type,
        span: Span,
    },

    #[error("cannot infer the element type of `{name}::new()`; annotate the binding, e.g. `val v: {name}<...> = {name}::new()`")]
    CollectionTypeNotInferable { name: String, span: Span },

    #[error("{ty} cannot be stored in a `{collection}`: elements must be `Copy` or `string`")]
    InvalidCollectionElement {
        collection: String,
        ty: Type,
        span: Span,
    },

    #[error("{ty} is not a valid `{collection}` key: {reason}")]
    InvalidCollectionKey {
        collection: String,
        ty: Type,
        reason: String,
        span: Span,
    },

    #[error("`impl Hashable for {type_name}` must provide exactly `func hash(&self) -> u64`")]
    InvalidHashableImpl { type_name: String, span: Span },

    #[error("an index must be an integer, found {found}")]
    IndexNotInteger { found: Type, span: Span },

    #[error("array literal has {found} elements but type annotation expects {expected}")]
    ArrayLengthMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("cannot index a value of type {found}: `.N` tuple indexing applies only to tuples `(T1, T2, ...)`")]
    NotATuple { found: Type, span: Span },

    #[error("tuple index {index} is out of range: the tuple has {arity} elements")]
    TupleIndexOutOfBounds {
        index: usize,
        arity: usize,
        span: Span,
    },

    #[error("cannot infer the element type of an empty array literal: add a type annotation like `[i32; 0]`")]
    CannotInferEmptyArray { span: Span },

    #[error("array destructuring pattern binds {expected} element(s) but the array has {found}: list every element or add a `..rest` pattern")]
    ArrayPatternLengthMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("enum '{name}' is already defined")]
    EnumAlreadyDefined { name: String, span: Span },

    #[error("type name '{name}' is already defined: a newtype may not reuse the name of an existing type")]
    NewtypeAlreadyDefined { name: String, span: Span },

    #[error("newtype '{name}' is cyclic: a newtype may not wrap itself directly or transitively")]
    CyclicNewtype { name: String, span: Span },

    #[error("enum variant payload type {ty} is not supported: enum variants may only carry scalar Copy primitives (integers, floats, bool, char) in this phase")]
    UnsupportedEnumPayload { ty: Type, span: Span },

    #[error("enum '{enum_name}' has no variant '{variant}'")]
    UnknownEnumVariant {
        enum_name: String,
        variant: String,
        span: Span,
    },

    #[error("enum variant '{enum_name}::{variant}' is a {expected} variant: {hint}")]
    EnumVariantFormMismatch {
        enum_name: String,
        variant: String,
        expected: String,
        hint: String,
        span: Span,
    },

    #[error(
        "enum variant '{enum_name}::{variant}' takes {expected} field(s) but {found} were provided"
    )]
    EnumVariantArityMismatch {
        enum_name: String,
        variant: String,
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("enum variant '{enum_name}::{variant}' has no field '{field}'")]
    UnknownEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("missing field '{field}' for enum variant '{enum_name}::{variant}'")]
    MissingEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("field '{field}' is set more than once for enum variant '{enum_name}::{variant}'")]
    DuplicateEnumField {
        enum_name: String,
        variant: String,
        field: String,
        span: Span,
    },

    #[error("non-exhaustive match: {reason}; add the missing pattern(s) or a `_` wildcard arm")]
    NonExhaustiveMatch { reason: String, span: Span },

    #[error("cannot match on a value of type {ty}: `match` supports enums, integers, `char`, and `bool` in this phase")]
    UnsupportedMatchScrutinee { ty: Type, span: Span },

    #[error(
        "this pattern matches {pattern_ty} but the value being matched has type {scrutinee_ty}"
    )]
    PatternTypeMismatch {
        pattern_ty: String,
        scrutinee_ty: Type,
        span: Span,
    },

    #[error("match arms have incompatible types: expected {expected}, found {found}")]
    MatchArmTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("a range pattern requires an ordered scalar (integer or `char`)")]
    InvalidRangePattern { span: Span },

    #[error("variant '{variant}' is written without its enum: qualify it as `Enum::{variant}` or import it with `import Enum::{{{variant}}}`")]
    UnimportedVariantPattern { variant: String, span: Span },

    #[error("enum variant '{enum_name}::{variant}' is a {expected} variant; its pattern must match that form")]
    VariantPatternFormMismatch {
        enum_name: String,
        variant: String,
        expected: String,
        span: Span,
    },

    #[error(
        "an alternative (`|`) pattern may not bind a variable: move the binding to a separate arm"
    )]
    OrPatternBinding { span: Span },

    #[error("a payload sub-pattern must be a binding or `_`: match a payload value with a guard instead (e.g. `Some(n) if n == 0`)")]
    RefutablePayloadPattern { span: Span },

    #[error("closure parameter '{name}' needs a type annotation: write `|{name}: T| ...`; closure parameter-type inference is not yet supported")]
    ClosureParamNeedsType { name: String, span: Span },

    #[error("closure captures '{name}' of non-Copy type {ty}: only Copy values may be captured in this phase (capture by reference / move of owned values is not yet supported)")]
    ClosureCapturesNonCopy { name: String, ty: Type, span: Span },

    #[error("closure assigns to captured variable '{name}': a captured variable is read-only in this phase (mutable capture / FnMut is not yet supported)")]
    ClosureAssignsCapture { name: String, span: Span },

    #[error("a block-bodied closure needs an explicit return type: write `|params| -> R {{ ... }}` (only single-expression closures `|x| expr` infer their return type)")]
    ClosureBlockNeedsReturnType { span: Span },

    #[error("a value of type {ty} cannot be interpolated into a string: interpolation renders integers, floats, `bool`, `char`, and `string`")]
    UnformattableType { ty: Type, span: Span },

    #[error("a value of struct type '{name}' cannot be interpolated: {hint}")]
    UnrenderableStruct {
        name: String,
        hint: String,
        span: Span,
    },

    #[error("format specifier `{spec}` does not apply to a value of type {ty}: {hint}")]
    FormatSpecMismatch {
        spec: String,
        ty: Type,
        hint: String,
        span: Span,
    },

    #[error("format field width {width} exceeds the maximum of {max}")]
    FormatWidthTooLarge { width: u32, max: u32, span: Span },

    #[error("format precision {precision} exceeds the maximum of {max}")]
    FormatPrecisionTooLarge {
        precision: u32,
        max: u32,
        span: Span,
    },

    #[error("{place} outlives {pool}, so storing a value of type '{ty}' there would leave the arena that the block releases at its closing brace; declare the binding inside the pool, or build the value outside it")]
    PoolStoreEscapes {
        place: String,
        ty: Type,
        pool: String,
        span: Span,
    },

    #[error("'{keyword}' may not leave {pool}; the arena is released at the block's closing brace and jumping past it would skip that release")]
    PoolControlFlowEscapes {
        keyword: String,
        pool: String,
        span: Span,
    },

    #[error("'{type_name}' implements 'Drop' but not 'PoolAware', so a value of it{origin} may not be owned by {pool}; the arena is released in one store and cannot run a destructor per object. Implement 'PoolAware' for '{type_name}', or build the value outside the pool")]
    PoolDropOnlyValue {
        type_name: String,
        /// Where the value came from, rendered into the message: `" returned by 'open'"`
        /// for a call, empty for a literal written in the block itself. The diagnostic
        /// names the constructing function when there is one.
        origin: String,
        pool: String,
        span: Span,
    },
}

impl TypeError {
    /// The source span this error points at.
    ///
    /// Every variant carries one: the driver resolves it against the source file
    /// to render a line, a column and a caret, which is why the `Display` messages
    /// above carry no location of their own.
    pub fn span(&self) -> Span {
        match self {
            Self::Mismatch { span, .. }
            | Self::UndefinedVariable { span, .. }
            | Self::UndefinedFunction { span, .. }
            | Self::FunctionUsedAsValue { span, .. }
            | Self::GenericParamShadowsBuiltin { span, .. }
            | Self::GenericParamNotInferable { span, .. }
            | Self::UnknownArrayLength { span, .. }
            | Self::UndeclaredLifetime { span, .. }
            | Self::ConstParamNotInteger { span, .. }
            | Self::ConstPredicateViolated { span, .. }
            | Self::TurbofishCountMismatch { span, .. }
            | Self::TurbofishKindMismatch { span, .. }
            | Self::GenericStructNeedsArgs { span, .. }
            | Self::GenericEnumNeedsArgs { span, .. }
            | Self::GenericEnumNotInferable { span, .. }
            | Self::GenericArgCountMismatch { span, .. }
            | Self::NotAGenericType { span, .. }
            | Self::NestedGenericTypeArg { span, .. }
            | Self::VariableAlreadyDefined { span, .. }
            | Self::FunctionAlreadyDefined { span, .. }
            | Self::ArgumentCountMismatch { span, .. }
            | Self::InvalidOperator { span, .. }
            | Self::CopyDeriveNonCopyField { span, .. }
            | Self::UnknownDerive { span, .. }
            | Self::UnimplementedDerive { span, .. }
            | Self::DuplicateDerive { span, .. }
            | Self::DeriveConflictsWithImpl { span, .. }
            | Self::DeriveFieldUnsupported { span, .. }
            | Self::DropTypeCannotBeCopy { span, .. }
            | Self::InvalidDropImpl { span, .. }
            | Self::UnknownTrait { span, .. }
            | Self::TraitAlreadyDefined { span, .. }
            | Self::DynTraitNotBehindReference { span, .. }
            | Self::SliceNotBehindReference { span, .. }
            | Self::NonScalarTensorElement { span, .. }
            | Self::TensorShapeRequired { span, .. }
            | Self::TensorExtentMismatch { span, .. }
            | Self::TensorRankMismatch { span, .. }
            | Self::TensorScalarNeedsConstructor { span, .. }
            | Self::TensorTypeNotInferable { span, .. }
            | Self::UnknownTensorConstructor { span, .. }
            | Self::TensorElementNotArithmetic { span, .. }
            | Self::TensorBroadcastMismatch { span, .. }
            | Self::TensorMatMulMismatch { span, .. }
            | Self::TensorIndexRankMismatch { span, .. }
            | Self::TensorIndexOutOfBounds { span, .. }
            | Self::TensorSliceBoundNotConstant { span, .. }
            | Self::TensorSliceOutOfRange { span, .. }
            | Self::TensorIndexOnNonTensor { span, .. }
            | Self::AssignToTensorSlice { span }
            | Self::UnknownTensorDimension { span, .. }
            | Self::TensorShapeParamConflict { span, .. }
            | Self::DuplicateTensorAxisName { span, .. }
            | Self::TensorAxisNameMismatch { span, .. }
            | Self::TensorLiteralSymbolicExtent { span, .. }
            | Self::TensorTransposeRank { span, .. }
            | Self::TensorShapeArgNotLiteral { span, .. }
            | Self::TensorReshapeExtentNotConstant { span, .. }
            | Self::TensorReshapeRepeatedInference { span, .. }
            | Self::TensorReshapeElementCount { span, .. }
            | Self::TensorReshapeIndivisible { span, .. }
            | Self::UnknownTensorAxisName { span, .. }
            | Self::TensorAxisOutOfRange { span, .. }
            | Self::TensorAxisRepeated { span, .. }
            | Self::TensorPermuteRank { span, .. }
            | Self::TensorFlattenNotAdjacent { span, .. }
            | Self::TensorFlattenNoAxes { span, .. }
            | Self::TensorDynamicExtent { span, .. }
            | Self::TensorShapeCastSymbolicExtent { span, .. }
            | Self::TensorReduceElementType { span, .. }
            | Self::TensorReduceMeanNotFloat { span, .. }
            | Self::TensorReduceEmpty { span, .. }
            | Self::TensorSortElementType { span, .. }
            | Self::TensorSortRankZero { span, .. }
            | Self::TensorSortEmpty { span, .. }
            | Self::TensorSortArgNotConstant { span, .. }
            | Self::TensorTopKOutOfRange { span, .. }
            | Self::EinsumSubscriptNotLiteral { span, .. }
            | Self::EinsumMalformedSubscripts { span, .. }
            | Self::EinsumOperandCount { span, .. }
            | Self::EinsumOperandNotTensor { span, .. }
            | Self::EinsumOperandRank { span, .. }
            | Self::EinsumElementMismatch { span, .. }
            | Self::EinsumElementType { span, .. }
            | Self::EinsumExtentConflict { span, .. }
            | Self::EinsumOutputLetterUnbound { span, .. }
            | Self::EinsumOutputLetterRepeated { span, .. }
            | Self::EinsumSymbolicExtent { span, .. }
            | Self::TensorConstructorNotApplicable { span, .. }
            | Self::TraitNotObjectSafe { span, .. }
            | Self::ImplTraitNotAllowedHere { span, .. }
            | Self::ImplReturnNotInferable { span, .. }
            | Self::ImplReturnDoesNotImplement { span, .. }
            | Self::MissingTraitMethod { span, .. }
            | Self::NotATraitMethod { span, .. }
            | Self::TraitMethodSignatureMismatch { span, .. }
            | Self::TraitBoundNotSatisfied { span, .. }
            | Self::InvalidBinaryOperator { span, .. }
            | Self::MissingPartialEqImpl { span, .. }
            | Self::OperatorTraitRequiresCopy { span, .. }
            | Self::AssociatedTypeMismatch { span, .. }
            | Self::UnboundAssociatedType { span, .. }
            | Self::UnknownAssociatedType { span, .. }
            | Self::MissingAssociatedType { span, .. }
            | Self::UnconstrainedAssociatedType { span, .. }
            | Self::AssociatedTypeBoundMismatch { span, .. }
            | Self::MissingSupertraitImpl { span, .. }
            | Self::HalfFloatArithmetic { span, .. }
            | Self::ReturnTypeMismatch { span, .. }
            | Self::MissingReturn { span, .. }
            | Self::UnknownTypeName { span, .. }
            | Self::NotCallable { span, .. }
            | Self::ComposeUndefined { span, .. }
            | Self::ComposeNotANamedFunction { span, .. }
            | Self::ComposeGenericFunction { span, .. }
            | Self::ComposeArity { span, .. }
            | Self::ComposeStageMismatch { span, .. }
            | Self::UninitializedVariable { span, .. }
            | Self::VoidBinding { span, .. }
            | Self::AssignToImmutable { span, .. }
            | Self::IntegerLiteralOutOfRange { span, .. }
            | Self::NegativeLiteralForUnsignedType { span, .. }
            | Self::BreakOutsideLoop { span, .. }
            | Self::ContinueOutsideLoop { span, .. }
            | Self::UndefinedLabel { span, .. }
            | Self::BreakValueInUnitLoop { span, .. }
            | Self::InvalidForRangeType { span, .. }
            | Self::ReservedNameSeparator { span, .. }
            | Self::StructAlreadyDefined { span, .. }
            | Self::UnknownStruct { span, .. }
            | Self::UnknownField { span, .. }
            | Self::PrivateField { span, .. }
            | Self::MissingStructField { span, .. }
            | Self::DuplicateStructField { span, .. }
            | Self::AssignToImmutableField { span, .. }
            | Self::MethodNotFound { span, .. }
            | Self::UnknownPathType { span, .. }
            | Self::UnknownAssociatedFunction { span, .. }
            | Self::ConstAlreadyDefined { span, .. }
            | Self::InvalidConstExpr { span, .. }
            | Self::ComparisonChain { span, .. }
            | Self::NullCoalesceOnNonFallible { span, .. }
            | Self::TryOnNonFallible { span, .. }
            | Self::TryOutsideFallibleFunction { span, .. }
            | Self::ValElseMustDiverge { span, .. }
            | Self::ValElseBindingOnOption { span, .. }
            | Self::UseOfMovedValue { span, .. }
            | Self::MovedInLoopBody { span, .. }
            | Self::CannotMoveOutOfBorrow { span, .. }
            | Self::CannotBorrowValue { span, .. }
            | Self::CannotBorrowMutably { span, .. }
            | Self::CannotDereference { span, .. }
            | Self::CannotAssignThroughRef { span, .. }
            | Self::CannotMutablyBorrowWhileBorrowed { span, .. }
            | Self::CannotBorrowWhileMutablyBorrowed { span, .. }
            | Self::CannotUseWhileMutablyBorrowed { span, .. }
            | Self::CannotMoveWhileBorrowed { span, .. }
            | Self::CannotAssignWhileBorrowed { span, .. }
            | Self::ReturnsReferenceToLocal { span, .. }
            | Self::RangeNotAllowed { span, .. }
            | Self::SliceExpectsRange { span, .. }
            | Self::NotIndexable { span, .. }
            | Self::NotIterable { span, .. }
            | Self::BorrowedIterableHead { span, .. }
            | Self::LoopAdapterNotCallable { span, .. }
            | Self::LoopAdapterInput { span, .. }
            | Self::LoopAdapterOutput { span, .. }
            | Self::CollectionTypeNotInferable { span, .. }
            | Self::InvalidCollectionElement { span, .. }
            | Self::InvalidCollectionKey { span, .. }
            | Self::InvalidHashableImpl { span, .. }
            | Self::IndexNotInteger { span, .. }
            | Self::ArrayLengthMismatch { span, .. }
            | Self::NotATuple { span, .. }
            | Self::TupleIndexOutOfBounds { span, .. }
            | Self::CannotInferEmptyArray { span, .. }
            | Self::ArrayPatternLengthMismatch { span, .. }
            | Self::EnumAlreadyDefined { span, .. }
            | Self::NewtypeAlreadyDefined { span, .. }
            | Self::CyclicNewtype { span, .. }
            | Self::UnsupportedEnumPayload { span, .. }
            | Self::UnknownEnumVariant { span, .. }
            | Self::EnumVariantFormMismatch { span, .. }
            | Self::EnumVariantArityMismatch { span, .. }
            | Self::UnknownEnumField { span, .. }
            | Self::MissingEnumField { span, .. }
            | Self::DuplicateEnumField { span, .. }
            | Self::NonExhaustiveMatch { span, .. }
            | Self::UnsupportedMatchScrutinee { span, .. }
            | Self::PatternTypeMismatch { span, .. }
            | Self::MatchArmTypeMismatch { span, .. }
            | Self::InvalidRangePattern { span, .. }
            | Self::UnimportedVariantPattern { span, .. }
            | Self::VariantPatternFormMismatch { span, .. }
            | Self::OrPatternBinding { span, .. }
            | Self::RefutablePayloadPattern { span, .. }
            | Self::ClosureParamNeedsType { span, .. }
            | Self::ClosureCapturesNonCopy { span, .. }
            | Self::ClosureAssignsCapture { span, .. }
            | Self::ClosureBlockNeedsReturnType { span, .. }
            | Self::UnformattableType { span, .. }
            | Self::UnrenderableStruct { span, .. }
            | Self::FormatSpecMismatch { span, .. }
            | Self::FormatWidthTooLarge { span, .. }
            | Self::FormatPrecisionTooLarge { span, .. }
            | Self::PoolStoreEscapes { span, .. }
            | Self::PoolControlFlowEscapes { span, .. }
            | Self::PoolDropOnlyValue { span, .. } => *span,
        }
    }
}
