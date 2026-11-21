# hir-lowering

## Purpose
Lower a type-checked surface AST into the typed High-Level IR (`neuro-hir`), re-deriving every expression's resolved type so each backend consumes HIR instead of the AST.

## Entry Point
- Type: Library function
- Input: `items: &[ast_types::Item]` (a program that already passed
  `semantic_analysis::type_check`)
- Output: `Result<neuro_hir::HirProgram, LoweringError>`

## Shared Kernel
- ast-types: read-only traversal of the surface `Item` / `Stmt` / `Expr` / `Type` nodes
- neuro-hir: the typed HIR node set produced as output
- shared-types: `Span`, `Literal`, `IntSuffix`, `FloatSuffix`, `Identifier` reused in nodes
- thiserror: `LoweringError` derivation

`syntax-parsing` is a `[dev-dependencies]` entry only (tests build ASTs through the parser); it
is never a production cross-slice dependency.

## Notes

### The governing rule
Lowering **re-derives** each expression's type rather than importing the checker's `Type`, which
would couple two feature slices (VSA: duplicate over couple). Several tables here are therefore
deliberate duplicates of the checker's (the collection method surface, the `val-else` binding
table, return-position `impl Trait` resolution), and a divergence between the two surfaces as a
`LoweringError`, which is the point. Lowering assumes well-typedness: a shape the checker should
have rejected is a `LoweringError`, never a panic. The exceptions are user-facing:
`NotDifferentiable`, below, and `InstanceCheck`, a shape rule only an instance can check.

A registration pre-pass mirrors the checker's: struct field tables (plus the five derives'
intent, in `copy_structs` / `clone_structs` / `partial_eq_structs` / `debug_structs` /
`hashable_structs`, and each hand-written `impl Copy`: a plain struct into `copy_structs`, a
generic one's `Copy`-bounded parameters into `generic_copy_bounds`), `impl` method
signatures under mangled `Struct__method` keys, free-function signatures,
trait method order, and module constants. Bodies then lower under a lexical scope stack and a
loop-context stack.

### Declared receivers
A method call reads through every borrow, but an instance holds concrete types only, so at
`T = &S` a `v: T` and an `s: &S` look alike. `items/declared_receivers.rs` tells them apart: an
instance with a parameter bound to a `&T` that has `.clone()` through a `Copy` or `Clone` bound
first lowers its template on a clone of the `Lowerer`, with that parameter as an opaque struct
(`$param$T`). A `.clone()` receiver reaching one records its call span and the borrows above it in
`param_receivers`, and the real pass peels those borrows and returns the place: the copy. The table
holds for one body, because a `Span` has no file id; every instance path sets and clears it
(`lower_mono_instance`, `emit_instance_impls`, `emit_mono_method`). Only `.clone()` is recorded,
since a reference meets no other bound.

### `@grad` and the derivative transform
`autodiff/` lowers `@grad func f` to `f` plus a `GradsOf_f` struct (one owned field per
differentiated parameter, typed as that parameter's tensor; `Wrt` is what the attribute selects:
every `&mut Tensor` parameter without `wrt:`, else the listed names and `self` field paths) and `__f__rev(<f's params>) -> (loss,
GradsOf_f)`, built from `f`'s already-lowered HIR by `derive_reverses`, which `lower_program` calls last,
once every function, generic instances and lifted closures included, is lowered: a `@grad` body
may call any of them. `lower_program` only records the names of the `@grad` functions as it
lowers them. A generic `@grad` template is derived per INSTANCE (recorded from the
monomorphization drain), so each instance `f$g_...` gets its own `GradsOf_f$g_...` and
`__f$g_...__rev` over concrete shapes; an instance nothing calls is never emitted. A `@grad`
METHOD (inherent, non-generic `impl`, borrowed receiver; the checker refuses every other form) is
recorded as `(type, method)` by `lower_program` and derived by `derive_method_reverses` under the
key `Type__method`: `GradsOf_Type__method` joins the items, and `__method__rev` is pushed into the
same `impl` as a `HirMethod` with the primal's `self_param`, so every backend dispatches it as a
method and no HIR node changed. The transform reads the method body as a function whose `self` is
a free variable. The receiver is a constant: a field chain rooted in a struct the tape reaches
(`self`, a parameter, or the caller's value an inlined callee's parameter stands for;
`rebase_place` renames the root) is read into an inactive entry, by value for a number, `bool`
or `char`, and as a `.clone()` for a tensor (`field_copy`), since the primal can only read a
tensor field in place and the tape does not yet bind a borrow of a field (LIM-132). A literal array position
(`self.heads[1]`) is a step of the chain like a field; a computed one is refused. Any other field
is refused. A `wrt:` field path is the exception to "constant": `derive_method_reverses` builds
its place (`field_place`, typed from `Lowerer::structs`), `linearize` copies it once ahead of the
body into an entry whose NAME seeds the activity set while the entry itself stays inactive, so
the sweep leaves its adjoint for the bundle, and `field_copy` hands every read whose rebased place
has that path (`receiver_path`) the same copy. Its bundle field is `self__<step>__...`
(`field_key`), a name no declaration can spell. `x.clone()` of a tensor is `x` itself on the tape
(`cloned_tensor`): tape values are never written after they are made.
The generated items follow every other item. `tape/` flattens the body into one-operation entries over
leaves and marks activity, keeping `if` as a `Branch` (one tape per arm) and `while` as a `Loop`
(condition and body tapes). A call to a user function is INLINED into the tape: the callee's
lowered body is linearized at the call with its parameters bound to the arguments' leaves (in
`params`, apart from `aliases`, so a callee assigning to its parameter is refused as the `@grad`
function's own is; through a `&mut` it would write the caller's value), under empty aliases and
scopes so nothing of the caller leaks in or out. A function value whose target is known is a
`Leaf::Function` (a function or lifted closure by name, each capture bound to the leaf it
snapshots where the closure was written), so aliases, parameters and scoping carry it like any
value; a call through one inlines the target, a closure's captures bound beside its parameters.
`Functions` is the tape's view of every lowered function and closure. A slot of function type is
refused (`RUN_TIME_TARGET`): that is how a target chosen by a branch or a loop is refused. A
`.map` / `.zip` / `.reduce` is unrolled (`traverse`): one literal-position element read and one
inlined call per element, in row-major order, the results rebuilt by a tensor literal; over
`MAX_UNROLLED_ELEMENTS` or a dynamic axis it is refused (`ponytail:` on `traverse`). A plain block
value is linearized in a scope of its own. Recursion (a
callee already being inlined), a method or builtin call and a callee returning nothing are
refused.

The two gradient fences are constants on the tape (`Op::Constant`, never active), so no adjoint
passes them. `.detach()` lowers to `HirExprKind::TensorDetach` (a node of its own so a reshape to
the same shape still carries a gradient); the tape replays it as a detach of a `.clone()` of its
receiver's leaf, since the reverse pass reads the receiver again and a second pass over the
derivative (`order: 2`) must meet the fence rather than a copy it differentiates through. A call
to a `@no_grad` function (`lower_program` collects their lowered names, instances included, and
hands them to `derive_reverses` / `derive_method_reverses`, which put them in `Functions`) is not
inlined: `constant_call` replays the call as written, an owned tensor argument as a `.clone()`
of its leaf, a `&T` argument as a shared borrow, a number as it is. A `&mut` argument, or one
taken by value that is neither, is refused at the argument: the reverse pass reads it again.
A call to a `@gpu` function that is not also `@no_grad` is refused at the call: inlining it would
run its body on the host, which `@gpu` forbids.

`@gpu` sets `HirFunction::target` to `HirTarget::Gpu` (`target_of`), or to `GpuOrHost` for
`@gpu(fallback: true)`, on a concrete function and on every instance of a generic template
alike. A call to a fallback function is refused in a `@grad` body like a bare one. Every function the transform generates is `Host`.
`@kernel(threads: [...])` sets `HirTarget::Kernel { threads }` (`block_shape` pads the literals
to three axes with 1s), on instances as well. Before registration, `lower_program` passes the
items through `kernel_signatures`, which rewrites each kernel's `Tensor` input to `&Tensor` and
`KernelOut<Tensor>` to `&mut Tensor` and records the inputs; `lower_ident_call` wraps a kernel
call's input arguments in `&` (`borrow_kernel_inputs`), so the HIR of a kernel and its calls is
the plain reference form. A call with a temporary input first becomes the checker's block,
binding every argument that is not a place to a `$kernel_arg<i>` (`kernel_call_binding_temporaries`). `lower_function_body` marks the body so that
`thread_id.x` / `block_id.z` lower to a `u64` `HirExprKind::GridPosition` (`expressions/grid.rs`) unless a
local of that name shadows it. `val t = thread_id` (or `= u` for another such local) lowers to an
empty statement and records `t` in `grid_aliases` with its scope depth (`grid_binding`), so `t.x`
is the same `GridPosition`; a later local of that name in that scope, or a deeper one, hides it. A kernel call inside a `@grad` body is refused like a `@gpu` one.
In a kernel body, `out.partition(closure)` on a `&mut Tensor` lowers to
`HirExprKind::KernelPartition` (`lower_kernel_partition`): the closure's body is lowered in place
over `base: u64` and `slice: &mut [T]` locals with the loop stack cleared, and nothing is lifted.
`t.flat(i)` on any tensor is `lower_tensor_flat`'s `TensorIndex` over the row-major strides, axis
`k` reading `(i / stride_k) % extent_k` (no modulo on axis 0, so its bounds check is the whole
tensor's), with `i` bound once to a `__flat_N` local when the rank is two or more. A dynamic axis
supplies its extent as a run-time read borrowed from the receiver (a temporary receiver is bound
first), so the strides and moduli behind it are expressions, not constants.

A `@grad` function with a function-typed parameter is never derived on its own
(`derive_reverses` skips it): only a call site knows the target. `lower_backward` resolves each
function-typed argument (`call_site_target`: a closure literal, a function by name, or a local of
the same block bound to a capture-free one; anything else is `NotDifferentiable` at the argument)
and records a `Specialization` per distinct set of targets in `grad_specializations`, keyed
`<f>__with<N>`. Its derivative `__<f>__with<N>__rev` takes `f`'s parameters plus one
`__ad_capture<i>` per capture, which the call passes by reading the captured binding, and the
tape binds the parameter to the target through `linearize`'s `bound`. A `@grad` method's
function-typed parameter is never bound, so a call through it is refused. Every call site gets its own copy (`ponytail:` on `call`); a reassigned binding is versioned (each assignment rebinds the name
to its new value's leaf), and one that leaves an arm or a loop body goes through a `Slot`, a `mut`
binding of the derivative. `sweep.rs` holds the two passes: `forward` replays the tape with every
tensor operand borrowed, `reverse` sweeps it backwards. A branch's reverse pass re-runs the taken
arm, a loop's undoes its iterations last to first, rebuilding each from the loop's entry by
replaying the ones before it (O(n^2) body evaluations, no stored values: the forward pass keeps
an iteration count only). What a nested sweep sends to an outer value leaves through a running
sum declared before the construct. `mod.rs` seeds the loss with `1.0`; `rules.rs` holds each
operation's adjoint and the per-value accumulation; `emit.rs` binds every emitted expression to a
fresh `__ad_t*` val so no nested owned temporary exists. Two ownership rules make the result
sound: the only consuming nodes emitted are shape casts, on an adjoint nothing else reads (the
reshape in an axis-reduction rule and the inverse cast in a shape-cast rule) or on a fresh copy
(a shape cast's forward replay), and an adjoint handed to two owners (an add passes its own through to
both operands) is copied before either takes it (`Adjoints::owners`). A slot is only ever given a
value its arm owns outright or a copy (`store`), and a nested sweep works on a copy of its seed.

**`@grad(order: 2)`** is the same transform applied twice (`GradParams::hessian`, read off
the attribute by `GradParams::of`). `derive_reverse` splits into `differentiate` (the tape,
forward replay, reverse sweep and materialized gradients, as a `Derivative` whose `Emitter` is
`Clone`) and the bundling around it. For each differentiated parameter `p`,
`hessian_vector_product` extends a CLONE of the first derivative's body with `<gradient_p, v>`
over a direction parameter `__ad_direction: &Tensor` shaped like `p`, and runs `differentiate`
again on that synthetic function (function-typed parameters dropped: the first derivative
already inlined their targets), wrt `p` alone: its gradient is `H v`, since the Hessian is
symmetric. That is `__<key>__hvp__<p>(<params>, <captures>, v) -> Tensor<S>`, one more item.
`assemble_hessian` then emits into `__<key>__rev` a counted `while` over the `n` elements of `p`:
a unit tensor built by one element store, re-described at `S`, one call of the HVP (a borrowed
or numeric parameter passed as it is, an owned tensor as a `.clone()` per call, anything else by
value refused, `repeated_argument`), and an inner `while` copying the row into an `[n, n]`
tensor, re-described at `S ++ S` at the end (`hessian_type`) and bundled as `<p>__hessian`
(`hessian_field`). For the second pass to read the first derivative back, the tape accepts
the one construct the forward replay emits that a user rarely writes: `loop { C; if !t { break
}; B }` is linearized as `while { C; t } { B }` (`guarded_loop`), in any `@grad` body. The
one first-derivative construct the tape cannot read back is the element STORE a run-time element
read's adjoint scatters through, so under `order: 2` such a read is refused at the read
(`run_time_read`). A method's `wrt:` fields are never reached here: the checker refuses
`order: 2` on a method.

**`.backward()`** (`autodiff/backward.rs`) never reaches a backend. The three block loops
(`lower_stmt_list`, `lower_body_stmts`, `lower_block_value_inner`) lower each statement through
`lower_stmt_into`, which pairs a `loss.backward()` statement on a tensor binding with the
`val loss = f(...)` already lowered into the SAME block (the checker guarantees it is there):
the declaration becomes `val __backward_N = __f__rev(<same args>)` plus `val loss =
__backward_N.0`, and the statement becomes one `(<arg>).__set_grad(__backward_N.1.<param>)` per
differentiated argument, plus one `<receiver>.<path>.__set_grad(__backward_N.1.self__...)` per
`wrt:` field path of a method, each gradient write followed under `order: 2` by
`(<arg>).__set_hessian(__backward_N.1.<param>__hessian)` (after it, because the gradient write
empties the Hessian slot), re-evaluating the argument or receiver, which the checker restricted to `&mut name`
or a `&mut` binding and held borrowed until here. So the derivative runs where the call ran and
a call with no `.backward()` stays the primal. A method call pairs the same way: `grad_key`
reads the `Type__method` key off the receiver's struct type, and the call becomes
`<receiver>.__method__rev(<same args>)`. `grad_params` maps each lowered `@grad` name or method
key (concrete from `register_function` / `register_impl_methods`, instances from the
monomorphization call site) to its `GradParams`: the parameter names, the bundle's field
names, its `Wrt` (`Wrt::of` reads the checked attribute) and whether it takes second derivatives. `.grad()` lowers as a builtin returning a
`&Tensor<T, S>` like the receiver, `.hessian()` one returning `&Tensor<T, S ++ S>`, `.zero_grad()` as a unit builtin; `reverse_name` /
`bundle_name` are the one spelling of the generated names.

At a point where the primal's control flow changes, the derivative is the executed path's: the
reverse pass follows the path the forward pass took. That is the language's ruling on a kink, not
an accident of the implementation.

The rule set is closed: float and tensor `+ - * /`, scalar unary `-`, `@` (batched too: each
operand's adjoint is an `einsum` over the result's batch axes, summed down by `unbroadcast`), `.sum()` / `.mean()`
whole or along one axis, tensor literals, element reads (a literal position scatters through one
literal per tensor, a run-time one through a zero tensor and one element store), slices at
literal bounds, the four shape casts (replayed on a copy, since the node consumes its receiver;
the adjoint goes back through the inverse permutation), `einsum` without a repeated letter in an
operand, `as` between integer and float types, elementwise math (`Op::Math`: `exp` and `tanh`
read their own value, `log` and `sqrt` divide, `abs` multiplies by an emitted `Sign`, which is 0
at 0 and sends nothing back itself, and `pow` emits `p * x^(p - 1)` without differentiating the
exponent; half-precision math is refused), constant fills, scalar comparisons, integer arithmetic and logic (`&&` / `||` become branches, keeping the
short circuit), calls to user functions and through function values with a known target
(inlined), `.map` / `.zip` / `.reduce` (unrolled), plain block values; `val` / `mut` bindings, assignment to a local, `if` / `else if` / `else` as a
statement or an expression, an early `return` ending an arm of a top-level `if`, `while`, and
`for` over a range, which `for_range` rewrites into the counted `while` it is (bounds read once;
an inclusive range stops on a flag rather than stepping past its end, and a reversed one counts up
and mirrors the counter onto the binding, as the backend's own loop does). A `.step(n)` head
stops on the same flag and advances only while `counter <= high - n` (strictly less for an
exclusive range), forming `high - n` only once `high` clears the type's minimum plus `n`, so no
test overflows. Its stride must be an integer literal: the replay has no run-time guard to stop a
zero one, so any other stride is refused.
Anything else, inactive or not, is
`LoweringError::NotDifferentiable { function, construct, span }`, a user-facing variant of
this enum: the transform owns its rule set, so it is the one place that can say precisely what
it cannot differentiate, and it says where. `neuroc` renders it like a type error. The signature
rules (loss type, `&mut` tensor parameters) are the checker's, in `semantic-analysis`; the
generated names are duplicated there, and a user name containing `__` is already rejected, so
`__f__rev` cannot clash.

`expressions/` holds the expression work: `mod.rs` the dispatch and the block-value tail rule,
with `calls`, `enums`, `structs`, `matches`, `sequences`, `coercion`, `try_op`, `coalesce`, and
`interpolation` beside it.

### Contextual typing
Two derivations take their type from context and faithfully mirror the checker:
- **Literals** take a suffix type, else the expected type when it fits the literal's family, else
  the default `i32` / `f64`. A string literal takes an expected `&string` (`is_string_view`) and
  is `string` otherwise. Both are the same `{ ptr, i64 }` value, so the backend lowers them alike.
- **A body's trailing expression** is an implicit return, typed against the declared return type.
  Nested block and `if`-arm tails inherit the same way: `lower_if_expr` takes the expected type if
  there is one, else the first arm's type, and `lower_block_value` threads it to the tail: the
  rule `lower_match` already applied to arm bodies.

**A trailing `Stmt::If` carrying an `else` is a value at every depth.** Both tail rules route it
through `lower_if_expr` and emit a `HirStmt::Expr`: `lower_body_stmts` (`items/mod.rs`, the implicit
return) and `lower_block_value_inner` (`expressions/mod.rs`, every nested block). When only the
function body recognised it, a nested tail `if` lowered via
`lower_stmt_block` and its branches' values were discarded, and a generic-enum construction
inside one also lost its payload (a branch that is not a value position never reaches the
`current_return` instance fallback). **The rule lives only here**: the LLVM backend no longer
carries a copy.

**A divergent arm contributes no type.** `lower_if_expr` and `lower_match` take the result type
from the first arm that is not `HirType::Void`. A `panic` / `unreachable` with no context type
lowers to `void`, so taking the first arm unconditionally made the whole `if` or `match` void and
the backend rejected it as "void type cannot be used as a value", but only when the divergent
arm happened to be written first.

### Nodes with a deliberately chosen type
An `Expr::Pool` lowers to `HirType::Void` whatever its body ends with, and its body is lowered
against no expected type: a `pool` is not an expression, so a trailing value has nowhere to go
and must not drag a contextual type into the block.

Three have no first-class source form: a `loop` value-expression takes its `break v` type (or
`void`); a method-name callee `FieldAccess` carries the *call's* result type, since there is no
method value. A `Call` with a `FieldAccess` callee therefore always means a method call, so a call
through a function-valued field (`(k.f)(x)`) binds the field's value to a fresh `__operand_N`
local first and calls through that; a `Range` carries `void`, being built only for a slice
intrinsic's argument (`lower_slice_range`), whose lowering reads its bounds directly. A range
anywhere else is a `Range` value (below).

Divergent panic-family calls (`panic` / `assert` / `unreachable`) adopt their context's expected
type, or `void` in statement position. The standard-output builtins (`print` / `println`,
`IO_BUILTINS` in `expressions/mod.rs`) are recognized in the same `lower_ident_call` fallback but
do **not** take their type from context: they return, so the call is always `HirType::Void` with
one `HirType::String` parameter. Both arms sit at the end of `lower_ident_call`, after every
declared function, generic template, and local binding has been tried, so a user function of the
same name still wins.

An `Expr::Loop` with no `break` targeting it lowers to the **expected** type rather than unit
(`LoopCtx.has_break`, set by `record_break_target`), so the dead exit-block result slot is still
typed for the position the loop sits in.

`LoopCtx` also carries the loop's own expected type, and a `break v` lowers `v` against it
(`break_target_expected`, resolved by label). The checker threads the same expectation, so a
tensor literal in a `break` is typed as a tensor in both slices instead of as a plain array.

### Desugars this slice owns
Each produces existing HIR nodes, so no backend learns the construct exists.

- **Reading through a borrow** (`expressions/coercion.rs`, `auto_deref`). Each operand of a
  built-in binary or unary operator, and every method receiver, is wrapped in `Deref` nodes
  mirroring the checker's `Type::auto_deref`: a borrowed scalar is loaded through every layer,
  any other borrow is peeled to one `&`. Applied before the literal re-typing, so `2 * r` with
  `r: &i64` types its literal `i64`, and before operator-trait dispatch, which reads the
  referent anyway.
- **`?` error propagation** (`expressions/try_op.rs`). `operand?` becomes a
  `HirExprKind::Match`: arm 0 tests the `Some`/`Ok` tag and yields payload slot 0 as `__try_N`
  (off `try_counter`); arm 1 is a `Wildcard` whose body is a `Block` holding a single
  `HirStmt::Return` of the failure variant. No backend change is needed: the arm terminates, and
  `codegen_arm_body` already skips the result-slot store for a terminated block. The failure value
  is rebuilt against `current_return`'s instance, **not** the operand's: a `Result<u8, E>`
  propagating out of a `-> Result<i32, E>` function must produce the latter. The `Err` payload is
  bound out of the operand's slot 0 and passed straight through, which is the "no implicit
  conversion" rule made structural.
- **`??` null coalescing** (`expressions/coalesce.rs`). `lhs ?? fallback` becomes a `Match`: arm 0
  tests the `Some`/`Ok` tag and binds payload slot 0 as `__coalesce_N` (off `coalesce_counter`, so
  nested coalesces cannot shadow each other); arm 1 is a `Wildcard` whose body is the fallback.
  The fallback's laziness comes free from the per-arm basic-block chain codegen already emits, and
  it lowers with the payload type as its expected type, which is what types a bare literal.
  Mirroring the checker, a payload that borrows a `Copy` type beside a non-`Reference` fallback
  lowers the fallback against the referent; when it comes out that type, arm 0's body is a
  `Deref` of the binding and the `Match` is typed as the referent. A `()` payload beside a
  fallback that returns nothing yields no value: arm 0's body is an empty block and the `Match`
  is `Void`.
  `binary_result_type`'s `NullCoalesce` arm is unreachable by construction and says so.
  `success_variant`, `not_fallible`, and `fallible_base` are shared with the `?` desugar.
- **Operator traits** (`operator_traits.rs` holds the table: `Add`, `Sub`, …, `MatMul`,
  `PartialEq`, `Comparable`, the `*Assign` family). An operator-trait impl populates `operator_binary_impls` / `operator_unary_impls`
  / `operator_assign_impls` during `register_impl`, and again per instance in
  `register_instance_methods`; a `Binary` / `Unary` whose peeled left/operand type is a struct, enum or newtype with a
  matching entry becomes the method call `a.op(b)`, a `Call` with a `FieldAccess` callee,
  identical to an ordinary method call, so the backend needs no operator awareness. A comparison
  method's `rhs: &Rhs` parameter means a bare argument is wrapped in a `Reference`, and a borrow
  needs storage, so when the right operand is not a variable `build_operator_call` binds both
  operands to fresh `__operand_N` locals in a `Block` first (left then right, keeping source
  order): `p == P { x: 1 }` otherwise reached the backend as a borrow of a temporary. A place
  operand binds a borrow, never its value: a local holding `v[i]` would be a second owner of what
  the element owns. An operand already of reference type passes through untouched. A `place OP=
  rhs` with a `*Assign` impl becomes `build_operator_assign`'s `Block`: the value bound first,
  then the method call on the place. Owned `self`
  methods are lowered like any other receiver, on a generic impl as well as a concrete one; the
  ownership difference is the backend's, not this stage's.
- **Derived equality** (`@derive(PartialEq)`). A struct in `partial_eq_structs` has no `eq` to
  dispatch to, so `==` / `!=` on one stays an `HirExprKind::Binary` typed `Bool` and the backend
  expands it over the fields. Handled in the `Binary` arm *before* `binary_result_type`, which
  admits no aggregate operand: that is the whole difference from the operator-trait route above.
- **The derivable traits' supplied methods** (`derivable.rs`). A receiver with no method of its
  own by that name, which the checker admitted because it implements the trait without an impl:
  `.clone()` on a `Copy` value is the value, read through a borrowed receiver (`Deref`);
  `.hash()` stays a `hash` call for the backend to answer with the map's own key hash;
  `.fmt_debug(out)` becomes `out.push_str("{x:?}")`. `lower_supplied_trait_method` runs before the
  impl dispatch in `lower_method_call`, so a generic body bounded by the trait instantiates the
  same way over a builtin, a derive and an impl. The other direction is
  `debug_hole_through_impl`: a `{x:?}` hole on a struct, enum or newtype with a hand-written
  `Debug` (`trait_impls` plus its own `fmt_debug`) calls `Type__fmt_debug$string`, a helper
  emitted once per type into `mono_items` (`derive_helpers`), which builds a
  `StringBuilder`, calls `fmt_debug` into it and returns its text; the hole then prints that
  `string` under the default spec, since `:?` would quote it. `$` cannot appear in a source name,
  and every return path of the helper allocates, so the hole owns the result. A `@derive(Debug)`
  struct rendered by a hole or a supplied `fmt_debug` gets the same helper for every field, at any
  depth through derived structs, whose `Debug` is written by hand (`emit_field_debug_helpers`):
  the backend's field-by-field rendering calls it. A newtype field is read through to its inner
  type only when the newtype has no `Debug` of its own. The rendering is headed by
  `HirStruct.written_name`, which `written_name` strips of the module path module resolution put
  on the canonical name, so `geometry::Point` prints as `Point` the way it was declared. `.clone()` on a non-`Copy` struct deriving
  `Clone` with a part whose clone runs code (`clone_is_a_call`: a `string`, a tensor, an
  `impl Clone` and not an inherent `clone`, a nested such struct, or a tuple, array or newtype of
  one) calls `S__clone$derived`, the impl a programmer would have written,
  `S { h: self.h.clone(), n: self.n }`, emitted once per type by `emit_derived_clone` as an
  inherent `&self` method (`derive_helpers`). `clone_part` builds each field's value, rebuilding a
  tuple, array or newtype part by part. The call lends it the receiver exactly as it lends
  one to a hand-written `clone`, so a borrowed, indexed or temporary receiver is never taken
  over. Every other derived clone, a `Copy` struct's included, stays the backend's copy of the
  whole value.
- **A `string` as a trait object.** A `&string` is a fat pointer and a trait object's data pointer
  is thin, so `lower_expr` admits the coercion only for `&place` with the place holding a `string`
  (`borrows_string_place`). The backend lends the place's address.
- **`val-else`** (`val_else.rs`). `lower_val_else` reuses `lower_pattern` from
  `expressions/matches.rs` for the success test and bindings, lowers the `else` branch in a pushed
  scope with its own binding, and only then defines the pattern's bindings in the **enclosing**
  scope so later statements type against them. `resolve_else_binding` mirrors the checker's
  binding table off `enum_instance_base`: a `Result`'s `Err` payload (slot 0), `None` for
  `Option`, else the whole scrutinee. An annotation (`val PATTERN: TYPE = ...`) is resolved and
  handed to the scrutinee as its expected type.
- **The `for`-loop iteration protocol** (`iteration.rs`). A `Stmt::ForEach` whose head is a
  nominal type carrying an `IntoIterator` or `Iterator` impl (`trait_impls`, this slice's own
  copy of the checker's table) becomes a `HirStmt::Expr` wrapping a `Block`: a mutable
  `__iter_N` binding initialized from `head.into_iter()` (or from the head itself, when it is
  already an `Iterator` or is `&mut it`, which `next` then drives through the borrow) followed by a `while true` whose single statement is a two-arm
  `Match` on `__iter_N.next()`. Arm 0 tests the `Some` tag and binds payload slot 0 to the loop
  variable; arm 1 is a `Wildcard` whose body breaks, exactly as the `?` desugar's failure arm
  terminates its block. The label rides on the emitted `while`, so `break`/`continue`, labeled
  or not, resolve against it with no backend change. A `string` temporary the iterator borrows
  (`(a + b).chars()`, the receiver of `(a + b).slice(..)`) is hoisted into an `__iter_src_N`
  binding declared ahead of `__iter_N` (`hoist_borrowed_temporaries`): the backend releases a
  temporary at the end of its statement, which for `__iter_N`'s declaration is before the loop.
  Two details are load-bearing. The `next` receiver carries the ITERATOR's type, not the call's
  result: the backend recovers the method symbol from the receiver, so typing it as the result
  sends `next` looking for a builtin on `Option`. And the element type is read out of the
  `Some` payload of `next`'s return (`success_variant`) rather than off the impl's `type Item`
  binding, so the binding and the storage the backend decodes cannot drift apart. An enumerated
  head gets a `__iter_pos_N` cursor declared beside the iterator and advanced at the TOP of the
  yielding arm, so a `continue` cannot skip the advance and repeat an index.
  The built-in sequence heads never reach this path: a range, an array, a `Vec`, and a borrowed
  slice keep their `HirStmt::ForRange` / `ForEach` counted-loop nodes, which is what leaves
  their generated code unchanged. A borrowed counted head defines its element binding as
  `&T` / `&mut T` of the head's own mutability (`borrowed_element`), adapters included; the
  backend reads the same rule off the head's type.
  A map head stays a counted `ForEach` as well, binding the entry by the checker's rule
  (`map_loop_item`). For `m.keys()` / `m.values()` over a map (`map_view_receiver`) the
  iterable is the map itself and `HirStmt::ForEach::view` names the half bound, which an
  adapter chain carries through (`lower_adapted_for_each` takes the view).
  A `LoopPosition` picks where the position binding's value comes from. `Step` is the counter
  above; `ByteOffset` is what a `text.char_indices()` head takes (`char_indices_receiver`
  recognises it on the AST iterable, ahead of lowering it): the loop samples the iterator's own
  `offset` field into `__iter_pos_N` as the FIRST statement of the `while` body, because `next`
  advances that field past the code point it returns and a sample taken afterwards would name
  the following one. Nothing increments it: the iterator owns it.
- **The `.map(f)` / `.filter(p)` desugar** (`loop_adapters.rs`). A head's `adapters` chain is
  folded into the loop it decorates rather than materialized as an iterator value, so all four
  head shapes and the protocol path share one implementation. `plan_loop_adapters` binds each
  adapter function ONCE, ahead of the loop (`__adapt_fn_N_k`), and the loop binds
  `__adapt_elem_N` instead of the user's name; `apply_loop_adapters` then opens the body with the
  chain (a filter as `if !p(cur) { continue }`, a map as a `__adapt_v_N_k` binding) and closes
  it with the user's binding over whatever the last adapter produced. The whole thing is wrapped
  in a `Block`, which is what scopes the function bindings to the loop.
  An enumerated adapted head takes its position from a `__adapt_pos_N` cursor rather than the
  counted loop's `index`: that index counts SOURCE steps, so a filtered chain would leave gaps in
  it. The cursor is read and advanced after the filters and before the user's statements, for the
  same reason the protocol path's `__iter_pos_N` is.
- **Adapter values** (`iterator_adapters.rs`). Outside a `for` head, `lower_method_call` hands
  `.map(f)` / `.filter(p)` / `.enumerate()` on a struct that implements `Iterator` and declares
  no method of the name to `lower_iterator_adapter`, which builds a `StructLiteral` of the
  prelude's `Map` / `Filter` / `Enumerate` instance (`instantiate_generic_struct` at the
  receiver, its `protocol_item_type` and the function's result) holding the receiver and the
  function, lowered through `lower_function_arg` with the element as its hint. No backend sees
  an adapter: the instance's own `impl Iterator` drives it through the protocol path above.
- **Range values** (`range_values.rs`). An `Expr::Range` reaching `lower_expr` becomes a
  `StructLiteral` of the prelude's `Range` instance at the bounds' type (`lower_range_bounds`,
  shared with `ForRange`; a `Range<T>` expectation types an unsuffixed literal), with a stride of
  one and every flag clear. `.rev()` / `.step(n)` are ordinary prelude methods. The walk's two
  prelude-private intrinsics lower per instance in `lower_method_call`, ahead of the impl lookup:
  `x.__to_u64()` to a `Cast` to `u64`, `x.__offset(k)` to `(x as u64).wrapping_add(k)` cast back
  to `x`'s type. No backend learns ranges exist.
- **Container iterator values** (`container_iterators.rs`). `lower_method_call` hands `.iter()` on
  an array, slice or `Vec` and `.keys()` / `.values()` on a map to `lower_container_iterator`,
  ahead of every other builtin: a `StructLiteral` of the prelude's `Iter` (the receiver borrowed
  shared and unsized to `&[T]` with `SliceCoerce`) or map view (the map borrowed shared), with its
  cursor at zero. The view's walk calls the map intrinsics `__slot_count`, `__slot_live`,
  `__key_at` and `__value_at`, which `lower_collection_method` types like any collection method.
- **Traits.** The parser has already injected each trait's default methods into the matching
  `impl Trait for Type` blocks, so trait impls and their inherited defaults lower through the
  ordinary inherent-impl path, and a trait-bounded generic monomorphizes to concrete dispatch with
  no trait awareness needed.

### Monomorphization
The HIR has no generic node, so every template is erased into concrete instances here.

A generic `FunctionDef` goes to `generic_templates` (not `functions`) and is never lowered
directly. `lower_generic_call` infers its type arguments by unifying the template's parameter
annotations against the lowered arguments' types (`unify_ast_hir`), resolves the concrete
signature under a `type_subst` map (consulted by `resolve_type` for a parameter name), mangles a
per-instance name, enqueues the instance if unseen, and emits a `Call` to the mangled name. A
worklist drains after the ordinary items. The backend pre-declares all functions, so emission
order is irrelevant. `unify_ast_hir` also binds through an instance: `Option<U>` against
`Opt$g_i32` reads the instance's arguments (`instance_args`, from `struct_instances` /
`enum_instance_args`) and unifies them pairwise, and through a function type position by
position. An associated function of a generic type (`Cell::new(42)`, `generic_assoc_fn`) infers
the type's instance the same way, plus any turbofish, and calls that instance's function
(`lower_generic_assoc_call`). `lower_generic_args`, shared by all three, lowers a closure literal
for a function-typed parameter after the other arguments, as the checker types it, with a hint of
what they bound (`generic_closure_hint`; `mentions_any` leaves a position naming an unbound
parameter open), and keeps the arguments in their own order. An argument is lowered against its
parameter only when that is a scalar or `&string`, written as one or through a type parameter
already bound, so a literal there takes it, as the checker types it.

Every function, struct and enum instance records its `Chain`: a depth one more than the body that
requested it, and the origin, the call in ordinary code the chain of requests started from
(`mono_chain`, set as each worklist item is emitted; `next_link` makes the next one). Past
`MAX_INSTANCE_DEPTH` the request is `InstantiationTooDeep`.

**Per-instance shape checks.** The checker types a shape-generic reshape, flatten or
reduction symbolically and leaves the counts to the instance, where they are first concrete. So
`.reshape` (`tensor_shape.rs`) refuses a count that changes, a `-1` that does not divide or an
entry with no value; a reduction (`tensor_reduce.rs`) refuses an empty run; and an extent
expression in a type (`resolve_tensor_dim`, `extent_value`) refuses one with no value at the
instance's extents. Each is `instance_check(check, span)`, a `LoweringError::InstanceCheck`
with no origin yet: the drain loop (`in_instance`) gives it the instance's chain origin and a
description (`describe_instance`) as it leaves the instance, and `lower_generic_call` does the same
for a signature resolved at the request. The drain holds each one back (`defer_instance_error`),
leaving that instance out, and goes on, so every failing instance is reported in one run: one
failure is returned as itself, several as `LoweringError::InstanceChecks` in source order. Any
other error after a held one ends the drain with the held ones, the later error most likely
being their consequence. Within one instance the first failing check ends its body. A generic struct instance is requested by a type, so
its methods' checks carry no origin and are reported at the operation. A `where` predicate the
checker refuses first means lowering never runs, which is how a predicate moves the error to the
signature. A generic that calls itself with a type built from its own parameter
(`grow(Option::Some(x))` inside `grow<T>`) needs a new instance per level, which the checker, seeing
one template, cannot tell; without the limit lowering would never finish.

Two rules read the instance's type arguments. A tensor over a type parameter the checker saw with no
bound, built inside a body whose signature does not name it, gets its element from the instance:
`resolve_type` refuses one that is not a scalar as an `InstanceCheck` at the element's span, so it
is reported at the call that made the instance, as a shape failure is. And `T::f(args)`
names the instance's type argument (`bound_assoc.rs`, `lower_bound_assoc_call`): a struct, enum or
newtype argument re-dispatches the call as `Arg::f(args)`, and a scalar takes the numeric
families' functions directly, `zero()` and `one()` as literals and `from_f64(x)` as a cast.
`instance_meets_impl_bounds` answers `Numeric`, `Integer` and `Float` for the scalars, so an
`impl<T: Numeric>` block is emitted for its instances.

Generic structs and impls work the same way, through `generic_structs` / `generic_impls` and
`instantiate_generic_struct(base, args)`: called from `resolve_type` for a `Type::Generic`
annotation and from `lower_generic_struct_literal` after inferring the arguments from field
values, each lowered against the matching field of the instance the expected type names when it
names one (mirroring the checker, so the literal's type and layout agree), or else against its
declared type when that names no parameter, so `count: 3` fills a `u64` field as a `u64`. A sized
array with a concrete element (`seen: [f32; CAP]`) types an array literal at the literal's own
length. Once the instance exists each field value takes the unsizing its concrete field calls for
(`apply_unsizing_coercion`), so `&a` into an `&[A]` field is a slice. Each instance registers concrete fields plus impl-method signatures and emits one
`HirItem::Struct` plus one `HirItem::Impl` per generic impl whose bounds the instance meets
(`instance_meets_impl_bounds`: a declared trait's impl, or for the five derivable traits the
checker's builtin rule, `implements_derivable`), with method bodies lowered under the
impl's `type_subst` and `self` bound to the instance. An instance is `Copy` when its template's
`Copy` impl has its bounds met and every field is `Copy` (`struct_is_copy`), the checker's
rule, so a closure copies exactly the captures the checker copied. Because these are ordinary struct/impl HIR
items, the backend needs no generic awareness. A generic enum's impls follow the same path from
`instantiate_generic_enum` and `emit_mono_enum` (shared `emit_instance_impls`); an instance made
before the impls were recorded (a struct field or enum payload naming `Wrap<i32>`, resolved in
the declaration pass) is caught up by `register_existing_instance_methods`, struct and enum alike,
so its methods and trait conformances match an instance made later. Every
`HirImpl` carries its receiver as `self_type` (`impl_target_type`: an enum when the name is one,
a `HirType::Newtype` with its resolved inner type when it names a newtype, a struct otherwise).
A method call on a newtype receiver resolves through `impl_methods` as an enum's does.

**The mangling scheme is load-bearing.** `mangle_instance` (`name$g_<type…>`) and
`mangle_struct_instance` (`Base$g_<type…>`) use a `$g` marker. It avoids `__`, because the
backend recovers a method's receiver struct by splitting the method symbol on `__` and taking
the first segment, and semantic analysis rejects a user name containing `__`, so the receiver
segment never holds one. It holds `$`, which no identifier can, so no declared function or type
shares an instance's name (with `_g`, a function named `size_g_i32` took the symbol of
`size<i32>`). A generic method's instance name is `method$g_<type…>`. `mangle_type` makes each
type token self-delimiting so the concatenation is injective: a declared type's name is prefixed
with its length (`5Inner`), a trait object's too (`dyn4Show`), and a function type spells its
arity, parameters and result (`fn1_i32_bool`). Without it the struct `tup2_i32_i32` and the
tuple `(i32, i32)` shared a token, as did `Pair<A_B, C>` and `Pair<A, B_C>`, and every function
type. No token ends in `_`: a rank-0 tensor is `tensor_f32_r0` and the unit tuple `tup0`, since a
trailing `_` before a method's `__` makes a `___` the backend splits one character early.

**Generic methods** (a method's own `<U>`) are templates too. `register_impl_methods` (a plain
impl) and `register_instance_methods` (each generic-impl instance) store them in
`generic_methods`, keyed `Type__method`, with the substitution in force there (the impl's
parameters, its `Self::Assoc` bindings, its const types) and register no signature.
`instantiate_generic_method` infers the method's own parameters from the turbofish, then by
`unify_ast_hir` against the lowered arguments, registers `Type__method__g_<args>` in
`functions` and queues a `MonoMethod` once; `emit_mono_method` lowers the body under the merged
substitution and emits it as a one-method `HirImpl` of the receiver's type, so the backend
lowers it, receiver mode and symbol included, as a written method. A trait's generic method needs
nothing of its own: a call through a bound re-lowers on the concrete receiver, and it takes no
`traits` entry and no `HirTrait.methods` slot, both of which serve `dyn`. The three call shapes all
route there: `lower_method_call` (receiver), the plain `Type::f` arm of `lower_call` (ahead of
the enum-variant arm, since a template registers no `Type__f`), and `lower_generic_assoc_call`,
where the turbofish lists the type's parameters first and the function's own after them.
The instance's method name is the one generated name with a second `__`: the backend takes the
receiver from the symbol's first segment only, and no user method name contains `__`, so
`pick__g_i64` collides with nothing.

`instantiate_generic_impls` also records each instance's `trait_impls` entry, mirroring the
checker: without it a generic iterator adapter would satisfy `Iterator` under its base name
only, and a `for` head over an instance would find no protocol on the type it actually has.

Size and const generics ride the same machinery: a `const_subst` (name → value) and `const_types`
(name → int type, `u64` for an extent) are active while an instance body lowers, parallel to `type_subst`. `MonoArg`
(Type | Const) is the positional instance-argument kind and `split_mono_args` builds the two maps;
`unify_ast_hir` binds a value param from an array-length position and from each axis of a
tensor-argument's shape, `resolve_array_size` resolves
`[T; N]` to a concrete length, `resolve_tensor_dim` resolves a `Tensor<T, [M, K]>` axis's extent to one (a dimension
name is checked by semantic analysis and dropped here), a const-param reference lowers to a typed integer literal, and
mangles include const values (`_cN`). Turbofish `type_args` seed the substitution before
inference (`seed_turbofish`, which also reads a bare name in `const_subst` as the caller's extent
passed on). A shape parameter is bound in `type_subst` as a `shape_carrier` (`types.rs`: a
tensor of `void` element), so it rides the type-argument paths, mangling included;
`unify_ast_hir` binds it to the axes around a template's splice, `resolve_shape` expands every
`..S` where a tensor type resolves, `GenericArg::Shape` (and a one-name `[N]` the parser read as
a slice) resolves to a carrier, and `S.rank()` / `S.count()` lower to `u64` literals
(`shape_param_method`). No carrier reaches the HIR. Backends are unaffected: every instance reaching the HIR has concrete `usize` array
lengths.

**Generic enums** (`Option<T>` / `Result<T, E>`) use `generic_enums` (base → template),
`enum_instance_base` / `enum_instance_args` (instance → base + arguments), and the
`mono_enum_pending` worklist, drained ahead of the struct worklist. A generic `Item::Enum` is
registered as a template and never lowered directly; `instantiate_generic_enum` resolves its
variants under the argument substitution and emits one ordinary `HirItem::Enum` per instance
(named by `mangle_struct_instance`, e.g. `Opt$g_i32`), so backends stay generic-unaware.
Construction mirrors the checker: the instance comes from the expected type, else the payload is
unified against the template, else any remaining parameter is taken from the enclosing return
instance. To make that fallback available, `lower_body` tracks the declared return type in
`current_return`, which also gives a `return` operand its contextual type. An enum pattern written
with a generic base resolves against the scrutinee's instance (`pattern_enum_name`) for both its
tag test and its payload bindings.

### Closures
`closures.rs` lifts each `Expr::Closure` to a `HirItem::Closure` named `__closure_N` (the `__`
prefix is a reserved generated-symbol marker the checker forbids in user names), collected in
`closure_items` and appended after the monomorphization worklists. Captures are the body's free
variables that resolve to an enclosing **local** binding (`lookup_local` excludes module
constants), in first-seen order. `collect_captures` settles each as the checker does: a `Copy`
value the body does not write (`FreeVars::writes`: an assignment root, a `&mut` operand, a call of
a method whose name some `&mut self` method has) is copied, a capture `ClosureCapture::by_value`
answers for (every one under `move`, plus each the checker recorded: a move out, or an `Fn` value
only read) is copied or moved in, and any other is borrowed (`HirCapture::borrowed`, typed `&mut T`).
Those two are the capture facts not re-derived here: a move takes every consuming position and
receiver mode, HIR has no call mode, and the two slices must agree exactly, so they ride on the literal. The captures sit in a
scope of their own outside the parameters', so a body binding hides a capture instead of
overwriting it, and `borrowed_captures` marks the borrowed ones by scope index. Inside the body
`binding_value` reads one as `*name`, `lower_place` writes it through `HirPlace::Deref`, and
`borrow_of_capture` hands `&name` / `&mut name` the reference itself. A capture an enclosing
closure borrowed is already that reference, so a nested closure copies it.

`lower_closure` takes a `ClosureHint`: what the literal's context fixes of its type, read as the
checker reads it. `lower_expr` builds one from its `expected` function type
(`ClosureHint::of`); a tensor traversal (element types, a `.zip` operand lowered first, a `.reduce`
seed first when the accumulator is unannotated), a `for`-head adapter (`plan_loop_adapters` takes
the head's element type) and a generic call build theirs through `lower_function_arg`. A
parameter takes its annotation, else the hint. A block body with a return type lowers through
`lower_body`; without one, `lower_inferred_body` types it by its last expression or trailing
`if`/`else`, as `infer_closure_block` does. The value site emits `HirExprKind::Closure { name, captures }` typed as `HirType::Function`, and
`lower_ident_call` dispatches a call on a local function-typed binding indirectly (the callee is
a `HirExprKind::Variable` of `HirType::Function`).

A composition arrives as the closure the parser built (`ast_types::COMPOSE_PARAM`); the only
lowering of its own is `compose_param`, which types that parameter from the first stage's
signature when the hint leaves it open, as the checker's `compose_param` does.
`lower_function_value` lowers a function named as a value (`val f = square`, BUG-071), an
associated path (`Point::scale`), a non-generic enum's tuple variant (`Error::Io`, signature from
`variant_constructor_signature`, which `assoc_signature` falls back to), a generic enum's tuple
variant (`is_generic_tuple_variant`; its parameters are the expected function type's, or, as a
generic call's argument, the `ClosureHint` the call has fixed, in `lower_function_arg`) and an
instance (`convert::<u8>`, typed by
`generic_instance_signature`, or `Cell::new::<i32>`, by `assoc_instance_params`) to a capture-free closure forwarding its `__fn_value_argN`
parameters, through `lift_capture_free`. A closure literal called in place with an unannotated
parameter (`lower_value_call`) lowers its arguments first and hands their types to the literal
as its hint, the order the checker checks them in.

### Tensor operations
`tensor_ops.rs` runs last in `lower_program`, after the derivatives, in every program. It
outlines each tensor operation out of host code (`Host` functions, methods, closures) into a
function named `__tensor_op_N`, its parameters named `__operandN`, whose body `mlir-backend`
computes: the LLVM backend has no tensor loops of its own. The function is
`HirTarget::FollowsOperands` in a program that lowered a tensor `.to(...)` (`transfers`), the one
way a device tensor comes to exist, so a backend may give it a device body too, and
`HirTarget::HostOperation` in any other program, which has a host body only.

An operation is outlined when its tensors have an element with arithmetic (a number, or `f16` /
`bf16`), static or dynamic shape alike; a slice or a permutation also takes a `bool` tensor, since
it moves elements without computing them. A maximal tree becomes one function over its leaves:
`+ - * / % @` and a permuting shape cast take their operands as written, elementwise math, a
slice and an `einsum` with output letters only read theirs, so each takes another node of the
tree or a leaf lent as `&leaf`. A reduction, a `.sort()` / `.argsort()` / `.topk()`, a full
`einsum` and a `.map` / `.zip` / `.reduce` are each one function over their receiver (and
`.zip`'s second tensor or `.reduce`'s seed). Parameters are the operands exactly as written (`&a`
stays a borrow, `a` a move), except that a place an operation only reads (a binding, a field, an
element, a deref) is passed as `&place`, which the LLVM backend lends through a cell holding its
handle. A `.topk` function returns its values/indices tuple as is. A whole-tensor reduction, a
full contraction and a `.reduce` yield a scalar, which a GPU body cannot return, so their
function returns a `[1]` tensor (`TensorLiteral` of the operation) and the call site reads
element 0.

A slice's positions become parameters checked where the call evaluates them:
`{ val __position = p; if !((p as u64) < extent) { panic("tensor index out of bounds") } __position }`,
the host's own guard and message, so a body never reads an unchecked position. A traversal's
function value is passed as a last parameter, `__callee`, which is how a host body calls it; a
closure literal whose captures are all scalars also stays in the body with its captures as extra
parameters named after them, which is where the literal loads them from and how a GPU body calls
it. Any other function (one reached through a local) is read from `__callee` in the body too. A
`.reduce` whose accumulator is no scalar is not outlined: it becomes the loop it is,
`{ val __receiver = r; mut __acc = init; val __callee = f; for each position in row-major order
{ __acc = __callee(__acc, __receiver[...]) }; __acc }`. A compound assignment `place OP= v`
becomes a `Void` call taking `(v, &mut place)` (a `&mut` binding is passed as itself) whose body
is the `TensorCompoundAssign` through `*__operand1`, so the update writes the target's own buffer
wherever it lives. Operands keep their spans inside the body, so a check the body reports still
names the operation's position.

**Dynamic axes.** A body over a `?` axis reads its extent from the operand at run time, so the
call site sizes the result from `HirFunction::result_extents`: one `HirExtent` per `None` axis of
the result (tuple parts in order), over the function's parameters (`extent_of`). An element-wise
operation or a matrix product takes the operand that walks the axis, counted from the trailing
axis, so a dynamic axis is never stretched; a matrix product's rows come from the left operand,
its columns from the right, and its batch axes from whichever has them. A permutation takes
receiver axis `permutation[d]`, a reduction the receiver axes other than the reduced one, a sort
or a map the receiver's own, a slice the `Whole` axes of its object, and an `einsum` the operand
carrying the output letter. Each run-time check is emitted where the call evaluates its operands:
a position or a constant range on a dynamic axis (`checked_position`, `dynamic_axis_check`), and
`.topk(k)` past a dynamic axis (`guarded_operand`). The checks borrow the object, so an object that
is a temporary is bound first (`outline_temporary_index`) and the checks read that binding.

### Dynamic extents (`extents.rs`)

A named extent `?B` is a hidden `u64` local, `__extent_B` (the `__` prefix is reserved). A name is
in scope from its binding to the end of the block its local was declared in (`extent_names`,
dropped with the scope). Inside its scope `B` reads that local (`lower_expr_uncoerced`), unless a
local of the same name shadows it. A closure body that reads `B`, as a value or in a shape it
writes (a turbofish, an annotation), captures `__extent_B` by copy (`collect_captures`), like any
scalar it does not write.

- **Signatures** (`bind_parameter_extents`): the body's prologue declares `mut __extent_B = 0`
  and assigns it from the first parameter axis that spells the name; every later occurrence is a
  run-time check that panics with the R7 call message naming both parameters.
- **Annotations** (`bind_annotation_extents`): a fresh name is declared before the `val` / `mut`
  and assigned from the value after it; a bound name is checked with the R7 annotation message.
  A reference annotation is read through the reference.
- **Statements**: a name first written by an expression (a constructor, `.as_shape`) is declared
  right before the statement that holds the expression, in the block the statement sits in.
  `in_extent_statement` wraps every statement and block tail for this; `lower_stmt_into`, the
  body's tail and `lower_block_value_inner` all go through it.
- **Constructors**: `zeros`, `ones`, `identity` and `random_normal` fill `dims` with one `u64` per
  `None` axis (`constructor_dims`). A bound name reads its local; a fresh one takes its `dims:`
  entry; an anonymous axis takes its entry. The names come from the turbofish or, without one,
  from `written_types`: the written type the checker's written-type sites supply, keyed by the
  expression's span (a `val` annotation, `written_return` for a `return` operand or tail, an
  annotated binding from `written_bindings` for an assignment). From a site only a bound name
  is read; a fresh one is bound by the annotation. The entries are bound to temporaries in order, in a
  block. `identity` over `[?B, ?B]` repeats the value. `random_normal` takes `dims:` as its third
  argument; `zeros`, `ones` and `identity` take it as their only one. Argument binding puts the
  `dims:` argument in that position, so it is read positionally here.
- **`.as_shape::<[...]>()`** (`lower_as_shape`): a block binds the owned receiver, assigns each
  fresh name from its axis, and yields `Some(tensor)` when every literal, parameter and bound name
  agrees, else `None`. The `Some` payload carries the written type, so the enum instance is
  `option_of` that type. A static receiver axis is not compared.
- **Struct literals** (`named_struct_literal`): a literal whose fields name extents binds each
  field value to a temporary in written order, then binds the struct's names in declaration order,
  the first field naming a name binding it and each later one checked (the R7 struct message). The
  names are local to the literal's block. Generic structs use the template's field types.
- **Call results** (`instantiate_result`): a call's declared result has each `None` axis the
  callee's written return type names replaced by the first extent an argument carries for that
  name. Tensors, tuples and borrows are rebuilt; any other result (an `Option`, say) keeps its
  declared form, and the checker types it the same way. `ExtentSignature` holds the names of a
  signature, for free functions, methods, generic impls' instance methods and generic templates (a
  generic call instantiates the template's written return type).
- **Reshape on a dynamic receiver** (`dynamic_reshape`): the receiver is bound to a temporary and
  the product of its extents is compared with the target count, which panics with the R7 reshape
  message. A `-1` is the count divided by the product of the other entries, and a zero divisor
  panics first. A reshape that names no dynamic extent casts inline, with each dynamic result axis
  a count quotient over the receiver's `HirExtent`s. One that names a dynamic extent is outlined
  into a host function `__reshape_N` taking the receiver and each name's value as a `u64` operand
  (`Scalar(k)` in its `dims`), because a `HirExtent` cannot read a local. A `.flatten` over a
  dynamic run is the product of the run's extents and needs no check.

An `einsum` letter bound to a dynamic axis is `None` in `TensorEinsum.extents`, and its result
axis is sized from the operand axis that letter walks. A `file.tensor` / `out.add` over a dynamic
shape is refused by the checker; one reaching lowering is `Malformed`.

### Dynamic dispatch
A `traits` table (name → methods in declaration order, with their visible parameter and return
types) and a `supertraits` table (name → direct supertraits) are registered before impls.
`trait_closure` walks the second. Each `Item::Trait` lowers to a `HirItem::Trait` carrying
`vtable_slots`, the trait's own methods then each supertrait's, one slot per name: the canonical
layout backends need. A call on `&dyn Sub` finds its signature through the closure, and the
coercion supplies methods for every trait in the closure, so a derived supertrait fills its slot. `resolve_type` delegates to
`resolve_type_ctx(ty, behind_ref)` so `&dyn Trait` resolves to `Reference(DynObject)` while a bare
`dyn` is an internal error (the checker rejects it first). `[T]` rides the same flag: it lowers to
`HirType::Slice` only behind a reference.

`lower_expr` is a thin wrapper that lowers via `lower_expr_uncoerced` and then applies
`apply_unsizing_coercion`. That is the **single site** where both unsizings are inserted:
`&T` → `&dyn Trait` (`DynCoerce`) and `&[T; N]` / `&Vec<T>` → `&[T]` (`SliceCoerce`), so every
context supplying an expected type (call arguments, returns, annotated bindings) gets them
uniformly, and a value that already has the target shape is never re-coerced. A generic call
lowers its arguments before the instance's parameter types exist, so `coerce_to_params` applies
the same coercion against the instance's concrete parameters once they are known, for a generic
function, method or associated function alike. A `DynCoerce`
whose concrete type implements the trait with no impl (a derive, a builtin) gets the impl the
compiler supplies from `emit_supplied_vtable_impl`, once per `(trait, type)` pair: `fmt_debug$dyn`
pushes the `{self:?}` rendering into the builder, `hash$dyn` returns the builtin hash, and a
marker trait gets an empty impl. A `&string` cannot be a trait object (its reference is a fat
pointer), which the checker refuses written directly and lowering reports as `StringTraitObject`
when a generic instance reaches it. A method call on a `DynObject` receiver types from the trait
declaration, naming no implementor, and `lower_supplied_trait_method` leaves it to the vtable. Return-position
`impl Trait` resolves to the type its body produces (`items/impl_returns.rs`): `register_items`
ends with `resolve_impl_returns`, which lowers each body with `lower_inferred_body` on a clone of
the `Lowerer` and registers the tail's type, in rounds as the checker does, so a body calling an
unregistered one waits. An inherent method's (`method_returns_impl_trait`) is left out of
`register_impl_methods` and probed the same way with `self` bound (`probe_method_return`); it joins
`functions` and `impl_methods` once resolved, and `lower_method` reads its return type back. `adopt_instances` makes in the real `Lowerer` the generic instances the
type names, which the clone made and dropped. A generic template's return is probed per instance
(`template_return_type`, from `instance_signature` and `lower_template`), with `probing_returns`
stopping a body that reaches itself.

An **associated type** reaches lowering as a `Type::Named` spelled `Self::Item`. `enter_impl_assoc`
adds each `impl` block's bindings, and the ones it inherits from a supertrait's impl
(`ImplDef.inherited_assoc`), to `type_subst` under that spelling for the block's signature
registration and its bodies: a name standing for a concrete type over one block is what the
type-parameter substitution already is, so annotations resolve through the one path. A generic
impl's methods are resolved per instance, so `with_impl_assoc` binds the same names under each
instance's substitution, in signature registration, the `declared_receivers` template pass and
body lowering alike. A trait's own
declaration has no implementor, so `resolve_trait_sig_type` gives such a position `Void` in the
`traits` table, and a bare `Self` position too; nothing reads either, because a trait naming one
is not object-safe. Inside an `impl` no `Self` reaches lowering: the parser replaced it.

`trait_assoc` holds, per `(trait, type)`, what each impl bound its associated types to
(`record_trait_assoc`, instance impls included via `register_instance_methods`). A generic call
whose arguments leave a type parameter unsolved fills it from a bound's constraint through
`infer_from_bounds` (in `items/register.rs`), called by `lower_generic_call` and
`instantiate_generic_method`, the same fixpoint the checker runs.

### Places
`lower_place` resolves `ast_types::Place` to `HirPlace`, giving each form the type of the
LOCATION: a binding's own type, a struct field's declared type, a tuple's element type (the
object auto-dereferenced to one `&`, as for a field), the element an indexable yields,
a tensor's element type, or a reference's referent. The base of each projecting form is lowered
as an ordinary expression, because reaching a location is a read of everything up to its last
step. A one-argument index over a tensor receiver becomes `HirPlace::TensorIndex` with a single
position axis, mirroring what `lower_tensor_index` does for the read.

A place the checker accepted but this cannot resolve is `LoweringError::Malformed`: it is a
compiler bug, not a diagnostic.

### Per-construct lowering notes
- **Slices**: `.slice(range)` is routed to `lower_sequence_slice` *ahead of* the collection
  method surface, because a `Vec` receiver's `.slice` borrows its buffer rather than acting on the
  header; `sliceable_element` names the three receivers that permit it (`[T; N]`, `Vec<T>`,
  `[T]`). Indexing, `for x in xs`, and an index place each read a slice's element type
  alongside the array's, and `slice.len()` is `u64`.
- **Tensors**: `resolve_type` maps `ast_types::Type::Tensor` to
  `HirType::Tensor { element, shape, names }`, resolving each extent through `resolve_tensor_dim`
  so a shape parameter takes the value the active instance bound it to and a `?` axis becomes
  `None`, and carrying each axis's
  dimension name into `AxisNames`; `mangle_type` spells the result `tensor_<elem>_<d0>x<d1>`,
  a dynamic axis as `?`,
  names taking no part (they are checked in the frontend and reach no backend). A symbolic extent with no binding is an
  `UnresolvedType` naming the dimension, which the checker's own diagnostic reaches first.
  `static_extents` is how a site that needs every extent as a value reads them: a tensor literal,
  `Tensor::from` / `scalar`, and the static reshape path, all of which the checker keeps static. A
  `?` there is a compiler bug and returns `Malformed` rather than panicking. A `?B` axis is `None`
  like `?`; its name lives in `extents.rs`, not in the type.
  Construction lives in `tensors.rs`. An array literal lowered against a
  `HirType::Tensor` expectation becomes `HirExprKind::TensorLiteral` whose `elements` are
  **flattened row-major**: the nesting carried the shape and the shape is on the node's type,
  so no backend re-derives it. The six constructors take the tensor type from the turbofish's
  single `type_args` entry, or from the expectation when the call was written without one, and
  pick a node: `zeros`/`ones` become `TensorFill` over a literal of the element type,
  `identity` becomes `TensorIdentity`, `random_normal` becomes `TensorRandomNormal`, and
  `scalar`/`from` become `TensorLiteral`. The first four carry `dims` for their open axes (see
  Dynamic extents). Shape, rank, arity, and applicability were settled by
  the type checker, so a violation here is a `LoweringError::Malformed`, not a diagnostic.
  The two ownership methods lower through `lower_builtin_method`: `.clone()` is nullary and
  takes the referent's tensor type (so a `&Tensor` receiver yields an owned tensor), and
  `.to(device)` lowers its one argument at `HirType::Enum("Device")` and yields the receiver's
  own type. `.to` is matched on `recv` rather than the referent, so a borrow does not resolve:
  the same verdict the type checker reaches. A compound `Stmt::Assign` is the one statement
  whose lowering is type-directed: a place with a `*Assign` impl calls it, a tensor place becomes
  `HirStmt::TensorCompoundAssign` carrying the place's tensor type, and every other place has its `place = place OP rhs` desugar
  re-formed as an `Expr::Binary` over `Place::to_expr()` and lowered through `lower_expr`, which
  is what keeps a by-value operator impl reachable through `+=`. A by-value tensor operator stays an ordinary
  `HirExprKind::Binary`; `binary_result_type` (`expressions/coercion.rs`) re-derives the
  broadcast join over `Vec<Option<usize>>` and `AxisNames` so the node carries the fresh
  result's shape, which is what tells the backend how big a buffer to allocate. A pair that
  does not join is `LoweringError::UnsupportedOperand`: the checker already accepted the
  operands, so failing here is a compiler bug rather than a diagnostic. `tensor_element` types
  a scalar operand by the tensor's element on both sides, including the compound-assignment
  right-hand side, or a bare literal would lower as the default `f64` and emit a mixed-width
  instruction. A bare literal left of a scalar of its own kind takes that scalar's type the same
  way (`literal_follows_right_operand`, the checker's rule repeated). `@` takes `matmul_shape` in the same file instead of the broadcast join: it
  contracts the operands' inner axis, so `[.., M, K] @ [.., K, N]` carries `[.., M, N]` with the
  left operand's row name and the right operand's column name, the batch axes joined by the
  broadcast rule. A scalar operand is
  `UnsupportedOperand` there rather than falling through to the element-wise arm, which would
  silently give it the tensor's own shape.
  Shape manipulation lives in `tensor_shape.rs`: `.t()`, `.reshape(...)`, `.permute(...)` and
  `.flatten(...)` are intercepted in `lower_method_call` **before the arguments are lowered**,
  because a `.permute` entry may be a dimension name and would otherwise be looked up as a
  variable. `lower_tensor_shape_cast` re-derives the result shape and emits
  `HirExprKind::TensorShapeCast`, whose `permutation` is `Some` only for `.t()` / `.permute`
  (the two that change the element order) and `None` for `.reshape` / `.flatten`. A name is
  resolved against the receiver's `AxisNames`, which is the one reason the HIR carries them;
  the result keeps the names its axes brought with them, and a merged or re-extented axis is
  unnamed. The checker validated all of this, so a shape that does not work out here is a
  `LoweringError::Malformed`. A dynamic axis keeps its `None`, and the cast's `dims` gives its
  run-time extent (`HirExtent` over operand 0, the receiver): a permutation reads receiver axis
  `permutation[d]`, a flatten the product of its run. A `.reshape` of a dynamic receiver, or into a
  dynamic name, is a block that binds the receiver, checks the element count (`dynamic_reshape`)
  and then casts.
  Reductions live in `tensor_reduce.rs`: `.sum()`, `.mean()`, `.max()` and `.min()` are
  intercepted in the same place and for the same reason (an `axis:` argument may be a
  dimension name), and `lower_tensor_reduce` emits `HirExprKind::TensorReduce` carrying the
  resolved `HirReduceOp` and the axis as an index, with a negative one already counted from
  the end. The receiver is left alone rather than moved: a reduction reads it, so a borrowed
  receiver lowers here too.
  Order-based selections live in `tensor_sort.rs`: `.sort()`, `.argsort()` and `.topk()` are
  intercepted in the same place and for the same reason, and `lower_tensor_sort` emits
  `HirExprKind::TensorSort` carrying the `HirSortKind`, the resolved axis (the last one when
  the call names none, a negative one already counted from the end) and the direction as a
  constant. `.topk` lowers to `HirSortKind::TopK(k)` at `descending: true`, top-k being the
  head of the descending order, and its type is the `HirType::Tuple` of the values tensor and
  the `i32` index tensor. The receiver is read, not moved, so a borrowed one lowers here too.
  An instance of a shape-generic body is where an empty sorted axis, or `k` past its extent,
  first has a number, so both are `InstanceCheck` errors here.
  The functional traversals live in `tensor_apply.rs`: `.map(f)`, `.zip(other, f)` and
  `.reduce(init, f)` become one `HirExprKind::TensorApply` whose `kind` says which. Every
  argument is lowered as a VALUE here, unlike a reduction's axis, and the function is always
  the last of them. The result type is rebuilt rather than carried over from the checker, as
  this slice re-derives every resolved fact: the receiver's shape and axis names over the
  callee's return type for the first two (neither changes which axis is which), and the
  seed's own type for `.reduce`. The receiver is read, not moved, so a borrowed one lowers
  here too.
  Elementwise math lives in `elementwise_math.rs`: `math_op` maps `.exp()` / `.log()` /
  `.sqrt()` / `.tanh()` / `.abs()` / `.pow(p)` to a `HirMathOp`, and a tensor receiver (owned
  or borrowed) or an `f32` / `f64` value lowers to one `HirExprKind::Math` typed as the
  receiver's value, `.pow`'s exponent lowered against the element type.
  Einstein notation lives in `tensor_einsum.rs`: `einsum("bij,bjk->bik", a, b)` is intercepted
  in `lower_plain_call` AFTER the user-declared functions, so a program's own `einsum` shadows
  it the way it shadows the panic and standard-output builtins. `lower_tensor_einsum` re-reads
  the subscript literal (`split_subscripts`) rather than carrying anything over from the
  checker, as this slice re-derives every resolved fact, and interns each letter to an index
  into one extent table. A `...` interns the axes it covers once, at the first operand that
  writes it, and the output takes those indices where it writes its own. What it emits is `HirExprKind::TensorEinsum` carrying only those
  indices, so the notation stops existing here and no backend parses a string. The operands
  are left alone rather than moved: a contraction reads them, so borrowed ones lower here too.
  Slicing and indexing live in `tensor_index.rs`: on a dynamic axis `..` is `HirTensorAxis::Whole`
  (the axis survives at its run-time extent), a position is bound to a temporary and checked against
  the extent (`dynamic_axis_check`, the same message as the backends'), and a constant range keeps
  its static extent with `end <= extent` checked. An element write (`place[i, j] = v`) binds and
  checks its positions before the store. `lower_tensor_index` folds each range bound to
  the constant the checker already proved it to be (`HirTensorAxis::Range { start, end, reversed, step }`, with
  a `..` full axis becoming the whole extent at step 1, an inclusive range stopping one further
  on, and a `.step(n)` stride folded the same way, dividing the surviving extent rounded up),
  lowers a position as an ordinary expression, and computes the result type by dropping every
  axis given a position along with its name, keeping the name of every axis that survives. Both index spellings reach it: `Expr::TensorIndex`, and the
  one-argument `Expr::Index` whose object lowered to a tensor. `.item()` on a rank-0 receiver
  (`expressions/calls.rs`) lowers to a `TensorIndex` with no axes, so the backends and the
  derivative tape read it as any element read and no node exists for it.
- **Enumerated loops**: `ForRange` / `ForEach` carry the position binding through as
  `index: Option<String>` and define it in the loop scope as `LOOP_INDEX_TYPE` (`u64`), ahead of
  the element binding so the two collide rather than shadow. The free-variable walker binds it
  too, or a closure in the body captures it.
- **Range bounds**: `lower_range_bounds` lowers the start first and the end against its type,
  except that an unsuffixed integer literal start beside a non-literal end is lowered against the
  end's type (`0..v.len()` runs over `u64`), the order `semantic-analysis` checks them in.
- **Reversed and stepped ranges**: `ForRange`'s `reversed` flag and `step` ride through to the
  adapted lowering as well as the plain one, the stride lowered against the range's element type.
  Nothing about the bounds, the element type, or the body changes with them; the walk is a
  backend traversal decision.
- **Tuples**: `resolve_type` gives `HirType::Tuple`; a literal is typed by lowering each element
  (hinted by the expected tuple's element type when annotated) and `t.N` reads the N-th element
  type off the auto-derefed tuple type. Destructuring is parser-desugared.
- **Enums**: registration is split in two, because a payload may name a struct and a struct field
  may name the enum: `register_items` reserves every enum NAME, registers the structs, then
  resolves the payloads. Structs register the same way among themselves: every name and generic
  template first, then the fields, so a field may name a struct declared after it. All three
  construction forms normalize to one `HirExprKind::EnumConstruct`: a unit `E::V` carries an empty
  payload, a tuple `E::V(..)` the positional args, and a struct `E::V { .. }` **reorders its
  provided fields into declared order** so codegen sees a single positional layout. `tag` is the
  variant's declaration index.
- **Array rest**: `Expr::ArrayRest { array, start, exact }` lowers to
  `HirExprKind::ArrayRest { array, start }` typed `[T; N - start]`, re-derived from the source
  array's `HirType`. A defensive arity re-check (`exact ⇒ N == start`, else `start <= N`) raises
  `Malformed` rather than underflowing `N - start`.
- **Pattern matching**: a borrowed scrutinee is wrapped in a `Deref` first, so the patterns test
  its referent (`match d` on `d: &Dir` lowers as `match *d`); a `&[T]` stays the borrow, since a
  slice has no value of its own. `lower_pattern` walks a pattern once and returns its
  `HirMatchTest` and its bindings. `pattern_at` carries the path walked so far: a sub-pattern's
  test is wrapped in a `Part { step, test }`, and a binding records the path to its value. A tag
  comes before its payload tests and a `SliceLen` before its element tests, inside one `All`,
  because a backend stops at the first failure. Wildcards drop out. `char` / `bool` literals are
  scalar codepoints or 0-1, and an exclusive `a..b` is normalized to `a..=b-1`. A pattern that looks
  inside a value through a borrow adds a `Deref` step, at any depth. An array pattern whose length
  the instance's `[T; N]` cannot have is `Never`: only a size parameter gets there.
  `lower_arm_patterns` merges an arm's alternatives into one binding list in the first
  alternative's order, each binding gathering one path per alternative. Bindings are defined in a
  per-arm scope so the guard and body lower correctly. A `Pattern::UnqualifiedEnum` reaching
  `pattern_at` means neither module resolution nor the checker ran, and surfaces as
  `LoweringError::Malformed`.
- **Newtypes**: a pre-pass records each newtype's inner AST type; `resolve_type` maps the name to
  `HirType::Newtype { name, inner }`, resolving the inner recursively (the checker already
  rejected cycles). A `Call` whose identifier callee names a newtype lowers to
  `NewtypeConstruct` (value hinted by the inner type), taking precedence over a same-named
  function exactly as the checker does; `.0` on a newtype-typed object lowers to `NewtypeAccess`.
  No `HirItem` is emitted: the backends erase the wrapper.
- **Collections** (`collections.rs`): `lower_collection_new` lowers `Vec::new()` and its siblings
  to `HirExprKind::CollectionNew` typed from the annotated target (`Vec::with_capacity(n)`, matched
  by `is_collection_ctor`, carries its lowered `n` as `capacity`), and `lower_collection_method`
  re-derives each method's argument and result types, instantiating `Option<T>` through the
  ordinary generic-enum path so backends see a real `HirItem::Enum`: `get` / `get_mut` yield
  `Option<&T>` / `Option<&mut T>` (`borrow_of`), `insert` and a map's `remove` `Option<V>`, and
  `Vec::remove` / `swap_remove` the element type. `resolve_type` maps the
  `Vec` / `HashMap` / `BTreeMap` generic application to `HirType::Collection` rather than
  monomorphizing a nominal instance (a user-declared type of that name still shadows it), and
  indexing, an index place, and `for`-in resolve a `Vec` element alongside an array's.
- **`StringBuilder`**: `collection_kind` recognizes the name and the `nullary_collection` helper lets
  `resolve_type` accept it as a complete type, checked **after** the struct/enum/newtype arms so a
  user declaration shadows it. `lower_collection_new` builds a nullary kind's type itself instead
  of requiring an annotated target; `lower_collection_method` adds `push_str` (`string` parameter)
  and `to_string` (`string` result). `mangle_type` uses `HirCollectionKind::mangle_tag()`, so
  `StringBuilder` mangles as `strbuf` and yields the bare tag when there are no arguments.
- **`.slice(range)` / `.char_slice(range)`**: `lower_builtin_method` gives both the same
  `&string` result type and lowers the range argument unchanged. The two differ only in the unit
  their indices count (bytes vs. code points), which is settled in the backend, so this slice does
  not distinguish them beyond the method name reaching codegen.
- **`.chars()`**: intercepted in `lower_method_call` ahead of every other dispatch and lowered
  to the prelude iterator's own `StructLiteral`: `Chars { source: &receiver, offset: 0 }`. No
  backend learns that `.chars()` exists. The borrow is emitted even for a temporary receiver,
  which is sound because an immutable borrow of a `string` IS the fat pointer by value.
  `.lines()` and `.split(sep)` build `Lines { source, start: 0 }` and
  `Split { source, separator, start: 0, done: false }` the same way (`text_iterator`), the
  separator lowered against `&string` so a literal is a view.
- **The runtime intrinsics** (`__read_text`, `__arg`, ...): `lower_call` types a call to one by
  `os_intrinsics::os_intrinsic_signature`, after every other free-function route, and lowers it as
  a `Call` on a `Variable` callee the backend recognises by name. The checker has already confined
  them to library bodies.
- **`std::safetensors`'s compiler-known methods** (`safetensors.rs`), intercepted at the top of
  `lower_method_call`, which now takes the expected type. `file.tensor(name)` becomes a `Match` on
  the library's private `std::safetensors::locate(&file, name, "F32", "[2, 3]")`, the dtype and
  shape spelled as text the library compares, whose `Ok` arm wraps
  `__tensor_from_bytes(bytes)` in a new `Result<Tensor<T, S>, IoError>` instance and whose other
  arm forwards the error. `out.add(name, &t)` becomes
  `std::safetensors::append(&mut out, name, dtype, shape, __tensor_bytes(&t))`. The two intrinsics
  are the backend's and appear in no checker table. `lower_try` and `lower_null_coalesce` take
  the expected type and pass it to a `.tensor(...)` operand only, as the checker does.
- **`__char_at(offset)`**: `lower_builtin_method` types the prelude's decode step as
  `HirType::Char` with a `u64` argument. The semantic pass has already refused it to every
  module but the prelude's, so no gate is repeated here.
- **`.is_nan()`**: `lower_builtin_method` types it as `HirType::Bool` on any float receiver
  (`is_float`, half precision included), with no arguments to lower.
- **`.abs()`** on an integer: typed as the receiver, no arguments. The checker has already
  refused an unsigned receiver; a float one takes the elementwise-math path instead.
- **`checked_{add,sub,mul}`**: `lower_builtin_method` types these on any integer receiver as
  `Option<T>` over that receiver, reusing `collections.rs`'s `option_of` so the instance is
  materialized as an ordinary `HirItem::Enum` exactly like `Vec::pop`'s.
- **`.to_checked::<T>()`**: `lower_builtin_method` types it on any float receiver as
  `Option<T>`, `T` resolved from the turbofish, which `lower_method_call` passes through for this
  one method. With `checked_*` these are the builtin intrinsics whose result is a monomorphized
  enum rather than a fixed type.
- **`.parse::<T>()`**: `lower_builtin_method` types it on a `string` receiver as
  `Result<T, ParseError>`, `T` resolved from the turbofish and the instance built with
  `instantiate_generic_enum`. The checker has already refused every other `T`.
- **Interpolated strings** (`expressions/interpolation.rs`): `Expr::InterpString` lowers to
  `HirExprKind::InterpString`, typed `string`. Each hole lowers with **no** expected type: the
  hole's own expression decides its type, and the rendering follows from that.

### Two guards against a wrong program
- **A surviving argument label is refused.** Lowering rejects an `Expr::Call` whose `arg_labels`
  is non-empty. Every label is bound to a parameter by `argument-binding` before type checking;
  one surviving to here would mean that pass never reached the call, and the arguments would then
  lower in the order they were *written* rather than the callee's: a wrong program rather than a
  failed build. The check costs one `is_empty` per call.
- **An operand the backend cannot lower is refused** (`LoweringError::UnsupportedOperand`,
  BUG-015). `binary_result_type` (`expressions/coercion.rs`) asks `has_operator_lowering` whether
  the operand type has an instruction sequence: the scalars, `string`, a `&string` slice, and a
  newtype forwarding one of those. The checker rejects a concrete aggregate operand itself; the
  path that reaches here is a **generic body**, which types `a == b` as its type parameter and is
  checked once as a template, so an instantiation with an aggregate argument used to arrive at
  codegen and abort it. An operator-trait impl is dispatched to a method call before operand types
  are ever combined, so a struct with `impl PartialEq` never reaches the check, and a struct with
  `@derive(PartialEq)` is returned before it, for the same reason.
