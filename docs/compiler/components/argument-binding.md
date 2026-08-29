# Argument Binding

**Status**: Complete (1H)
**Crate**: `compiler/argument-binding`
**Entry Point**: `pub fn bind_arguments(items: &mut [Item]) -> Result<(), Vec<ArgumentError>>`

## Overview

Argument binding resolves **named arguments** — `connect("localhost", port: 8080)`
— against the callee's parameter list. It permutes each call's arguments into the callee's
declaration order and drops the labels, so semantic analysis, HIR lowering, and both
backends only ever see an ordinary positional call. That is what makes a named argument
free: it produces the same IR as writing the arguments in order.

The pass also enforces the obligations a *declaration* creates. A parameter written
`external internal: T` must be named at every call site; one written `_ internal: T` must
never be. Both are checked here, on every call, including calls that name nothing.

`neurc` runs it after module resolution has merged every file and the prelude has been
prepended — a call names a callee that may be declared anywhere in that list — and before
type checking, which would otherwise pair arguments with the wrong parameters.

## Architecture

- **Dependencies**: `ast-types` (the tree it walks and rewrites, and the `ParamLabel` a
  declaration carries), `shared-types`, `thiserror`. It depends on **no feature slice**.
- **Public API**: `bind_arguments`, `ArgumentError`.
- Internally: a signature table built from the program's declarations, a binder that turns
  one call's labels into a permutation, and one mutable traversal reaching every call
  expression.

## Behavior

**A call that names nothing is left untouched.** The parser emits no label list for it, and
the binder returns immediately unless the callee has a *required* label. Arity and type
errors therefore stay the type checker's to report, and a program that does not use named
arguments cannot fail here.

**Three ways to name a callee, three lookups.** A bare identifier finds a top-level `func`;
a `Type::member` path finds an associated function declared in `impl Type`; a
`receiver.method(...)` is matched on the **method name alone**, because the receiver's type
is not known before type checking.

**Method labels must agree.** The method table records a signature only while every `impl`
and `trait` declaring that name agrees on the parameter names. When two disagree, a named
argument on that method is rejected rather than guessed at; positional calls are
unaffected.

**Nothing else declares parameter names.** Closures, the panic builtins, builtin methods,
enum tuple variants, and newtype constructors all take no labels, so a label on one is an
error. Attribute arguments (`@grad(wrt: [...])`) share the surface syntax in the spec but
are a separate grammar and are not handled here.

**Local bindings are not tracked**, the same limitation module resolution documents. A
closure shadowing a same-named top-level function is looked up as the function.

## Diagnostics

`ArgumentError` names the call and the label at fault: a positional argument after a named
one, an unknown label, a label given twice, a required label omitted, a positional-only
parameter named, an argument count that cannot be permuted, a callee with no declared
parameter names, and a method name whose labels disagree across types. Every failing call
in the program is reported, not just the first.

## Guarding the erasure

If the traversal ever failed to reach a call, its labels would survive and the arguments
would stay in the order they were written — a wrong program rather than a failed build.
HIR lowering therefore refuses any call whose label list is non-empty, which turns a missed
node into a build failure.

## Source

- [`compiler/argument-binding/src/lib.rs`](../../../compiler/argument-binding/src/lib.rs)
- [`compiler/argument-binding/CONTEXT.md`](../../../compiler/argument-binding/CONTEXT.md)

## See Also

- [Functions — Named Arguments](../../language-reference/functions.md#named-arguments)
- [Module Resolution](module-resolution.md)
- [Semantic Analysis](semantic-analysis.md)
