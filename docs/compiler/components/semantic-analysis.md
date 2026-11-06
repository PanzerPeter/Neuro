# Semantic Analysis

Semantic analysis type-checks the merged AST and enforces the language's static rules: scopes, mutability, moves and borrows, generics and trait bounds, and the signature rules of the attributes that change how a function compiles (`@grad`, `@gpu`, `@kernel`). It runs after [argument binding](argument-binding.md) and before [HIR lowering](hir-lowering.md), and it reports almost every error a user sees.

**Crate**: `compiler/semantic-analysis`
**Entry point**: `pub fn type_check(items: &[Item]) -> Result<Vec<Warning>, Vec<TypeError>>`

## Overview

The checker validates types but does not annotate the tree. A successful run returns the lint warnings. A failed run returns every error it found, and the warnings are dropped. [HIR lowering](hir-lowering.md) re-derives each expression's type afterwards, so no later stage depends on this slice's `Type`.

## Architecture

- **Dependencies**: `ast-types` (read-only traversal), `shared-types` (`Span`, `FormatSpec`), `thiserror`. `syntax-parsing` is a dev-dependency for the integration tests only.
- **Public API**: `type_check`, and the `Type`, `TypeError`, `Warning` and `WarningCode` types it returns. Everything else is `pub(crate)`.
- **Internals**:
  - [`symbol_table.rs`](../../../compiler/semantic-analysis/src/symbol_table.rs): the scope stack. Each binding records its type, mutability and move state.
  - [`types.rs`](../../../compiler/semantic-analysis/src/types.rs): the semantic `Type`. The enum is the reference for the current set. A monomorphized generic is a distinct nominal type named after its arguments (for example `Pair<i32, f64>`), which never reaches a backend.
  - [`type_checkers/`](../../../compiler/semantic-analysis/src/type_checkers/): the checker, split by subject. `expressions/`, `statements/` and `declarations/` hold the dispatch, and the feature modules (`moves`, `grad`, `backward`, `gpu`, `kernel`, `pools`, and so on) hold the rules for one feature each.

## Behavior

**Fail-slow.** The checker records an error and continues, so one run reports every error in the program.

**Recovery type.** `Unknown` is compatible with every type, so one error does not cascade into a second. Divergent expressions (`panic`, `unreachable`, and an arm that `return`s, `break`s or `continue`s) carry it too. They produce no value, so they must not constrain the arms that do.

**No implicit conversions.** Two types are compatible when their names and shapes match. `as` is the only way to widen or narrow. A literal takes its type from its context, so `val x: i64 = 42` is fine, but an `i32` binding assigned to an `i64` one is a mismatch:

```neuro
val a: i32 = 1
val b: i64 = a         // error: type mismatch: expected i64, found i32
val c: i64 = a as i64  // ok
```

**Inference stops at signatures.** A local binding infers its type from its initializer, and a literal with no context defaults to `i32` or `f64`. Parameter and return types are always written. The parser refuses a parameter without a type.

**Lexical scopes.** Every block opens a scope. An inner binding may shadow an outer one of the same name, and the outer binding is visible again when the block ends.

### Pass order

`check_program`, in [`type_checkers/mod.rs`](../../../compiler/semantic-analysis/src/type_checkers/mod.rs), walks the item list several times. Each pass registers only what the next one needs, so declaration order does not matter. Letters mark a pass slotted between two existing ones when a later requirement arrived.

| Pass | What it does | Why it sits here |
|---|---|---|
| 0z | Reject declared names containing `__` (`check_reserved_names`), and attributes the language does not define (`check_attribute_names`) | `__` is the method-symbol separator, so it is refused before anything mangles with it |
| 0a | Pre-register newtype names | a newtype may appear as a struct field, enum payload, or another newtype's inner type before its own declaration |
| 0 | Pre-register enum names (a generic enum keeps its template under the base name) | an enum may be a struct field type, and a struct may be an enum payload |
| 0b | Pre-register trait names | a struct field may be a `&dyn Trait` of a trait registered later |
| 1 | Register structs (generic ones via `register_generic_struct`), record derive intent, then each hand-written `impl Copy` | type names must resolve in signatures, and every later rule reads `Copy` |
| 1a | Resolve enum variant payloads | a payload may name a struct, so it waits for pass 1 |
| 1c | Resolve and validate newtype inner types | every nominal name is known; cycles are rejected, so predicates that recurse through an inner type terminate |
| 1e | Reject an enum stored inline inside itself | the cycle can pass through struct fields, payloads and newtype inners, all resolved by now |
| 1b | Validate `@derive(Copy)` field by field | runs after 1c, so a newtype field reports its real `Copy`-ness |
| 1d | Register traits, then check object safety of each `dyn Trait` a struct field names | conformance and generic bounds need the trait's method signatures, and so does object safety |
| 2 | Register `impl` method signatures (generic ones via `register_generic_impl`) | uses the struct types from pass 1 |
| 2a | Give generic instances made before their impls those impls | a bound or derive rule must see every conformance, whatever the order |
| 2b | Supertrait check (`check_supertraits`) | all impls are known, so a supertrait's impl may appear after the subtrait's |
| 2c | Reject a type that both derives a trait and implements it by hand | the two are separate implementations, and each consumer would silently pick one |
| 2d | Field rules of `@derive(Debug, Clone, PartialEq, Hashable)` | a field may implement the trait by hand, so this waits for every impl |
| 3 | Register module-level constants | they must be visible in every function body |
| 3b | Register every free function signature, and require `func main() -> i32` at the root | a call resolves regardless of source order, and mutually recursive functions can name each other |
| 3c | `@grad` signature rules (with the form rule of `@no_grad`), then `@gpu` and `@kernel` forms | every signature is resolved, so a generated name can be tested against every declared struct |
| 4 | Check function, method and const bodies | every signature is known, so forward references resolve |
| 5 | Lints (`run_lints`) | they run regardless of type errors, so style guidance always reaches the developer |

### Method names and symbols

Methods live in the same flat table as free functions, keyed `TypeName__methodName`. The backend recovers a method's receiver type by splitting its symbol on `__`, so a declared name may not contain `__` (pass 0z, `ReservedNameSeparator`). That keeps a user method from colliding with a generated symbol.

### `@grad` signatures and `.backward()`

A `@grad` function is held to the rules its derivative needs:

- The return type is a rank-0 `Tensor<f32, []>`.
- Every differentiated parameter is `&mut` over a float element type, with literal extents.
- The only attribute arguments are `wrt:` and `order:`. `order:` must be the literal 1 or 2, and 2 is refused on a method.
- Without `wrt:`, every tensor parameter is differentiated. With it, only the parameters it names, and on a method the tensors reached from `self` through exported fields. That path requires `&mut self`.
- A `@grad` method is an instance method of a non-generic inherent `impl` that borrows its receiver.

These rules live in [`type_checkers/grad.rs`](../../../compiler/semantic-analysis/src/type_checkers/grad.rs). Which constructs a `@grad` body may use is not checked here. The derivative transform in [HIR lowering](hir-lowering.md#grad-functions) owns that rule and reports it with a span.

`.backward()` writes gradients into the `&mut` arguments of the `@grad` call its receiver came from, after that call returns. So a `val` bound to such a call holds those borrows until its `.backward()` releases them. The `.backward()` must be on that binding, in the same block, and only once. `.grad()` and `.hessian()` borrow their receiver, and `.zero_grad()` takes it exclusively. The user-facing rules are in the [autodiff reference](../../language-reference/autodiff.md).

### `@gpu` and `@kernel` signatures

- `@gpu` marks a free function that is not also `@grad`. It takes no argument, or one `fallback:` set to the literal `true` or `false`. Whether the body can become a GPU kernel is decided by the GPU backend at compile time, so `check` can accept a body that `compile` refuses.
- `@kernel` has its own form rules (`KernelForm`). It is a free function with one `threads:` array of one to three positive integer literals whose product is at most 1024, and a `void` return. Its parameters are numbers, `bool`s, `Tensor` inputs, or `KernelOut<Tensor>` outputs. The output handles may only be indexed inside an `unsafe` block. The rules are in [Tensors](../../language-reference/tensors.md#writing-a-kernel-kernel) and [`type_checkers/kernel.rs`](../../../compiler/semantic-analysis/src/type_checkers/kernel.rs).

## Diagnostics

Every error is a `TypeError` variant carrying the span of the offending expression or item. The variants and their messages are in [`errors.rs`](../../../compiler/semantic-analysis/src/errors.rs). Lint warnings are the `WarningCode` variants in [`warnings.rs`](../../../compiler/semantic-analysis/src/warnings.rs). Two messages a user will hit:

```text
error: type mismatch: expected i64, found i32
error: name 'add__one' contains '__', which is reserved for compiler-generated symbols; use a single underscore
```

## Testing

- Unit tests sit beside the code, in [`types.rs`](../../../compiler/semantic-analysis/src/types.rs) and in `type_checkers/tests/`.
- Integration tests are in [`compiler/semantic-analysis/tests/`](../../../compiler/semantic-analysis/tests/), one file per subject. They parse source strings with `syntax_parsing::parse` and call `type_check`.
- End-to-end behavior is covered by the `neurc` suites in [`compiler/neurc/tests/suite/`](../../../compiler/neurc/tests/suite/).

## Source

- [`compiler/semantic-analysis/src/lib.rs`](../../../compiler/semantic-analysis/src/lib.rs)
- [`compiler/semantic-analysis/CONTEXT.md`](../../../compiler/semantic-analysis/CONTEXT.md): the rationale behind each rule the checker enforces

## See Also

- [Argument Binding](argument-binding.md), the stage before this one
- [HIR Lowering](hir-lowering.md), the stage after it
- [Types reference](../../language-reference/types.md)
- [Ownership reference](../../language-reference/ownership.md) and [Generics and traits reference](../../language-reference/generics-and-traits.md)
