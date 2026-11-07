# HIR Lowering

HIR lowering turns the type-checked surface AST into the typed High-Level IR (`neuro-hir`), the backend-agnostic form every backend reads. It gives each expression a resolved type, rewrites the language's sugar into existing HIR nodes, and runs the automatic-differentiation transform. You need it when a change touches what a backend receives.

**Crate**: `compiler/hir-lowering`
**Entry point**: `pub fn lower_program(items: &[Item]) -> Result<HirProgram, LoweringError>`

## Overview

The type checker validates types but does not expose them, so lowering re-derives each expression's type while it walks the AST. `neurc` runs lowering immediately after `semantic_analysis::type_check`, in both `check` and `compile`. The output feeds the [LLVM backend](llvm-backend.md), which emits scalar code, and the [MLIR backend](mlir-backend.md), which computes the tensor operations lowering outlined.

## Architecture

- **Dependencies**: `ast-types` (read-only traversal), `neuro-hir` (the output node set), `shared-types`, `thiserror` (for `LoweringError`). `syntax-parsing` is a dev-dependency only, for tests that build ASTs through the parser.
- **Public API**: `lower_program` and `LoweringError`.
- **No semantic coupling.** Lowering does not import `semantic_analysis::Type`. That import would couple two feature slices, which VSA forbids, so duplicating the checker's tables is the intended cost. A divergence between the two surfaces as a `LoweringError`, which is how the duplication is supposed to fail.
- **Internals**: [`autodiff/`](../../../compiler/hir-lowering/src/autodiff/) (the derivative transform), [`tensor_ops.rs`](../../../compiler/hir-lowering/src/tensor_ops.rs) (outlining of tensor operations), and one module per construct family (`expressions/`, `items/`, `statements/`, `closures.rs`, `iteration.rs`, and so on).

## Behavior

### Lowering assumes well-typedness

Lowering computes types, it does not check them. A shape the checker should have rejected becomes a `LoweringError`, never a panic. The one exception is the derivative transform's refusal, described under [`@grad` functions](#grad-functions).

### Registration and scopes

A registration pass records each struct's fields and derive intent (`Copy`, `Clone`, `Debug`, `PartialEq`, `Hashable`, and a hand-written `impl Copy`), each `impl` method under its mangled `Struct__method` key, the free-function signatures, the trait method order, and the module constants. Bodies then lower under a lexical scope stack and a loop-context stack.

### Contextual typing

- A literal takes its suffix type. Otherwise it takes the expected type when that type fits the literal's family, and otherwise the default `i32` or `f64`.
- A function body's trailing expression is an implicit return, typed against the declared return type. Nested block and `if` / `match` tails take the expected type when there is one.
- A divergent arm (`panic`, `unreachable`) contributes no type. The result type of an `if` or `match` comes from the first arm that is not `void`.
- A trailing `if` with an `else` is a value at every depth. Both tail rules send it through the same `if` lowering.

### Nodes with a deliberately chosen type

HIR has no source form for these, so each carries a fixed type:

- A `loop` used as a value takes the type of its `break v`, or `void`.
- A method name used as a callee (`FieldAccess`) carries the call's result type, since a method is not a value.
- A `Range` carries `void`. It is valid only as a `string.slice` or `string.char_slice` argument, and that lowering reads its bounds directly.
- A `pool` block is `void` and is lowered against no expected type. A `pool` is a statement, not a value.
- The parser's `Paren` grouping node is dropped, because the tree already encodes grouping.

### Desugars

Each desugar produces existing HIR nodes, so no backend learns the construct:

- **`?` propagation** becomes a `match` on the operand's tag. The failure arm returns the failure variant rebuilt against the enclosing function's return type, not the operand's.
- **`??`** becomes a `match` that yields the payload or the fallback.
- **Operator traits** (`Add`, `Sub`, `MatMul`, and so on) become ordinary method calls, so the backend emits a plain `StructName__op` call.
- **Reading through a borrow**: operands and method receivers are wrapped in `Deref` nodes, so a borrowed scalar is loaded through every reference layer.
- **`for` loops** over the iteration protocol, and the `.map` / `.filter` adapters, become loops.

### Monomorphization

The HIR has no generic node. A generic function is never lowered directly. Each call instantiates it with type arguments inferred from its arguments, under a mangled instance name. Instances are queued and drained after the ordinary items, and the backends declare every function before any body, so emission order does not matter.

Instance names carry the `$g` marker, which no declared identifier can contain. Examples from the compiler: `identity$g_i32` for a generic function, and `Option$g_char` for an enum instance. Each type argument is spelled so that two argument lists never produce the same name. A built-in type uses its own spelling, and a name the program chose is prefixed with its length.

### Closures and dynamic dispatch

Each closure lifts to a `HirItem::Closure` named `__closure_N`. Its captures are the free variables that resolve to an enclosing local. A `Copy` value the body does not write is copied, a `move` closure copies or moves each capture, a capture the type checker found the body moving out is moved in, and any other capture is borrowed. That one fact comes from the checker, which records it on the closure literal: a move depends on every consuming position and receiver mode, and the two stages must agree exactly.

Each trait lowers with its methods in declaration order, which is the canonical vtable slot layout. The unsizing coercions `&T` to `&dyn Trait` and `&[T; N]` (or `&Vec<T>`) to `&[T]` are inserted in one place, so every context with an expected type receives them.

### `@grad` functions

A `@grad` function lowers to itself plus two generated items: a `GradsOf_<f>` struct with one field per differentiated parameter, and `__<f>__rev`, which takes the same parameters and returns the loss and that struct. The derivative is built from the function's already-lowered HIR, where every value still carries its tensor shape. The work lives in [`src/autodiff/`](../../../compiler/hir-lowering/src/autodiff/):

- The body is flattened to one operation per binding. A reverse sweep adds each operation's adjoint, and sums contributions for values used more than once.
- Control flow keeps its structure. A branch's reverse pass reruns the taken arm. A loop's reverse pass replays earlier iterations, so the forward pass keeps only an iteration count.
- A call to a user function is inlined into the flattened body. A recursive call cannot be inlined, and is refused.
- A generic `@grad` template is derived once per instance, over concrete shapes.
- `@grad(order: 2)` applies the transform twice. It generates `__<f>__hvp__<param>`, a Hessian-vector product, which the first derivative calls once per unit direction to build the Hessian row by row.
- A `@grad` method is derived the same way. Its derivative `__<m>__rev` is a method of the same `impl`, and its struct is `GradsOf_<Type>__<m>`. The receiver is treated as a constant, except for a `wrt:` path, which is differentiated.

A construct the transform has no rule for is `LoweringError::NotDifferentiable`. It carries the construct's span, and `neurc` renders it as a located diagnostic. The accepted body is described in the [autodiff reference](../../language-reference/autodiff.md).

### `.backward()`

`.backward()` never reaches a backend. A block's `loss.backward()` statement pairs with the `val loss = f(...)` declared earlier in the same block. That declaration becomes a call of `__<f>__rev`, and the statement becomes one slot write per differentiated argument, moving that gradient out of the returned struct. A call with no `.backward()` stays the plain function. `.grad()`, `.hessian()` and `.zero_grad()` lower as tensor builtins.

### Tensor operations

The last step of lowering outlines each tensor operation out of host code (host functions, methods and closures) into a function of its own, named `__tensor_op_N`. Its body is computed by the [MLIR backend](mlir-backend.md), because the LLVM backend has no tensor loops. The outlined function is marked:

- `HirTarget::FollowsOperands` in a program that moves a tensor to a device with `.to(...)`. The backends choose per call where it runs, from where its operands live.
- `HirTarget::HostOperation` in any other program, which has a host body only.

Operands are passed as written, so ownership does not change. A read-only operand is lent. A compound assignment's target is passed as `&mut`. A slice's positions are checked at the call with the host's own message (`tensor index out of bounds`), so a body never reads an unchecked position. A reduction whose accumulator is not a number is not outlined. It becomes the loop it is, over element reads. The source is [`src/tensor_ops.rs`](../../../compiler/hir-lowering/src/tensor_ops.rs).

## Diagnostics

`LoweringError` is an internal error, with one exception. `NotDifferentiable` is the only variant with a user-facing location:

```text
error: cannot differentiate <construct>, reached from `@grad` function '<f>'; ...
```

Any other `LoweringError` reaches the user as `HIR lowering error: <message>`, and means a check the type checker should have made did not happen.

## Testing

- Slice unit tests are in [`src/tests/`](../../../compiler/hir-lowering/src/tests/), including the autodiff tests.
- End-to-end coverage is in [`compiler/neurc/tests/suite/hir_lowering.rs`](../../../compiler/neurc/tests/suite/hir_lowering.rs) and [`autodiff.rs`](../../../compiler/neurc/tests/suite/autodiff.rs).
- The workspace architecture test in [`compiler/neurc/tests/architecture_tests.rs`](../../../compiler/neurc/tests/architecture_tests.rs) enforces the infrastructure-only dependency rule.

## Source

- [`compiler/hir-lowering/src/lib.rs`](../../../compiler/hir-lowering/src/lib.rs)
- [`compiler/hir-lowering/CONTEXT.md`](../../../compiler/hir-lowering/CONTEXT.md)
- [`compiler/infrastructure/neuro-hir/CONTEXT.md`](../../../compiler/infrastructure/neuro-hir/CONTEXT.md), the HIR node set

## See Also

- [Semantic Analysis](semantic-analysis.md), the stage before this one
- [LLVM Backend](llvm-backend.md) and [MLIR Backend](mlir-backend.md), the consumers of the HIR
- [Automatic differentiation](../../language-reference/autodiff.md), the language rules the transform implements
