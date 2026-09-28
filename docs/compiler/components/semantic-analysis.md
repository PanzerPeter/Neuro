# Semantic Analysis

**Crate**: `compiler/semantic-analysis`
**Entry Point**: `pub fn type_check(items: &[Item]) -> Result<Vec<Warning>, Vec<TypeError>>`

## Overview

Semantic analysis type-checks the merged AST and enforces the language's static rules: scoping,
mutability, ownership and borrows, generics and trait bounds, and the signature rules of
`@grad`. It runs after [argument binding](argument-binding.md) and before
[HIR lowering](hir-lowering.md), and it is the stage that reports almost every error a user
sees.

## Architecture

- **Dependencies**: `ast-types` (the tree it reads), `shared-types` (`Span`, `FormatSpec`),
  `thiserror`. `syntax-parsing` is a dev-dependency for the integration tests only.
- **Public API**: `type_check`, plus the `Type`, `TypeError`, `Warning` and `WarningCode` types
  it returns. Everything else is `pub(crate)`.
- **Internally**: a scope stack of bindings (type, mutability, move state) in
  [`symbol_table.rs`](../../../compiler/semantic-analysis/src/symbol_table.rs), the semantic
  [`Type`](../../../compiler/semantic-analysis/src/types.rs), and the checker itself under
  [`type_checkers/`](../../../compiler/semantic-analysis/src/type_checkers/), one module per
  subject (expressions, declarations, moves, borrows, `@grad`, `.backward()`, lints).

`Type` covers the scalars, `string`, nominal user types (a monomorphized generic is a nominal
type with a mangled name, which keeps generics invisible downstream), arrays, slices, tuples,
references, function types, trait objects, generic and const parameters inside a generic body,
tensors, the built-in collections, and `Unknown`. Read the enum in `types.rs`
for the current set.

## Behavior

**Fail-slow.** The checker records an error and keeps going, so one run reports every error in
the program. `Unknown` is the recovery type: it is compatible with everything, so one error does
not cascade into a second. Divergent expressions (`panic`, `unreachable`, an arm that `return`s,
`break`s or `continue`s) carry it for the same reason: they produce no value, so they must not
constrain the arms that do.

**No implicit conversions.** Two types are compatible when their names and shapes match; `as` is
the only way to widen or narrow. A literal takes its type from the context, so
`val x: i64 = 42` is fine, but an `i32` binding assigned to an `i64` one is a mismatch:

```neuro
val a: i32 = 1
val b: i64 = a         // error: type mismatch: expected i64, found i32
val c: i64 = a as i64  // ok
```

**Inference stops at signatures.** Local bindings infer their type from the initializer, and a
bare literal with no context defaults to `i32` or `f64`. Parameter types and return types are
always written.

**Lexical scopes, with shadowing.** Every block opens a scope, and an inner binding may shadow an
outer one of the same name; the outer binding is visible again once the block ends.

### Pass order

`check_program`, in
[`type_checkers/mod.rs`](../../../compiler/semantic-analysis/src/type_checkers/mod.rs), walks the
item list several times. Each pass registers only what the next one needs, which is what makes
declaration order irrelevant. Passes are lettered where a later requirement was slotted between
two existing ones.

| Pass | What it does | Why it sits here |
|---|---|---|
| 0z | Reject declared names containing `__` (`check_reserved_names`) | `__` is the method-symbol separator, so it must be refused before anything mangles with it |
| 0a | Pre-register newtype *names* (`predeclare_newtype`) | a newtype may appear as a struct field, enum payload, or another newtype's inner before its own declaration |
| 0 | Pre-register enum *names* (`predeclare_enum`), keeping a generic template under its base name for construction-site inference | an enum may be a struct field type, and vice versa |
| 0b | Pre-register trait *names* | a struct field may be a `&dyn Trait` of a trait registered only in pass 1d |
| 1 | Register struct definitions (generic ones via `register_generic_struct`); record derive intent | type names must resolve in method signatures |
| 1a | Resolve enum variant payloads (`resolve_enum_variants`) | a payload may name a struct, so it cannot be resolved until pass 1 has run |
| 1c | Resolve and validate newtype inner types | every nominal name is known by now; rejects cycles, which is what makes every predicate recursing through an inner type terminate |
| 1e | Reject an enum stored inline inside itself (`reject_recursive_enums`) | the cycle can pass through struct fields, payloads and newtype inners, which are all resolved by now |
| 1b | Validate `@derive(Copy)` and the other derives field by field | runs after 1c so a newtype field reports its real `Copy`-ness |
| 1d | Register trait declarations, then check the object safety of every `dyn Trait` a struct field named (`check_deferred_object_safety`) | `impl Trait for T` conformance and generic trait bounds need the trait's method signatures, and so does object safety |
| 2 | Register `impl` method signatures (generic ones via `register_generic_impl`) | uses the struct types from pass 1 |
| 2b | Operator-trait supertrait check (`check_operator_supertraits`) | all impls are registered, so `Comparable: PartialEq` is order-independent |
| 2c | Reject a type that both derives a trait and implements it by hand (`check_derive_impl_conflicts`) | the two would be two different `==`, and dispatch would silently pick the impl |
| 3 | Register module-level constants | they must be visible in every function body |
| 3b | Register every free function's signature (`register_function_signature`) | a call resolves regardless of source order, and mutually recursive functions can name each other |
| 3c | `@grad` signature rules (`check_grad_attributes`) | every signature is resolved, and the generated bundle name can be tested against every declared struct |
| 4 | Check function, method, and const **bodies** | every signature is known, so forward references and mutual recursion resolve |
| 5 | Lints (`run_lints`) | run independently of type errors so style guidance always reaches the developer |

### Method name mangling

Methods live in the same flat function table as free functions, keyed by
`TypeName__methodName`. `__` is reserved as the compiler's symbol separator: the backend
recovers a method's receiver type by splitting its symbol on `__`, so no generated name may
introduce a second `__`. Two consequences:

- Monomorphized instance names use a single-underscore marker, `_g_`, for a generic struct
  instance (`Pair_g_i32_f64`) and for a generic function instance (`identity_g_i32`).
- User-declared identifiers may not contain `__` (pass 0z, `TypeError::ReservedNameSeparator`),
  so a user method can never collide with a generated instance or vtable-thunk symbol.

### `@grad` signatures and `.backward()`

A function marked `@grad` has its signature held to the rules the derivative needs: a rank-0
`Tensor<f32, []>` return, every differentiated parameter borrowed `&mut` with a float element
and literal extents, and no attribute argument but `wrt:` and `order:`. Without `wrt:` every
tensor parameter is differentiated; with it, only the parameters it names and, on a method, the
tensors it reaches from `self` through exported fields and literal array positions, which needs
`&mut self`. A `@grad` method must be an instance method of a non-generic inherent `impl` that
borrows its receiver. These rules live in `type_checkers/grad.rs`. Which constructs a `@grad`
body may use is not checked here: that rule set belongs to the transform in
[HIR lowering](hir-lowering.md).

`.backward()` writes gradients into the `&mut` arguments of the `@grad` call its receiver came
from after that call has returned, so a `val` bound to such a call holds those borrows until its
`.backward()` releases them (`type_checkers/backward.rs`). The `.backward()` must be on that
binding, in the same block, once. `.grad()` and `.hessian()` borrow their receiver, the second
typed `&Tensor<T, S ++ S>`, and `.zero_grad()` takes it exclusively. `order:` must be the literal
1 or 2, and 2 is refused on a method. The user-facing rules are in the
[autodiff reference](../../language-reference/autodiff.md).

## Diagnostics

Every error is a `TypeError` variant carrying the span of the offending expression or item. The
set grows with each feature, so it is not reproduced here: the authoritative list, with each
user-facing message, is [`errors.rs`](../../../compiler/semantic-analysis/src/errors.rs). Lint
warnings (`WarningCode`) are in [`warnings.rs`](../../../compiler/semantic-analysis/src/warnings.rs)
and are dropped when the program has errors.

## Testing

Some modules carry unit tests of their own; the integration tests in
`compiler/semantic-analysis/tests/` are
split by subject (errors, control flow, functions, const generics, integers, strings, lints) and drive
the checker from source strings through `syntax_parsing::parse`. End-to-end behavior is covered
by the `neurc` suites.

## Source

- [`compiler/semantic-analysis/src/lib.rs`](../../../compiler/semantic-analysis/src/lib.rs)
- [`compiler/semantic-analysis/CONTEXT.md`](../../../compiler/semantic-analysis/CONTEXT.md):
  the rationale behind each rule the checker enforces

## See Also

- [Argument Binding](argument-binding.md)
- [HIR Lowering](hir-lowering.md)
- [Types reference](../../language-reference/types.md)
