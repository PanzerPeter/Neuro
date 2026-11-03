# Argument Binding

Argument binding resolves named arguments (`connect("localhost", port: 8080)`) against the callee's parameters and rewrites each call into declaration order. Type checking and every later stage then see an ordinary positional call. Read it when a named call binds to the wrong parameter, or when a call fails with one of the label diagnostics below.

**Crate**: `compiler/argument-binding`
**Entry Point**: `pub fn bind_arguments(items: &mut [Item]) -> Result<(), Vec<ArgumentError>>`

## Overview

A call may name its arguments, and a declaration may require or forbid the names. The pass matches each label to a parameter, permutes the arguments into declaration order and clears the labels. A named call then produces the same IR as the positional call, except for the calls that need the rewrite described under [Evaluation order](#evaluation-order).

The pass also enforces what a declaration asks of its call sites. A parameter written `external internal: T` must be named at every call, and one written `_ internal: T` must never be. This is checked on every call, including calls that name nothing.

The pass runs after module resolution and the prelude, because a call may name a callee declared in any file. It runs before type checking, which would otherwise pair arguments with the wrong parameters.

## Architecture

- **Dependencies**: `ast-types` (the tree it rewrites, and the `ParamLabel` a declaration carries), `shared-types`, `thiserror`. It depends on **no feature slice**.
- **Public API**: `bind_arguments`, and `ArgumentError` with its `span()` accessor.
- **Internals**:
  - [`signatures.rs`](../../../compiler/argument-binding/src/signatures.rs): the signature table. It is built from the whole program and seeded with the compiler's own builtin signatures before the program's declarations are read.
  - [`binding.rs`](../../../compiler/argument-binding/src/binding.rs): turns one call's labels into a permutation and fills the parameters a call leaves out.
  - [`hoisting.rs`](../../../compiler/argument-binding/src/hoisting.rs): decides whether a permutation would reorder observable arguments, and rewrites those calls.
  - [`walk.rs`](../../../compiler/argument-binding/src/walk.rs): the one mutable traversal that reaches every call, with a scope stack of local names.
  - [`errors.rs`](../../../compiler/argument-binding/src/errors.rs): `ArgumentError`.

## Behavior

### Calls that name nothing

A call with no labels is left untouched unless its callee has a required label, which the binder checks for. A program that uses no named arguments cannot fail here. A positional call with the wrong number of arguments is the type checker's error, not this pass's.

### Three ways to name a callee

A bare identifier finds a top-level `func`. A `Type::member` path finds an associated function in `impl Type`. A `receiver.method(...)` is matched on the method name alone, because the receiver's type is not known until type checking.

### Method labels must agree

A method name carries a signature only while every `impl` and `trait` declaring it agrees on the parameter names. When two disagree, a named call to that method is rejected rather than guessed at. Positional calls are unaffected.

### Builtin signatures

A few compiler-known callees have no declaration to read labels from, so their labels are seeded. These are `Tensor::random_normal(mean:, std:)`, `.flatten(dims:)`, the reductions `.sum`, `.mean`, `.max` and `.min` (`axis:`), `.sort` and `.argsort` (`axis:`, `descending:`), and `.topk` (`k:`, `axis:`). Every label is optional except `k:` on `.topk`, so `t.sum()` and `Tensor::random_normal(0.0f32, 0.02f32)` pass through untouched. A left-out optional parameter is filled with its default before the call is checked, so the type checker always sees the full list.

A program's own declaration of the same associated function replaces the seeded entry. A method the program declares under the same name with different parameter names is reported as an ambiguity instead of binding silently to the builtin. Declared functions have no defaults: a named call to one must supply every argument.

### Evaluation order

Permuting the arguments also permutes when they are evaluated. When two arguments that the permutation moves past each other are observable, the call is rewritten into a block. The block binds each argument to a temporary in source order, then calls the callee with the temporaries in declaration order.

A literal, a path, or a value under `paren` or `as` is not observable. A plain variable read is, because another argument's `&mut` borrow can change it. Only these rewritten calls lose the identical-IR property. A call whose permutation moves nothing observable is still permuted in place.

A temporary keeps its parameter's type annotation, so `Vec::new()` keeps its element type. The annotation is dropped where it would not mean the same at the call site: a type parameter of the callee, `Self`, `impl Trait`, `dyn Trait`, a slice, or a function type. The full rules are in [`hoisting.rs`](../../../compiler/argument-binding/src/hoisting.rs).

The rewrite is skipped when the callee is an expression, such as a method on a computed receiver. Hoisting the arguments ahead of such a receiver would invert the receiver's order against them.

### Local names shadow functions

The walk tracks every local a body binds: parameters, `self`, `val`, `mut` and `const`, loop variables, pattern bindings, and closure parameters. A bare-name call whose name a live local holds is a call through a value. No signature is consulted, and a label on it is rejected.

### Nothing else declares parameter names

Closures, the panic builtins, builtin methods outside the seeded set, enum tuple variants and newtype constructors take no labels. A label on one is an error, and a positional call to one passes through. Attribute arguments (`@grad(wrt: [...])`) use the same call-site syntax but a separate grammar, which this pass does not read.

### Guarding the erasure

If the traversal ever missed a call, its labels would survive and its arguments would keep the order they were written. That is a wrong program rather than a failed build. HIR lowering therefore refuses any call whose label list is still non-empty, so a missed call fails the build.

## Diagnostics

The pass reports every failing call in the program, not just the first. The errors print under an `Argument errors found in "<file>":` header, and the command ends with a count: `Error: N argument error(s) found` from `check`, or `Compilation failed: N argument error(s) found` from `compile`.

Each message names the callee and the label at fault. For a method, the callee is the method name.

| Situation | Message |
|---|---|
| A positional argument after a named one | ``in the call to 'connect': a positional argument cannot follow a named one; move it before `host:` `` |
| A label the callee does not declare | `'connect' has no parameter named 'prot'` |
| The same parameter named twice | `argument 'port' is given twice in the call to 'connect'` |
| A required external label omitted | ``the 'min' argument of 'clamp' must be named: write `min: <value>` `` |
| A `_` parameter named at the call site | ``the 'value' parameter of 'scale' is declared `_ value:`, which means it is passed positionally and its name is not written at the call site`` |
| A named call with the wrong count | `'connect' takes 3 argument(s), but 2 given` |
| A callee with no parameter names | `named arguments are not available here: 'len' has no declared parameter names` |
| A method name whose labels disagree across types | ``named arguments cannot be used with 'put': more than one type declares a method of that name with different parameter names, so `amount:` does not identify one parameter`` |

A positional call with the wrong count gets the type checker's `incorrect number of arguments: expected 2, found 1` instead. A required label that is missing shows the rule that caused it, as the `clamp` row does:

```text
Argument errors found in "clamp.nr":
error: the 'min' argument of 'clamp' must be named: write `min: <value>`
 --> clamp.nr:8:24
  |
8 |     val x = clamp(1.5, 0.0, 1.0)
  |                        ^^^

Error: 1 argument error(s) found
```

## Testing

- [`compiler/argument-binding/src/tests.rs`](../../../compiler/argument-binding/src/tests.rs): unit tests for the signature table, the binder and the hoisting rewrite.
- [`compiler/neurc/tests/suite/named_arguments.rs`](../../../compiler/neurc/tests/suite/named_arguments.rs): end to end. It runs programs that use every label form, checks that a reordered call evaluates in source order, and asserts the diagnostic text of the rejected calls it covers.
- [`examples/basics/named_arguments.nr`](../../../examples/basics/named_arguments.nr) is built and run by the examples test.

## Source

- [`compiler/argument-binding/src/lib.rs`](../../../compiler/argument-binding/src/lib.rs)
- [`compiler/argument-binding/CONTEXT.md`](../../../compiler/argument-binding/CONTEXT.md): the rationale behind each rule

## See Also

- [Functions: Named Arguments](../../language-reference/functions.md#named-arguments)
- [Module Resolution](module-resolution.md), the stage before this one
- [Semantic Analysis](semantic-analysis.md), the stage after it
- [Compilation Pipeline](../compilation.md), the full stage order
