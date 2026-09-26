# HIR Lowering

**Status**: Complete (1D)
**Crate**: `compiler/hir-lowering`
**Entry Point**: `pub fn lower_program(items: &[Item]) -> Result<HirProgram, LoweringError>`

## Overview

The HIR lowering slice turns a type-checked surface AST into the typed High-Level IR
(`neuro-hir`), the backend-agnostic contract every backend lowers from. Its defining job is to
attach a fully resolved type to every expression: the frontend type checker validates types but does
not expose them, so the lowerer **re-derives** each expression's type while walking the AST.

`neurc` runs lowering immediately after `semantic_analysis::type_check` in both `check` and
`compile`. The output feeds the [LLVM backend](llvm-backend.md) (and the
[MLIR backend](mlir-backend.md) under the `mlir` feature).

## Architecture

- **Dependencies**: `ast-types` (read-only AST traversal), `neuro-hir` (output node set),
  `shared-types`, `thiserror` (`LoweringError`). `syntax-parsing` is a **dev-dependency only**
  (tests build ASTs through the parser), never a production cross-slice dependency.
- **Public API**: single `lower_program` entry point + `LoweringError`.
- **No semantic coupling**: lowering re-derives types rather than importing
  `semantic_analysis::Type`, importing it would couple two feature slices, which VSA forbids
  (duplicate over couple).

## Behavior

Lowering **assumes well-typedness**, it computes types, it does not validate them. A shape the
checker should have rejected surfaces as a `LoweringError`, never a panic. The one exception is
the derivative transform's refusal, described under [`@grad` functions](#grad-functions).

A registration pre-pass mirrors the checker's: struct field tables (plus `@derive(Copy/Clone)`
intent), `impl` method signatures under mangled `Struct__method` keys, free-function signatures, and
module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

Two type derivations are contextual, faithfully mirroring the checker:

- **Literals** take a suffix type, else the expected type when it fits the literal's family, else the
  default `i32` / `f64`.
- A **function/method body's trailing expression** is an implicit return, typed against the declared
  return type; nested block / `if`-arm tails are typed with no hint.

Three nodes carry a deliberately-chosen type the source has no first-class form for:

- a `loop` value-expression takes its `break v` type (or `void`);
- a method-name callee (`FieldAccess`) carries the call's result type (there is no method value);
- a `Range` carries `void` (valid only as a `string.slice` / `string.char_slice` argument, whose lowering reads its bounds
  directly).

Divergent panic-family calls (`panic` / `assert` / `unreachable`) adopt their context's expected
type, or `void` in statement position. The AST's `Expr::Paren` grouping node is dropped, tree
structure already encodes grouping.

### `@grad` functions

A function marked `@grad` lowers to itself plus two generated items: a `GradsOf_<f>` struct with
one field per differentiated parameter, and `__<f>__rev`, which takes the same parameters and
returns the loss and that struct. The derivative is built from the function's already-lowered HIR,
where every value still carries its tensor shape. The work lives in `src/autodiff/`: the body is
flattened into one operation per binding, those bindings are emitted again with every tensor
operand borrowed, and a reverse sweep adds each operation's adjoint rule, summing contributions
for values used more than once.

Control flow keeps its structure. An `if` becomes a branch holding one flattened list per arm, a
`while` a loop holding its condition and body, and a binding either of them reassigns becomes a
`mut` binding of the derivative. The reverse pass of a branch runs the taken arm again and sweeps
it. The reverse pass of a loop undoes the iterations last to first, rebuilding each one's values by
replaying the iterations before it, so the forward pass keeps only an iteration count.

A call to a user function is inlined into the flattened body: the callee's lowered body is
flattened at the call, its parameters standing for the arguments, so its operations are
differentiated like the caller's own. That is why derivatives are built after every function,
generic instances included, has been lowered. A recursive call cannot be inlined and is refused.

A call through a function value is inlined the same way once its target is known: a closure
(its captures bound to the values they snapshot), a `>>` composition, or a callee's
function-typed parameter bound to the argument. A `.map` / `.zip` / `.reduce` is unrolled into
one element read and one inlined call per element. A `@grad` function with a function-typed
parameter is never derived on its own: each `.backward()` call records the targets it passes,
and gets a derivative `__<f>__with<N>__rev` for them that takes a closure's captures as extra
arguments. A function value whose target a branch or a loop decides is refused.

A `@grad` method is derived the same way. Its derivative is a method of the same type,
`__<m>__rev`, added to the `impl` that declares it so it takes the receiver as the primal does,
and its struct is `GradsOf_<Type>__<m>`. The receiver is a constant: a field of it is read into
the derivative (a number by value, a tensor as a copy) and never differentiated, except a tensor
its `wrt:` names by path. That one is copied once at the top of the derivative, every read of
it is that copy, and the copy's gradient goes in the struct under a name built from the path.

`.backward()` is lowered here too, and never reaches a backend. A block's `loss.backward()`
statement is paired with the `val loss = f(...)` lowered earlier in the same block: that
declaration becomes a call of `__<f>__rev` (or of the receiver's `__<m>__rev` for a method
call) whose loss is unpacked into `loss`, and the statement
becomes one private slot write per differentiated argument or `wrt:` field of the receiver,
moving that gradient out of the returned struct. The derivative therefore runs where the call ran, and a call with no
`.backward()` stays the plain function. `.grad()`, `.hessian()` and `.zero_grad()` lower as
tensor builtins.

`@grad(order: 2)` applies the same transform twice. The first derivative's body, extended to
return the gradient's dot product with a direction `v`, is itself differentiated, giving a
generated `__<f>__hvp__<param>` that computes the Hessian times `v`. The first derivative calls it
once per element of the parameter, along each unit direction, to build the Hessian row by row, and
its `.backward()` moves the Hessian into its own slot after the gradient.

This is the one place lowering reports a user error with a location. The transform owns its rule
set, so a construct it has no rule for is a `LoweringError::NotDifferentiable` that carries the
construct's span, and `neurc` renders it like a type error. See
[Automatic Differentiation](../../language-reference/autodiff.md) for the accepted body.

## Testing

Slice unit tests cover the lowering rules; `neurc/tests/hir_lowering.rs` provides end-to-end
coverage. The workspace architecture test enforces the slice's infrastructure-only dependencies.

## Resources

- [neuro-hir CONTEXT](../../../compiler/infrastructure/neuro-hir/CONTEXT.md), the HIR node set
- [hir-lowering CONTEXT](../../../compiler/hir-lowering/CONTEXT.md), slice contract
