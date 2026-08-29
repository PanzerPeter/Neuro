# argument-binding

## Purpose
Bind every call site's arguments to the callee's parameters in declaration order, so a named argument is resolved before any pass that reads a call.

## Entry Point
- Type: Library function
- Input: `items: &mut [Item]` — the whole program, every module merged and the prelude prepended
- Output: `Result<(), Vec<ArgumentError>>` — the items are rewritten in place; every call that cannot be bound is reported, not just the first

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none
- Reads and rewrites the AST it is handed; touches no files

## Shared Kernel
- ast-types — the `Item` / `Stmt` / `Expr` tree this slice walks and rewrites, and the `ParamLabel` a declaration carries
- shared-types — `Identifier`, `Span` on the labels it matches and the errors it reports
- thiserror — `ArgumentError` derivation

## Notes
- **A label is surface syntax, and this slice is where it stops.** `Expr::Call` carries
  `arg_labels` beside `args`; this pass permutes `args` into the callee's declaration order
  and empties `arg_labels`. Type checking, HIR lowering, and both backends therefore see the
  positional call they always saw, which is what makes a named argument cost nothing at
  runtime — it produces the same IR as writing the arguments in order.
- **The pass runs on the whole program, after module resolution and before type checking.**
  A call names its callee and the callee may be declared in any file, so the table cannot be
  built until every module is merged; and the permutation must be in place before the type
  checker matches argument types against parameter types, or it would compare the wrong
  pairs.
- **A call that named nothing is left untouched.** The parser emits an empty `arg_labels` for
  it, and the fast path returns immediately unless the callee has a *required* label — an
  `external internal:` parameter is an obligation on every call site, so an all-positional
  call to one is still checked. Everything else keeps the exact node it had, so arity and
  type errors stay the type checker's to report and this pass adds no failure mode to
  programs that do not use the feature.
- **Three ways to name a callee, three tables.** A bare identifier looks up a top-level
  `func`; a `Type::member` path looks up an associated function declared in `impl Type`; a
  `receiver.method(...)` looks up the method *by name alone*, because the receiver's type is
  not known before type checking. The method table therefore records a signature only while
  every `impl` and `trait` declaring that method name agrees on the parameter names; when two
  disagree, a named argument on it is rejected with `AmbiguousMethodLabels` rather than
  guessed at. Positional calls are unaffected, so the limitation costs nothing until someone
  writes a label.
- **Nothing else declares parameter names.** A closure, a builtin (`panic`, `.len()`), an enum
  tuple variant, and a newtype constructor all reach the `Unknown` arm: a label on one is
  `LabelsUnsupported`, and a positional call passes through. Attribute arguments
  (`@grad(wrt: [...])`) follow the same call-site syntax in the spec but are a separate
  grammar (`Attribute.args`) and are not handled here.
- **Local bindings are not tracked**, the same limitation module resolution documents for
  rewriting. A closure named `f` shadowing a top-level `func f` is looked up as the function,
  so a required label on that function would be enforced against the closure call. Only a
  program that shadows a labelled function by name can notice.
- **A missed call would be silent, so it is made loud.** If this walk failed to reach a call,
  its labels would survive and the arguments would stay in written order — the one way a named
  argument could bind to the wrong parameter instead of failing. `hir-lowering` refuses a call
  whose `arg_labels` is non-empty, so anything the walk misses fails the build rather than
  compiling to the wrong program.
