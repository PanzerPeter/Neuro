# Tensors

`Tensor<T, [d0, d1, ...]>` is an n-dimensional array whose element type and every extent are part of its type, with each extent ordinarily known at compile time. Use it for numeric arrays and linear algebra. An axis written `?` defers that one extent to run time ([Dynamic shapes](#dynamic-shapes)), and device placement and GPU kernels follow the CPU operations below.

```neuro
type Weights = Tensor<f32, [784, 128]>

struct Layer {
    bias: Tensor<f32, [128]>
}

func forward(w: Weights, x: Tensor<f32, [128]>) -> Tensor<f32, [128]> {
    return x
}

func loss(l: Tensor<f32, []>) { }        // rank-0 scalar tensor
func image(px: Tensor<u8, [3, 224, 224]>) { }
```

The shape is a bracketed list of non-negative integer literals, of extent parameters and constant expressions over them inside a generic definition ([Shape generics](#shape-generics)), or of `?`. A shape parameter stands for a whole shape, written `Tensor<T, S>` or spliced into a longer one as `[..S, N]` ([Shape parameters](#shape-parameters)). `[]` is the rank-0 scalar tensor. The element type is a fixed-width scalar: any integer type, `f16`, `bf16`, `f32`, `f64` or `bool`, or a type parameter inside a generic definition ([Element-type generics](#element-type-generics)).

Rank and every extent are part of the type, so a mismatch is a compile error naming both types:

```neuro
func takes_square(t: Tensor<f32, [3, 3]>) { }

func pass_through(t: Tensor<f32, [2, 2]>) {
    // error: type mismatch: expected Tensor<f32, [3, 3]>, found Tensor<f32, [2, 2]>
    takes_square(t)
}
```

`Tensor` is a prelude name, not a keyword, so a module that declares its own `Tensor` shadows it. A shape argument is what marks a type as a tensor, so the same form under any other name is a parse error.

## Building a tensor

A nested array literal becomes a tensor where an explicit `Tensor<...>` annotation is in scope. The annotation supplies the element type and every extent, and it types the literal's leaves: `1.0` under a `Tensor<f32, ...>` annotation is an `f32` literal, the same way `val x: f32 = 0.01` types its literal.

```neuro
val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]

val m: Tensor<f32, [2, 3]> = [
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0]
]

val arr = [1.0, 2.0, 3.0]          // no annotation: a plain [f64; 3], not a tensor
```

A nested literal must be **rectangular**: every sub-array at a depth has the length its extent declares, and the nesting is as deep as the shape is long. A value that already has a type is not converted for the annotation's benefit, so a non-literal element must already be the element type.

```neuro
val a: Tensor<f32, [3]> = [1.0, 2.0]             // error: this literal has 2 element(s) where the tensor's shape declares 3
val b: Tensor<f32, [2, 3]> = [1.0, 2.0, 3.0]     // error: this literal has 3 element(s) where the tensor's shape declares 2
val c: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0]]   // error: this literal has 2 element(s) where the tensor's shape declares 3
```

Where no annotation reaches, name the type with a turbofish and use a constructor:

```neuro
val zeros = Tensor::<f32, [3, 3]>::zeros()
val ones  = Tensor::<f32, [3, 3]>::ones()
val eye   = Tensor::<f32, [4, 4]>::identity()
val w     = Tensor::<f32, [128, 64]>::random_normal(mean: 0.0f32, std: 0.02f32)
val v     = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])
val loss: Tensor<f32, []> = Tensor::scalar(0.5)
```

`identity()` applies only to a square rank-2 shape, `random_normal` draws only into `f32` or `f64`, `scalar` builds only the rank-0 tensor, and `from` takes the same nested literal the annotated form coerces. A rank-0 tensor has no array-literal form, so it is written `Tensor::scalar(value)`. The generator behind `random_normal` is seeded with a fixed constant, so a compiled program draws the same values on every run.

## Ownership and clones

A tensor owns its buffer, so it is not `Copy`. Binding it, passing it, returning it, storing it in a struct field and `.to(device)` all move it. Pass `&Tensor<T, S>` or `&mut Tensor<T, S>` to share it.

```neuro
func consume(t: Tensor<f32, [2, 2]>) { }

func twice(t: Tensor<f32, [2, 2]>) {
    consume(t)
    consume(t)                            // error: use of moved value 't': bind a `.clone()` if you need an independent copy
}
```

`.clone()` is the explicit way to get a second owner. It takes no arguments, yields a tensor of the same type and leaves the receiver usable. Through a borrow it yields an owned tensor, not the borrow.

```neuro
func consume(t: Tensor<f32, [2, 2]>) { }

func read(t: &Tensor<f32, [2, 2]>) -> i32 { return 2 }

func twice(t: Tensor<f32, [2, 2]>) {
    consume(t.clone())
    consume(t)                            // fine: the clone was consumed, not `t`
}

func borrowed(t: &Tensor<f32, [2, 2]>) {
    consume(t.clone())                    // an owned copy of someone else's tensor
    read(t)                               // the borrow was never consumed
}
```

A tensor value is a DLPack handle, a pointer to a `DLManagedTensorVersioned` whose `data` field addresses a flat, row-major run of the elements. The buffer is allocated at construction and released when its binding leaves scope, through the handle's `deleter`, which is also the release path for a foreign owner. The handle carries the rank, shape, strides, element dtype and device, so the pointer a Neuro program passes is the one NumPy or PyTorch reads, with nothing wrapped or converted at the boundary. The handle's `manager_ctx` points to a compiler-private control block that shares the allocation and is released with it. A foreign consumer hands that pointer back to the deleter and never reads through it. The block holds the tensor's gradient slot, which the deleter releases first, so a gradient never outlives its tensor (see [Automatic Differentiation](autodiff.md#running-the-derivative-backward-grad-zero_grad)).

The buffer keeps one address for its whole life and is aligned to 64 bytes, as DLPack requires. `.clone()` allocates a second handle and buffer. `.detach()` consumes the tensor and returns the same buffer with an empty gradient slot (see [Automatic Differentiation](autodiff.md#stopping-the-gradient-detach-and-no_grad)). A host tensor's handle reports the `kDLCPU` device. A device tensor reports `kDLCUDA`, or `kDLROCM` on an AMD build.

A tensor held in a struct field, array element or enum payload is released with the value that holds it (see the [memory model](memory-model.md)).

The handle is checked, not only asserted. `neuroc compile --emit obj` on a module of tensor-returning functions, linked into a shared library, yields results that `numpy.from_dlpack` imports, reading the dtype, rank, extents and strides off the handle. `tools/dlpack_differential.py` runs this for a representative set of operations (broadcasting, matrix product, reductions, integer element types, transposes, reshapes, rank-0 results and chained operators) and compares the elements with NumPy's answer. See [emitting an object file](../guides/cli-usage.md#emitting-an-object-file).

## Element-wise arithmetic

`+`, `-`, `*`, `/` and `%` combine two tensors element by element and return a **fresh** tensor. `*` is the element-wise product. The matrix product is [`@`](#matrix-multiplication).

```neuro
val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val b: Tensor<i32, [2, 3]> = [[10, 20, 30], [40, 50, 60]]

val sum = &a + &b                          // both operands read, neither consumed
val product = a * b                         // both operands moved
```

Every operator takes **borrowed** operands as well as owned ones. An owned operand is moved, as passing it to a function would move it. A borrowed one is only read, so a weight can feed an operator without leaving the binding that owns it. The result is always a new buffer, which is what lets an operator read two borrows at once.

The element type must have arithmetic: any integer or float, `f16` and `bf16` included. Each half-precision element is computed in `f32` and rounded back once. An overflowing element panics in the debug tier, and a zero divisor panics in every build.

### Broadcasting

The two operand shapes are aligned at their **trailing** axis. An axis whose extent is `1` is stretched across the other operand's extent. An operand of **lower rank** supplies the innermost axes and repeats across the leading ones.

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]

val row: Tensor<i32, [3]> = [100, 200, 300]
val wide = &m + &row                        // the row repeats down both rows of m

val column: Tensor<i32, [2, 1]> = [[7], [9]]
val tall = &m + &column                     // each column entry stretches across 3 columns
```

So a `[2, 3]` combines with a `[3]`, a `[2, 1]` and a `[1, 3]`. It does not combine with a `[2]`, because 2 lines up against the 3 at the trailing axis and neither is `1`. A pair that does not broadcast is a compile error naming both operands, for example ``the operands of `+` do not broadcast: Tensor<f32, [2, 3]> against Tensor<f32, [2]>``.

An extent parameter (see [shape generics](#shape-generics)) is never the axis that stretches, because whether it is `1` is not known until instantiation. It can still be stretched *into*, since a literal `1` on the other side is known at the call site. A dynamic axis is never stretched either ([Dynamic shapes](#dynamic-shapes)).

### Scalar broadcast

A scalar on either side of the operator is stretched across every element.

```neuro
val matrix: Tensor<f32, [2, 2]> = [[1.0, 2.0], [3.0, 4.0]]

val scaled = &matrix * 2.0                  // 2.0 is an f32 literal here
val shrunk = 0.5 * matrix                   // and so is 0.5
```

The scalar takes the tensor's **element type**, the same way `val x: f32 = 0.01` types its literal. A scalar that already has a type is not converted for the operator's benefit, so an `f64` value beside an `f32` tensor is a type mismatch. A bare literal takes the element type on either side of any tensor expression, including a call result such as `0.5 * make_matrix()`.

## Matrix multiplication

`@` is the matrix product. It is the one tensor operator that is not element-wise: it **contracts** an axis rather than walking one, so element `[i, j]` of the result is the dot product of row `i` of the left operand and column `j` of the right.

```neuro
val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val b: Tensor<i32, [3, 2]> = [[7, 8], [9, 10], [11, 12]]

val c = &a @ &b            // [2, 2]: c[0, 0] is 1*7 + 2*9 + 3*11 = 58
```

The shape rule is `[M, K] @ [K, N]` -> `[M, N]`. The two inner extents must agree and disappear. The result keeps the left operand's row axis and the right operand's column axis, with their dimension names.

```neuro
val x: Tensor<f32, [2, 3]> = Tensor::<f32, [2, 3]>::zeros()
val y: Tensor<f32, [5, 6]> = Tensor::<f32, [5, 6]>::zeros()
val z = x @ y              // error: the operands of `@` do not multiply: Tensor<f32, [2, 3]> against Tensor<f32, [5, 6]>
```

Axes in front of the last two are **batch axes**. Each batch element is multiplied on its own, and the batch axes broadcast by the element-wise rule: equal extents walk together, an extent of `1` stretches, and a lower-rank operand repeats across the leading axes. The two matrix axes never broadcast.

```neuro
val heads = Tensor::<f32, [8, 4, 16]>::ones()
val shared = Tensor::<f32, [16, 2]>::ones()
val each = &heads @ &shared             // [8, 4, 2]: `shared` repeats across the 8
```

There is no vector form, because a rank-1 operand has no inner axis to contract, and no scalar form: a matrix-vector product is `m @ v.reshape([-1, 1])`. A dynamic axis follows the naming rule of [Dynamic shapes](#dynamic-shapes): the contracted axes must be the same literal, parameter or name.

Shape parameters make `@` generic. The repeated `K` is checked once, at the declaration, rather than per call site:

```neuro
func project<M, N, K>(w: &Tensor<f32, [M, K]>, x: &Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    return w @ x
}
```

A shape parameter spliced in front of the matrix axes makes one function serve every batch rank, none included. A splice may stand for batch axes but not for a matrix axis:

```neuro
func bmm<S, M, K, N>(a: &Tensor<f32, [..S, M, K]>, b: &Tensor<f32, [..S, K, N]>) -> Tensor<f32, [..S, M, N]> {
    a @ b
}
```

`@` shares the precedence row of `*`, `/` and `%`, as in Python, so a mix of them groups left to right: `w @ x * s` scales the product and `x * s @ w` scales the input. It binds tighter than `+` and looser than `as`, so `w @ x + b` adds the bias to the product. The `[N]` bias broadcasts down the product's rows under the element-wise rule above. Like every tensor operator, `@` takes owned or borrowed operands, so `&w @ &x` reads a weight without moving it out of its binding.

On a user type, `@` dispatches through the `MatMul` trait, exactly as `+` dispatches through `Add` (see [Operators](operators.md#operator-overloading)).

## Updating a tensor in place

The compound assignment operators `+=`, `-=`, `*=`, `/=` and `%=` update a `mut` tensor's own buffer element by element. They do not desugar to `w = w OP g` as they do for other types. Nothing is allocated, and the tensor's DLPack handle and `data` pointer are unchanged across the statement. The bitwise forms `&=`, `|=`, `^=` and `<<=` are errors on a tensor, which has no bitwise operators.

```neuro
mut w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
val step = Tensor::<f32, [784, 128]>::zeros()

for i in 0..8 {
    w -= &step                            // in place: one buffer for the whole loop
}
```

The operand is a tensor of the same element type, owned or borrowed. `w += g` consumes `g`, while `w += &g` reads it, so one operand can serve every iteration of a loop. It broadcasts under the rule above, with one asymmetry the in-place write forces: the result goes back into the buffer the target already owns, so an operand may be stretched **up to** the target's shape and never past it. A scalar is accepted the same way, which is what makes `w *= 2.0` the scalar broadcast. The operand is evaluated before the target is borrowed for the update. The element type must have arithmetic, half precision included, with the same overflow and zero-divisor guards as the scalar operator (see [Compound Assignment Operators](operators.md#compound-assignment-operators)).

## Slicing and indexing

A tensor index gives **one argument per axis**. Each argument is a position, a range, or the whole axis `..`. An axis given a position is **dropped** from the result. An axis given a range or `..` **survives** at the extent that range names. So naming every axis with a position reads one element, and anything else builds a smaller tensor.

```neuro
val m: Tensor<i32, [3, 4]> = [
    [0, 1, 2, 3],
    [10, 11, 12, 13],
    [20, 21, 22, 23]
]

val element = m[1, 2]                     // i32: every axis dropped
val row: Tensor<i32, [4]> = m[0, ..]      // the whole first row
val column: Tensor<i32, [3]> = m[.., 1]   // the whole second column
val block: Tensor<i32, [2, 2]> = m[1..3, 2..4]
val inclusive: Tensor<i32, [3, 3]> = m[0..=2, 0..=2]
```

A **position** may be any integer expression, including one known only at run time, which is what lets a loop walk a tensor. A **range bound** must fold to a compile-time constant, because the extent it produces is part of the result's type. `t[0..k]` with a `mut k` is therefore a compile error.

A constant position outside its axis, a range whose start is past its end, and a range reaching past the extent are compile errors. A run-time position is bounds-checked in every build, as an array index is, and panics when out of range.

```neuro
val m: Tensor<i32, [4, 2]> = Tensor::<i32, [4, 2]>::zeros()
val a = m[0..5, 0]                        // error: the slice `0..5` does not name a sub-range of axis 0, whose extent is 4
val b = m[4, 0]                           // error: index 4 is outside axis 0, whose extent is 4
```

A range argument may wear `.rev()`, which reads that axis back to front. Only the order changes: the surviving extent is the range's own. A sub-range reverses within its own bounds, not the axis's. Each axis decides its own direction, so `m[(0..2).rev(), ..]` flips the rows and leaves each row's order alone. The same `.rev()` reverses a `for` range (see [Iterating Backwards](control-flow.md#iterating-backwards-rev)).

```neuro
val samples: Tensor<i32, [5]> = [10, 20, 30, 40, 50]

val newest: Tensor<i32, [5]> = samples[(0..5).rev()]   // 50, 40, 30, 20, 10
val middle: Tensor<i32, [3]> = samples[(1..4).rev()]   // 40, 30, 20
```

A range argument may also wear `.step(n)`, which keeps every `n`th element. The stride divides the extent, rounding up, so it is part of the result's type and must be a positive constant. Under `.rev()`, the walk starts at the range's last element.

```neuro
val samples: Tensor<i32, [7]> = [10, 20, 30, 40, 50, 60, 70]

val evens: Tensor<i32, [4]> = samples[(0..7).step(2)]         // 10, 30, 50, 70
val newest: Tensor<i32, [3]> = samples[(0..7).rev().step(3)]  // 70, 40, 10
```

A slice is a **fresh owned tensor holding a copy**, not a view. The slice may be sliced again, cloned, passed by value or returned, and it is freed at the end of its own scope. Indexing **reads** its receiver and reads through a borrow, so `t[i, j]` on a `&Tensor<T, S>` parameter inspects a borrowed weight without moving it.

Range indexing is a tensor form. An array or a `Vec` takes one integer index and offers `.slice(a..b)` for a sub-range. Writing `xs[0..2]` on one is an error.

### Writing one element

An index that names every axis with a position is a **place**, so it can be assigned to:

```neuro
mut t: Tensor<i32, [3, 4]> = [
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 0, 0]
]

for row in 0..3 {
    for column in 0..4 {
        t[row, column] = row * 4 + column
    }
}

t[1, 1] += 100
```

The write goes into the buffer the DLPack handle already addresses. No handle is replaced and nothing is allocated, as with the in-place compound operators above. The bounds check a read carries applies to the write too.

An index that leaves an axis standing produces a fresh tensor rather than naming storage, so `t[0, ..] = 5` is a compile error: ``cannot assign to a tensor slice: an index that leaves an axis standing produces a fresh tensor, not storage``. Name every axis. The target must be a writable place: a `mut` tensor binding, or a tensor reached through one, such as a struct field.

### Reading by row-major position

`.flat(i)` reads the element at row-major position `i`, whatever the rank. On the tensor above, `t.flat(5)` is `t[1, 1]`. The position is a `u64` and is bounds-checked against the element count. Like an index, it reads through a borrow and moves nothing. The shape must be static, since the extents turn a position into one index per axis.

## Shape generics

A tensor extent may be a **generic parameter** rather than a literal, so one function serves every shape it is written for. A bare name in a shape position is a size parameter (see [Size and Const Parameters](generics-and-traits.md#size-and-const-parameters)). It is a compile-time `u64`, inferred from the shape of the argument the caller passes, and each distinct set of extents is monomorphized into its own specialization.

```neuro
func corner<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return t[0, 0]
}

func main() -> i32 {
    val wide: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val tall: Tensor<i32, [4, 2]> = Tensor::<i32, [4, 2]>::ones()
    val a = corner(&wide)                     // instantiated at M = 2, K = 3
    val b = corner(&tall)                     // and again at M = 4, K = 2
    return a + b
}
```

A parameter written more than once must have the same extent everywhere. That makes a shape mismatch a compile error rather than a wrong answer at run time:

```neuro
func pair<M, N, K>(a: &Tensor<i32, [M, K]>, b: &Tensor<i32, [K, N]>) -> i32 {
    return a[0, 0] + b[0, 0]
}

val x: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val y: Tensor<i32, [5, 6]> = Tensor::<i32, [5, 6]>::zeros()
val z = pair(&x, &y)   // error: extent parameter 'K' is already 3 here, but this argument makes it 5
```

An extent parameter is an ordinary size parameter. It can be read as a `u64` value in the body, and constrained by a `where` predicate that is checked at the call supplying the offending extent. A `const K: u32` parameter sizes nothing, so writing one as an extent is an error whose hint is the bare `K`.

```neuro
func last<N>(t: &Tensor<i32, [N]>) -> i32
where N > 0
{
    return t[N - 1]
}
```

An extent also flows into the **return** type, so a function can return a shape it derived from its argument's:

```neuro
func top_row<M, K>(t: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return t[0, ..]
}
```

Some things an extent parameter does not do:

- A tensor **literal** cannot be written against a symbolic extent, since one literal would have to serve every instantiation. Build the tensor with a constructor instead.
- A name that no enclosing signature declares is an error naming the dimension, not a silently accepted extent: ``tensor dimension 'N' is not a known extent``. A struct or enum declares its extents by bare name the same way, as in `struct Dense<IN, OUT> { w: Tensor<f32, [IN, OUT]> }`, and `impl<IN, OUT> Dense<IN, OUT>` passes them on.

### Reshaping over parameters

A shape may write an extent as a constant expression over its parameters: integer literals, extent parameters, a shape parameter's `S.count()`, parentheses and `+ - * / %`, with `/` and `%` floored as on `u64`. A generic body is checked once, before its extents have values, so `.reshape`, `.flatten` and the reductions work over parameters and their results are typed with such expressions:

```neuro
func split<B, D, H>(x: Tensor<f32, [B, D]>) -> Tensor<f32, [B, H, D / H]>
where D % H == 0
{
    x.reshape([B, H, D / H])         // `x.reshape([B, H, -1])` has the same type
}

func merge<B, H, W>(x: Tensor<f32, [B, H, W]>) -> Tensor<f32, [B, H * W]> {
    x.flatten(dims: [1, 2])
}

func head_peaks<B, H, W>(x: &Tensor<f32, [B, H, W]>) -> Tensor<f32, [B, H]> {
    x.max(axis: -1)
}
```

Two extents are the same when they agree by the laws of `+`, `-` and `*`, so `[N, 2].flatten()` is a `[2 * N]` and also a `[N * 2]`. A quotient simplifies only where it divides exactly or by a factor both sides share: a `[B, D]` reshaped with `-1` into `[B, H, -1]` gets the head width `D / H`.

Whether the element counts agree is a question about values, so it is answered at each instance rather than in the template: `[N, 2]` into `[N, 3]` holds at `N = 0`. An instance where a reshape changes the element count, a `-1` does not divide, a reduction runs over no elements, or an extent has no value (`N - 1` at `N = 0`) is a compile error. It is reported at the call in non-generic code that supplied the extents, with the failing operation as a note:

```text
error: `.reshape` would hold 4 elements at this instance but the receiver holds 6; a reshape cannot change how many elements a tensor has
note: checked here, in the instance `heads` with B = 1, D = 6, H = 4
```

A `where` predicate such as `D % H == 0` above states the requirement in the signature, so a caller that misses it is refused there, before any instance is built. An extent expression in a parameter type binds nothing: `[N + 1]` against a `[5]` does not solve for `N`, and is compared once another argument or the turbofish has bound `N`.

### Shape parameters

A **shape parameter** stands for a whole shape, of any rank, so one function serves vectors, matrices and batches alike. It is declared by bare name like an extent; writing it where the shape goes makes it one. `..S` splices it into a longer shape: in `[..S, N]` the last extent is `N` and `S` takes the leading ones, possibly none. A parameter's shape splices at most once, so a call can always tell which axes each parameter takes. A result shape may splice as often as it needs.

```neuro
func leaky<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    x * 0.75 + x * 0.25
}

func add_bias<S, N>(x: Tensor<f32, [..S, N]>, b: &Tensor<f32, [N]>) -> Tensor<f32, [..S, N]> {
    x + b                            // the bias repeats over every leading axis
}

func describe<S>(x: &Tensor<f32, S>) -> u64 {
    S.count() * 10 + S.rank()        // [2, 3] gives 62
}
```

In a body, `S` is read through `S.rank()` and `S.count()`, both `u64`. A shape-generic body accepts what is valid at every rank: moving and borrowing, `.clone()`, element-wise arithmetic and the scalar broadcast, the math methods, the constructors (`Tensor::<f32, S>::zeros()`), the traversals, a whole-tensor reduction, `.reshape`, and a whole `.flatten()`, which gives a `Tensor<f32, [S.count()]>`. An axis reduction and a sort work along an axis the shape writes, counted from its own side, so `axis: -1` of `[..S, N]` is `N` at every rank. A splice broadcasts only against the same splice, aligned at the trailing axis, so `[..S, N] + [N]` works and `Tensor<f32, S> + Tensor<f32, [3]>` is an error. A splice may stand for the batch axes of `@` and for an `einsum` `...`. Anything else that counts or picks axes (an index, `.t()`, `.permute`, `.flatten(dims:)`, `.item()`, a reduction or a sort along `axis: 0` of `[..S, N]`) is refused on a spliced shape: `` `.t` needs the rank of `Tensor<f32, [..S, 2, 2]>`, which the shape parameter 'S' leaves open ``.

A shape argument is written as a bracketed shape where a call or a type needs it spelled, and a parameter in scope is passed on by its bare name:

```neuro
struct Gain<S> {
    g: Tensor<f32, S>
}

impl<S> Gain<S> {
    func apply(&self, x: &Tensor<f32, S>) -> Tensor<f32, S> { &self.g * x }
}

func filled<S>(value: f32) -> Tensor<f32, S> {
    Tensor::<f32, S>::ones() * value
}

val half = filled::<[2, 3]>(0.5)       // no argument names the shape, so the turbofish does
val small: Gain<[3]> = Gain { g: [2.0, 2.0, 2.0] }
```

An argument with a `?` axis binds no shape parameter, as it binds no extent. [`tensor_shape_parameters.nr`](../../examples/tensors/tensor_shape_parameters.nr) runs every form above.

## Element-type generics

The element type may be a type parameter, and its bound says what the body may do with the tensor. A body is checked once against the bound and compiled once per element type it is called with.

| Bound on `T` | Operations on a `Tensor<T, S>` |
|---|---|
| none | moving, indexing, `.to` and the shape casts |
| `Numeric` | element-wise arithmetic and broadcasting against a `T`, `@`, compound assignment, `.sum()` / `.max()` / `.min()` and the sorts, `einsum` |
| `Float` | everything `Numeric` gives, plus `.mean()` and the elementwise math methods |

```neuro
struct Dense<T> {
    w: Tensor<T, [3, 2]>,
    b: Tensor<T, [1, 2]>,
}

impl<T: Numeric> Dense<T> {
    func forward(&self, x: &Tensor<T, [1, 3]>) -> Tensor<T, [1, 2]> {
        x @ &self.w + &self.b      // one body, used at `f32` and at `i64`
    }
}
```

An unbounded `T` admits `bool`, and a call that makes it anything other than a scalar is refused: ``tensor element type string is not a numeric scalar`` at the call, or, for a tensor built only inside the body, when that instance is compiled, still reported at the call with a note at the tensor that needed the scalar. The bounds are described under [Numeric bounds](generics-and-traits.md#numeric-bounds), and [`tensor_element_generics.nr`](../../examples/tensors/tensor_element_generics.nr) runs these forms.

## Named dimensions

An axis may carry a name before its extent. The name is part of the type, so a signature says which axis is which, and the compiler checks the claim:

```neuro
func batch_norm(
    x: &Tensor<f32, [batch: 32, channels: 3, height: 224, width: 224]>
) -> i32 {
    return 32
}
```

A name does not create a new type. Two tensor types agree when their element types agree, their ranks agree, and each pair of extents agrees. A name is compared only where **both** sides write one. So `Tensor<f32, [3, 224, 224]>` and `Tensor<f32, [channels: 3, height: 224, width: 224]>` are interchangeable, and a function written before the names existed still takes a named tensor:

```neuro
val plane: Tensor<i32, [height: 2, width: 3]> = [[1, 2, 3], [4, 5, 6]]
val same:  Tensor<i32, [2, 3]> = plane          // fine: one side names nothing
```

What is rejected is the transposition the feature exists to catch, the same extents the wrong way round:

```neuro
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

val t: Tensor<f32, [width: 4, height: 4]> = Tensor::<f32, [4, 4]>::zeros()
normalize(t)   // error: tensor axis 0 is named 'height' here but 'width'
```

The extents are identical, so nothing else would have caught it.

Names live in the tensor type's own namespace, not the surrounding scope. A local variable called `height` neither shadows an axis nor collides with one. Each axis of one shape needs its own name (`[side: 4, side: 4]` is an error). A name is not a generic parameter: in `[batch: N]`, `batch` names the axis and `N` is the extent parameter the call infers. An axis keeps its name through an index, at whatever extent survives:

```neuro
val plane: Tensor<i32, [height: 2, width: 3]> = [[1, 2, 3], [4, 5, 6]]
val row: Tensor<i32, [width: 3]> = plane[1, ..]   // `height` was dropped, `width` kept
```

Names are checked and then erased. A named tensor compiles to the same code as an unnamed one. The name-driven operations `.permute([...])` and `.flatten(dims: [...])` read them back (see [Rearranging a shape](#rearranging-a-shape)).

## Rearranging a shape

Four methods build a tensor of a different shape from the receiver's elements. Each result shape is computed at compile time, so the result is as statically shaped as the input.

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]

val t: Tensor<i32, [3, 2]> = m.t()                 // matrix transpose
val flat: Tensor<i32, [6]> = t.reshape([-1])       // same order, new extents
```

| Method | Result |
|---|---|
| `.t()` | the rank-2 transpose; any other rank is an error suggesting `.permute` |
| `.reshape([d0, ...])` | the same elements at new extents; `-1` in at most one position takes whatever extent the others leave over. An extent is a constant: a literal, a `const`, or arithmetic over them |
| `.permute([...])` | the axes reordered; every axis named exactly once |
| `.flatten()` | every axis merged into one |
| `.flatten(dims: [...])` | an adjacent run of axes merged into one, the rest untouched |

All four **consume** the receiver. A tensor owns its buffer, so handing it on is what a move means here. A transpose that quietly left a second copy of a weight matrix alive is exactly what move-by-default prevents. Use `.clone()` where the original must stay readable. A borrow cannot be consumed, so none of the four is offered on `&Tensor<T, S>`.

```neuro
val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
val t = m.clone().t()
val first = m[0, 0]                       // fine: the clone was consumed, not `m`
```

`.permute` and `.flatten` take **dimension names** as well as positions. A name resolves against the receiver's own shape, never the surrounding scope, so a local called `height` neither shadows the axis nor is shadowed by it. Naming a dimension the shape does not declare is an error that lists the ones it does.

```neuro
val image: Tensor<i32, [channels: 3, height: 2, width: 4]> = Tensor::<i32, [3, 2, 4]>::ones()
val hwc = image.permute([height, width, channels])    // [height: 2, width: 4, channels: 3]

val batch: Tensor<i32, [batch: 2, seq_len: 3, embed: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
val tokens = batch.flatten(dims: [seq_len, embed])    // [batch: 2, 12]
```

`.t()` and `.permute` carry each axis's name with it, so a transposed `[height: H, width: W]` is a `[width: W, height: H]` and is still rejected where the original was expected. A reshaped extent is not the axis its old name documented, so `.reshape` and a merged `.flatten` axis are unnamed.

The errors are compile-time:

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val a = m.reshape([4])                    // error: `.reshape` would hold 4 elements but the receiver holds 6
val b = m.reshape([4, -1])                // error: `-1` cannot be inferred: the other extents multiply to 4, which does not divide the receiver's 6 elements
```

Other rejected forms: `.t()` on a rank-3 tensor (``use `.permute([...])` to reorder the axes``), a `.permute` that names an axis twice (`` `.permute` names axis 0 twice ``), and a `.flatten` whose axes are not adjacent. Flattening re-describes the element order rather than moving elements, so the axes must be named in shape order.

## Reductions

A reduction folds a tensor's elements. With no argument it folds all of them into one scalar of the element type. With `axis:` it folds along that axis alone, which drops that axis from the result and leaves every other axis, extent and dimension name included, as it was.

```neuro
val grid: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]

val total: i32 = grid.sum()                        // 21
val peak: i32 = grid.max()                         // 6

val row_totals: Tensor<i32, [2]> = grid.sum(axis: 1)   // (6, 15)
val col_totals: Tensor<i32, [3]> = grid.sum(axis: 0)   // (5, 7, 9)
```

| Method | Result |
|---|---|
| `.sum()` / `.sum(axis: k)` | the elements added |
| `.mean()` / `.mean(axis: k)` | the arithmetic mean; `f32` and `f64` elements only |
| `.max()` / `.min()`, with or without `axis:` | the largest or smallest element. A float `NaN` never wins, so a run that is all `NaN` is the only one answering `NaN` |

Unlike the rearranging methods, a reduction **reads** its receiver. It produces a scalar or a fresh, smaller tensor, and leaves the buffer it summarised in place. It is offered on `&Tensor<T, S>` too, so a weight can be summarised without being moved out of its owner.

```neuro
func spread(scores: &Tensor<i32, [4]>) -> i32 {
    scores.max() - scores.min()
}

func main() -> i32 {
    val scores: Tensor<i32, [4]> = [4, 9, 2, 7]
    val range = spread(&scores)
    val total = scores.sum()                  // fine: `scores` was never moved
    return range + total
}
```

The axis may be a position, a **dimension name** the receiver's type declares, or a negative index counting from the end, so `axis: -1` is the last axis whatever the rank. A name resolves against the receiver's own shape, as with `.permute`.

```neuro
val frame: Tensor<f64, [height: 2, width: 3]> = [[0.0, 3.0, 6.0], [1.0, 4.0, 9.0]]

val column_means: Tensor<f64, [width: 3]> = frame.mean(axis: height)
val row_peaks: Tensor<f64, [height: 2]> = frame.max(axis: -1)
```

Reducing a rank-1 tensor along its only axis leaves the rank-0 `Tensor<T, []>`, the shape `Tensor::scalar` builds. `.item()` reads its one element back as a plain `T`. It does not consume the tensor, so it works through a borrow and the tensor stays usable. On a tensor of any other rank it is a compile error, and one should index a single element with `t[i, j]` instead.

```neuro
val scores: Tensor<i32, [4]> = [4, 9, 2, 7]
val folded: Tensor<i32, []> = scores.sum(axis: 0)
val total: i32 = folded.item()                 // 22
```

This is how a loss leaves a `@grad` function's rank-0 result as a number, for printing or for keeping past the `pool` block the tensor is released with.

**The order a float reduction adds in is part of the language.** Float addition is not associative, so the order decides the last bits of a sum. A run of up to 4096 elements folds left to right. A longer run folds in 4096 lanes: lane `l` folds the elements at run positions `l`, `l + 4096`, `l + 8192`, and so on, and then the lanes fold in order, lane 0 first. `.mean()` divides that sum by the run length. `.max()` and `.min()` take the same path, though their answer does not depend on it. Every backend follows this order, so a reduction gives the same bits on the host and on a GPU. The lanes can run side by side, in SIMD registers or as thousands of GPU threads, without changing the answer. The lanes are also more accurate than one running total: a long `f32` sum no longer stalls once each new element is below half its precision. Integer reductions fold left to right at any length, and their total is exact whatever the order. The order only decides where a checked build reports an overflow.

`.mean()` on an integer tensor is `sum / count` in the element type. It truncates toward zero, as integer `/` does, and the sum carries the overflow check of `+`. So `[1, 2, 3, -7]` has mean `0`, and a fractional mean is `.cast::<f32>().mean()`.

Two rules are compile-time errors:

- The element type must be an integer or a float. A `bool` tensor has nothing to fold: `` `.sum()` reduces a tensor's elements, which requires an integer or float element type; this tensor holds bool ``.
- A reduction over **no** elements is rejected, since `.max()` of nothing has no answer. Over an extent or shape parameter the run length is known only at each instance, which is where an empty one is refused ([Reshaping over parameters](#reshaping-over-parameters)). Over a dynamic axis the run is counted at run time, and an empty one panics.

A half-precision reduction accumulates in `f32`, so a long `bf16` sum does not stall.

## Sorting and selection

`.sort()`, `.argsort()` and `.topk()` order one axis of a tensor. They differ only in what they return: the elements in that order, the receiver positions that produce that order, or the leading `k` of both.

```neuro
val scores: Tensor<i32, [5]> = [5, 3, 9, 1, 7]

val up: Tensor<i32, [5]> = scores.sort()                      // (1, 3, 5, 7, 9)
val down: Tensor<i32, [5]> = scores.sort(descending: true)    // (9, 7, 5, 3, 1)
val order: Tensor<i32, [5]> = scores.argsort()                // (3, 1, 0, 4, 2)

val (top, at) = scores.topk(k: 3)     // top = (9, 7, 5), at = (2, 4, 0)
```

| Method | Result |
|---|---|
| `.sort()` / `.sort(axis: k, descending: d)` | the receiver's shape, elements in order |
| `.argsort()` / `.argsort(axis: k, descending: d)` | the receiver's shape at `i32`: the positions that produce that order |
| `.topk(k: n)` / `.topk(k: n, axis: k)` | a `(values, indices)` pair whose selected axis is `n` long |

`axis:` takes a position, a dimension **name** the receiver's type declares, or a negative index counting from the end. It defaults to the last axis, which is the row of a matrix. `.topk` selects the `n` **greatest**, so it has no `descending:` of its own. Its selected axis is `n` long in both results and carries no dimension name, since a truncated axis is no longer the thing its name documented.

An `argsort` entry is an ordinary integer, so it reads back into the tensor that produced it. A reduction cannot do that: `.max()` answers *what* the best element was, never *where*.

```neuro
val v: Tensor<i32, [4]> = [4, 9, 2, 7]
val order: Tensor<i32, [4]> = v.argsort(descending: true)
val best = order[0] as u64
val value = v[best]
```

These methods are **native to the element dtype**. `f32` and `f64` sort directly, without wrapping each element in an ordered-float type. The comparator is IEEE-754 ordered with one rule: `NaN` sorts to the **end**, whether the order is ascending or descending. Real workloads treat `NaN` as invalid, so pushing it to the back leaves the best candidates at the front, where top-k expects them. A program that wants the "partial order returns nothing on `NaN`" semantics maps through an ordered-float wrapper first.

Equal elements keep the order they were written in, which makes an `argsort` of a tensor with ties reproducible. On a GPU (a `@gpu` body, or a float tensor that lives on a device) the result is the same, ties and `NaN` included.

Like a reduction, a selection **reads** its receiver. It allocates its own result and leaves the ordered buffer where it was, so it is offered on `&Tensor<T, S>` too.

`k:` and `descending:` must be constants, because the result's shape and the comparator are both settled before any element is read. `k` must lie between `1` and the sorted axis's extent, so `` `.topk(k: 5)` selects more elements than the sorted axis holds, which is 4 `` is an error. The element type must be an integer or a float, half precision included. The receiver must have at least one axis. Over an extent parameter or a splice the result is typed from the shape as written (`.topk(k: 2)` of `[..S, N]` is `[..S, 2]`), the sorted axis must be one the shape writes, counted from its own side as for a reduction, and an empty axis or `k` past the extent is reported at the instance that makes it so. On a dynamic sorted axis, `k` past the extent panics.

## Einstein notation

`einsum` writes a contraction as subscripts: one per operand on the left of `->`, one letter per axis, and the result's axes on the right. A letter that appears in an input but not in the output is **summed over**. A letter in the output takes its extent from an input that binds it. That one rule covers matrix products, traces, outer products, transposes, axis sums and batched contractions.

```neuro
val a: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val b: Tensor<i32, [3, 2]> = [[1, 0], [0, 1], [1, 1]]

val product: Tensor<i32, [2, 2]> = einsum("ij,jk->ik", a, b)   // `j` is contracted
val rows: Tensor<i32, [2]> = einsum("ij->i", a)                // `j` is summed away
val transposed: Tensor<i32, [3, 2]> = einsum("ij->ji", a)      // both letters survive

val u: Tensor<i32, [2]> = [3, 4]
val v: Tensor<i32, [3]> = [1, 2, 5]
val outer: Tensor<i32, [2, 3]> = einsum("i,j->ij", u, v)       // nothing is contracted
```

A letter repeated **within one operand** walks that operand's diagonal, which makes a trace a subscript rather than a loop. An empty output subscript contracts everything away, and the call produces one number of the element type, the same as a whole-tensor `.sum()`:

```neuro
val square: Tensor<i32, [3, 3]> = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
val u: Tensor<i32, [2]> = [3, 4]

val trace: i32 = einsum("ii->", square)          // 15
val dot: i32 = einsum("i,i->", u, u)             // 25
```

A letter shared by both operands *and* the output batches the contraction. It indexes all three at once instead of being summed:

```neuro
val batch: Tensor<f32, [8, 4, 16]> = Tensor::<f32, [8, 4, 16]>::zeros()
val heads: Tensor<f32, [8, 16, 2]> = Tensor::<f32, [8, 16, 2]>::zeros()

val attended: Tensor<f32, [8, 4, 2]> = einsum("bij,bjk->bik", batch, heads)
```

`...` stands for an operand's **leading axes**, so one subscript serves every batch rank. An operand writes it before its letters, the result writes it where those axes go, and the two are written together or not at all. Every operand that writes `...` covers the same axes, the same spliced shape parameter in a generic function:

```neuro
func scores<S>(q: &Tensor<f32, [..S, 3, 4]>, k: &Tensor<f32, [..S, 5, 4]>) -> Tensor<f32, [..S, 3, 5]> {
    einsum("...qd,...kd->...qk", q, k)
}
```

A result that writes `...` is a tensor even where the ellipsis covers no axes: `einsum("...ii->...", x)` on a `[3, 3]` is a `Tensor<T, []>`, so a generic function returns one kind at every instance. Operands whose `...` axes differ are an error naming both runs, `` `einsum` operand 1 writes `...` over [5], but an earlier operand's `...` covers [2] ``.

**The subscripts are a string literal, never a value.** The compiler reads them during type checking, matches each letter against the operands' shapes, and derives the result type from the letters after `->`. A string computed at run time could not decide a shape that type checking depends on. `einsum` is the one variadic call in the language, and only because the literal fixes its arity.

Like a reduction, `einsum` **reads** its operands. It allocates its own result and leaves the contracted buffers in place, so it is offered on `&Tensor<T, S>` too.

```neuro
func project(w: &Tensor<f32, [2, 3]>, x: &Tensor<f32, [3]>) -> Tensor<f32, [2]> {
    einsum("ij,j->i", w, x)
}
```

A subscript that disagrees with its operands is a compile error **naming the letter**:

- A letter bound to two extents: `` `einsum` binds 'j' to extent 3 and then to 2 ``.
- An output letter no input binds: `` `einsum` writes 'k' on the right of `->` but no operand's subscript binds it ``.
- An output letter written twice: `` `einsum` writes 'i' twice on the right of `->` ``.

The rest are checked the same way: one comma-separated subscript per operand, one letter per axis, every operand a tensor, and every operand sharing one integer or float element type, half precision included. A lettered extent may be an extent parameter, so `einsum("ij,jk->ik", a, b)` serves every `[M, K]` and `[K, N]`, and a letter bound to two different parameters is the same conflict as one bound to two numbers. A letter may walk a dynamic axis when every operand that writes it carries the same name, and the axes `...` covers may be a shape parameter.

A result axis carries no dimension name. A subscript letter is a local label for the contraction, not a name the type keeps. The result's axes are unnamed, and an annotation may name them.

## Functional traversals

`.map`, `.zip` and `.reduce` walk a tensor's elements once, in row-major order, calling a function per element. They differ in what the function receives and what becomes of its answer.

```neuro
val tensor: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
val other: Tensor<f32, [3]> = [4.0, 5.0, 6.0]

val doubled = tensor.map(|x| x * 2.0)              // same shape, elementwise
val paired  = tensor.zip(other, |a, b| a * b)      // same shape, two inputs
val total   = tensor.reduce(0.0, |acc, x| acc + x)
```

A closure literal's parameters need no annotation: each takes the type of the element it receives, and an unannotated accumulator takes the seed's type.

`.map` is the one construct that changes what a tensor's buffer holds. The result's element type is the **function's return type**, not the receiver's, so `t.map(|x: i32| -> f64 { ... })` changes dtype elementwise. That return type must be an integer or a float, half precision included, because the answers go into a buffer. A function returning `bool` is refused: ``.map` builds a tensor out of what its function returns, which requires an integer or a float; this one returns bool``.

`.zip` walks two buffers at one index, so its operand carries the receiver's extents. The two need not hold the same element type, since the function's parameters say what each holds. A `bool` tensor is a valid operand, so it can select as the second argument of a `.zip`.

`.reduce` returns **one value of the seed's type**, not a rank-0 tensor, the way a whole-tensor `.sum()` does, so the number comes out without a second call. The accumulator is the **first** parameter, which is what `|acc, x|` means. An untyped seed takes its type from an annotated accumulator, or from the element type when the accumulator is left open. `0.0` folded over an `f32` tensor is an `f32` either way.

The function is an ordinary function value, so anything that produces one is a stage: a closure literal, a closure binding or a `>>` composition.

```neuro
func halve(x: f32) -> f32 { return x * 0.5 }

func clamp_low(x: f32) -> f32 {
    if x < 0.0 { return 0.0 }
    return x
}

func main() -> i32 {
    val raw: Tensor<f32, [3]> = [1.0, -2.0, 3.0]
    val rescale = halve >> clamp_low
    val clean = raw.map(rescale)
    return 0
}
```

Like a reduction, all three **read** their operands. Each allocates its own result and leaves the walked buffers in place, so all three are offered on `&Tensor<T, S>`, and a chain such as `t.map(f).map(g)` releases its intermediate. The result has the walked shape, so a traversal works over extent and shape parameters, and each instance walks its own length. A dynamic axis is walked at its run-time length.

There is deliberately **no `.filter`**. A filter's output length depends on the values in the buffer, so its result would have no shape the type system can name. Use a `Vec<T>` where a data-dependent length is what you want. The shape-preserving alternative is a boolean mask. A `Tensor<bool, S>` is a tensor like any other and already selects as the second operand of a `.zip`. Nothing produces one yet, though: a traversal cannot answer `bool`, and `a > b` is not defined on two tensors, so a mask must be written out by hand.

## Elementwise math

`.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()` and `.pow(p)` apply a function to every element and return a fresh tensor of the receiver's shape and element type. The same six are methods on every float scalar, `f16` and `bf16` included, where they return that scalar type. None needs an import.

```neuro
val x: Tensor<f64, [3]> = [-1.0, 0.0, 2.0]
val magnitude = x.abs()
val squashed = x.tanh()
val squared = x.pow(2.0)

val a: f64 = 3.0
val b: f64 = 4.0
val hypotenuse = (a * a + b * b).sqrt()
```

`.pow` takes its exponent as a **scalar of the element type**, so an `f32` tensor takes an `f32` and a `bf16` one takes a `bf16` (`h.pow(0.5bf16)`). Any float element type has the methods, half precision included. An integer receiver, scalar or tensor, has none.

Like a reduction, each method **reads** its receiver. It is offered on `&Tensor<T, S>`, and the receiver stays usable afterwards. Out-of-domain inputs follow IEEE 754 and are not diagnosed: `.log()` of `0.0` is `-inf`, `.log()` of a negative number is NaN, and so is `.sqrt()` of a negative number.

In a `@grad` body each one is differentiated by its own rule (see [automatic differentiation](autodiff.md#what-a-grad-body-may-contain)).

## Dynamic shapes

An axis written `?` has no compile-time extent. The body reads it from the tensor's DLPack shape at run time, so one compiled function serves every extent at that position, and every other axis is still checked at compile time. A dynamic extent is not monomorphized.

```neuro
func scores(x: &Tensor<f32, [?B, 3]>, w: &Tensor<f32, [3, 2]>) -> Tensor<f32, [?B, 2]> {
    x @ w
}
```

**Named and anonymous.** `?B` names the extent. Every `?B` in one signature or body is the same run-time value, so two tensors that carry it combine element-wise and under `@` with no check, and inside the scope `B` reads as a `u64` (`for i in 0..B`). A bare `?` is anonymous and equal to nothing, not even another `?`. The element-wise operators, `@`, `einsum` and compound assignment join two axes only when both are the same literal, the same extent parameter or the same name. Any other pairing is a compile error whose hint names `.as_shape` (below). Broadcasting never stretches a dynamic axis.

**Where a name gets its value.** A name is bound the first time a value meets it:

- at a call, from the arguments in parameter order. A later argument with the same name and a different extent panics before the body runs, naming both values;
- at a `val` or `mut` annotation, `val x: Tensor<f32, [?B, 784]> = load()`, from the value, a [`std::safetensors`](standard-library.md#weights-stdsafetensors) read included, whose `?` axes take the stored extents;
- at a constructor, from `dims:` (below);
- at a struct literal. A field's type may name an extent, and every field naming `?B` must agree when the value is built. The name belongs to the value, not to the struct type, so one `Batch` type holds batches of every size, and a field read sees `?`;
- at `.as_shape`, from the receiver (below).

Once a name is bound, a binding site compares any other extent against it at run time and panics on a mismatch. A return value and an assignment do not: there an expected `?B` accepts only `?B`. A call's result names the caller's extent, so `scores(&x, &w)` is a `Tensor<f32, [?N, 2]>` for an `x: Tensor<f32, [?N, 3]>` and a `Tensor<f32, [4, 2]>` for a `[4, 3]` argument. Inside an `Option` or any type other than a tuple or a borrow, a static extent comes back as `?`.

A name may be written at the top of a parameter's tensor type (through one `&` or `&mut`), in a return type when a parameter also carries it, at the top of a `val` / `mut` annotation or a struct field's type, in a constructor's shape and in `.as_shape`. Anywhere else it is a compile error: in a closure's signature, nested in another parameter type (`Vec<Tensor<f32, [?B]>>`), in a type alias, an enum payload or a trait, and in a `@grad`, `@gpu` or `@kernel` signature. Each of those positions takes an anonymous `?`.

**Constructing.** `zeros`, `ones`, `identity` and `random_normal` take one `u64` in `dims:` per anonymous `?` and per name not yet in scope, in order of first occurrence. A name already in scope takes no entry, and `identity` needs one name twice, `[?N, ?N]`:

```neuro
val buf = Tensor::<f32, [?, 128]>::zeros(dims: [n])
val out = Tensor::<f32, [?B, 10]>::zeros()               // B already in scope
```

A constructor with no turbofish also reads a bound name from the type the program writes for it: a `val` or `mut` annotation, the return type of the function or method it returns from, and the annotation of the binding it is assigned to. So `val out: Tensor<f32, [?B, 10]> = Tensor::zeros()` and a tail `Tensor::zeros()` under `-> Tensor<f32, [?B]>` both size `B` from its value. A fresh name there is bound from the value, as any annotation binds one. Anywhere else (a closure's result, an unannotated binding, an argument) a bound name in the expected type is a compile error, and its hint gives the turbofish form.

**What works on a dynamic axis.** Everything a static axis supports, with the checks that depend on an extent moved to run time: `.clone()` and `.to(device)`; indexing, where every position is bounds-checked; slicing, where `..` keeps the axis and a range with constant bounds, `.rev()` and `.step(n)` included, gives a static extent whose end is checked; the reductions and sorting; the shape casts, where `.reshape` compares element counts at run time and a `-1` there is `?`; compound assignment, `@`, `einsum`, `.map` / `.zip` / `.reduce` and the element-wise operators. A reduction over an empty run panics.

**Narrowing.** `.as_shape::<[...]>()` checks a tensor's extents against a written shape at run time and returns `Option<Tensor<T, [...]>>`, consuming its receiver either way. The written shape may use literals, extent parameters, names in scope and a fresh name, which stays in scope after the expression whatever the outcome:

```neuro
val Some(batch) = x.as_shape::<[32, 784]>() else { continue }     // dynamic to static
val Some(rows) = loaded.as_shape::<[?N, 3]>() else { return 1 }   // binds N
```

Static to dynamic needs no call: a `Tensor<f32, [32, 784]>` is accepted where a `Tensor<f32, [?B, 784]>` is expected, binding `B` to 32. The other way round is a type mismatch without `.as_shape`, because the next reader would index the buffer at strides it may not have.

A `?` binds no extent or shape parameter. A call such as `func rows<N>(t: &Tensor<f32, [N, 4]>)` with a dynamic argument is a type mismatch, since nothing supplies `N`. A dimension name may sit on a dynamic axis like any other (`[batch: ?B, embed: 768]`), and names are still compared wherever both shapes supply one.

[`tensor_dynamic_shapes.nr`](../../examples/tensors/tensor_dynamic_shapes.nr) runs each binding site, a constructor and both narrowings; [`dynamic_batches.nr`](../../examples/integration/tensors/dynamic_batches.nr) scores batches of several sizes through one compiled body.

## Device transfer

`.to(device)` **consumes** the tensor and returns one whose buffer lives on the requested device. Its argument is the prelude enum `Device`, which has a `CPU` variant and a `GPU` variant that takes the GPU index, counting from 0.

```neuro
func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val here = a.to(Device::CPU)
    val again = a.to(Device::CPU)         // error: use of moved value 'a': bind a `.clone()` if you need an independent copy
    return 0
}
```

To keep the source, clone first: `t.clone().to(Device::GPU(0))`. A borrow cannot be consumed, so `.to` is not offered on `&Tensor<T, S>`. Calling it there reports that the borrowed type has no such method.

`.to(Device::GPU(n))` copies the elements into the memory of GPU `n` and frees the buffer they were in, whether on the host or on another GPU. `.to(Device::CPU)` copies them back and frees the device buffer. Transferring a tensor to the device it is already on copies nothing.

Outside `@gpu`, an operation on a device tensor runs on that tensor's GPU, and its result stays there. For float and integer tensors this covers `+ - * /` and `@`, compound assignment (`w -= g`), `.sum()`, `.mean()`, `.max()` and `.min()`, `.sort()`, `.argsort()` and `.topk()`, slicing, `.t()` and `.permute(...)`, `einsum`, the elementwise math methods, `.map`, `.zip` and `.reduce` with a closure, reading or writing one element, and `.clone()`. A host operand next to a device operand is copied over for the operation. If every operand is a host tensor, the operation runs on the host as usual. The results are the same values the host computes, with one exception: `.exp()`, `.log()`, `.tanh()` and `.pow(p)` come from the GPU vendor's math library and may differ from the host's in the last bits. `.sqrt()` and `.abs()` are exact everywhere. Integer operations keep the host's checks on the device. An element that overflows in a debug build (`-O0`), or a zero divisor in any build, aborts at the operation with the host's own `panic:` line and location.

```neuro
val g = Tensor::<f32, [8, 8]>::ones().to(Device::GPU(0))
val halved = &g * 0.5f32          // computed on GPU 0, and stays there
val total = halved.sum()          // reduced on GPU 0; the scalar comes back
val first = halved[0, 0]          // copies that one element back
```

A compound assignment updates its target where the target lives, in its own buffer. `w -= &g`, with `w` on the host and `g` on a GPU, copies `w` over for the update and back again. A slice position is checked against its axis before anything runs, with the host's message.

Reading or writing one element copies only that element between the host and the GPU, and waits for the copy to finish. A loop over the elements of a device tensor therefore pays one copy per element. Move the tensor back with `.to(Device::CPU)` before reading many of them.

Some operations have no device form. These are an operation on an `f16`, `bf16` or `bool` tensor, `.map`, `.zip` or `.reduce` whose function uses a construct the GPU path lacks (a tuple, say), calls a function (a function passed by name does), or is passed through a local, and elementwise math or a float `%` when the compiler found no device math library. A tensor reached through a struct field works like any other: `layer.w.sum()` and `layer.w -= g` run where `layer.w` lives. Given a device tensor, each of these aborts at the operation with ``panic: this tensor lives on a GPU, where host code cannot read it: move it back with `.to(Device::CPU)` first``, plus its location. Moving a device tensor, passing it, returning it, storing it in a struct and dropping it all work as they do for a host tensor.

A transfer needs a usable GPU of the vendor the program was built for: NVIDIA, unless [`--gpu-arch`](../guides/cli-usage.md#choosing-a-gpu) names an AMD chip. A program whose only GPU use is `.to` looks for one at its first transfer, not at startup. If none is usable, the transfer aborts with ``panic: `Device::GPU` needs an NVIDIA GPU, and none is usable:`` (or `an AMD GPU`), followed by the reason. `Device::GPU(n)` for a GPU the machine does not have aborts with ``panic: `Device::GPU(n)` names no GPU: this machine has <count>``. Any `neuroc` compiles a transfer, but on Windows a transfer to a GPU aborts.

```neuro
@gpu
func step(state: &Tensor<f32, [8, 8]>, delta: &Tensor<f32, [8, 8]>, rate: f32) -> Tensor<f32, [8, 8]> {
    val moved = delta * rate
    state + moved
}

func main() -> i32 {
    val delta = Tensor::<f32, [8, 8]>::ones() * 0.5f32
    mut state = Tensor::<f32, [8, 8]>::ones().to(Device::GPU(0))
    val resident = delta.clone().to(Device::GPU(0))
    mut i = 0
    while i < 10 {
        state = step(&state, &resident, 2.0f32)   // never leaves the GPU
        i += 1
    }
    val result = state.to(Device::CPU)
    return result[7, 7] as i32                    // 11
}
```

[examples/tensors/tensor_device.nr](../../examples/tensors/tensor_device.nr) runs this program and checks it against the host.

## Running on a GPU: `@gpu`

`@gpu` pins a function's body to the GPU. Each operation in it becomes a kernel. A call copies its host tensor arguments to the device, runs the kernels and copies the result back, so a caller with host tensors still passes and receives ordinary ones. An argument already moved with [`.to(Device::GPU(n))`](#device-transfer) is read where it is, and a call given one runs on that GPU and leaves its result there. A call with only host arguments runs on GPU 0. Arguments on two different GPUs cannot meet in one kernel. The call aborts with ``panic: a `@gpu` call's operands live on GPU 0 and GPU 1: move them to one device with `.to(Device::GPU(n))` first``.

```neuro
@gpu
func blend(a: &Tensor<f32, [4, 6]>, b: &Tensor<f32, [4, 6]>, t: f32) -> Tensor<f32, [4, 6]> {
    val scaled = a * t
    scaled + b
}
```

Bare `@gpu` requires a GPU, and the compiler never runs the body anywhere else. A body the GPU path cannot lower is a compile error at the function: `` `@gpu` function 'f' cannot become a GPU kernel ``. The GPU path takes straight-line tensor code over `f32`, `f64` or integer tensors of static shape and rank 1 or more. It accepts element-wise `+ - * /`, `@`, `.sum()` / `.mean()` / `.max()` / `.min()`, `.sort()` / `.argsort()` / `.topk()`, the elementwise math methods, slices at literal positions, `.t()` / `.permute(...)` and `einsum`. Parameters may be scalars or tensors, owned or `&`, and the result is a tensor. A generic function qualifies per instance. Integer arithmetic keeps the host's overflow and zero-divisor checks. Elementwise math other than `.abs()` also needs the GPU vendor's device math library when compiling (libdevice from the CUDA toolkit, or ROCm's).

A program with a `@gpu` function checks for a usable GPU when it starts, before `main`. If there is none, it prints ``panic: `@gpu` needs an NVIDIA GPU, and none is usable:`` (or `an AMD GPU`), followed by the reason, and aborts.

The attribute goes on a free function. The checker refuses it on a method (`` `@gpu` on a method is not supported yet ``) and refuses any argument other than `fallback:` (`` `@gpu` takes only `fallback: true` or `fallback: false` ``). A `@grad` body cannot differentiate through a `@gpu` call, with or without a fallback, unless the callee is also `@no_grad`, because the derivative would be computed on the host.

Compiling `@gpu` needs a Linux `neuroc`. On any other platform `neuroc` refuses a bare `@gpu` function. By default the kernels are for NVIDIA and ship as PTX that the CUDA driver compiles for the GPU it finds. The machine that compiles therefore needs no CUDA toolkit, except for the elementwise math noted above, and the machine that runs needs only the NVIDIA driver. `--gpu-arch gfxNNN` builds for that AMD chip instead. That needs ROCm to compile and HIP to run (see [Choosing a GPU](../guides/cli-usage.md#choosing-a-gpu)). [examples/tensors/tensor_gpu.nr](../../examples/tensors/tensor_gpu.nr) runs the `blend` example above.

### A CPU fallback: `@gpu(fallback: true)`

`@gpu(fallback: true)` builds the body twice, as kernels and as ordinary host code. The program picks one when it starts: the kernels if it finds a usable GPU, the host copy if not. Every call in a run takes the same path, and both give the same result.

```neuro
@gpu(fallback: true)
func hidden(x: &Tensor<f32, [4, 3]>, w: &Tensor<f32, [3, 5]>, b: &Tensor<f32, [5]>) -> Tensor<f32, [4, 5]> {
    val product = x @ w
    product + b
}
```

The body still has to be one the GPU path can lower, because the host copy is built only beside a kernel. The value must be the literal `true` or `false`. `fallback: false` means bare `@gpu`. A program that also has a bare `@gpu` function still aborts at startup without a GPU, because that function has no host copy.

Off Linux, `neuroc` compiles a fallback function to its host copy only, and warns that it always runs on the host. [examples/showcase/gpu_fallback.nr](../../examples/showcase/gpu_fallback.nr) uses two fallback layers and prints the same output with or without a GPU.

## Writing a kernel: `@kernel`

`@kernel(threads: [...])` runs a function's body once per GPU thread. Where `@gpu` turns whole-tensor operations into kernels for you, a kernel is the per-thread code itself. The launch has one thread per element of the first `KernelOut` parameter, the grid tensor, in blocks shaped by `threads`. That is one to three positive integer literals, one per axis of the grid tensor, at most 1024 threads a block.

```neuro
@kernel(threads: [16, 16])
func add_relu(a: Tensor<f32, [37, 45]>, b: Tensor<f32, [37, 45]>, out: KernelOut<Tensor<f32, [37, 45]>>) {
    val row = thread_id.x
    val col = thread_id.y
    if row < 37 && col < 45 {
        val sum = a[row, col] + b[row, col]
        // SAFETY: thread (row, col) is the only one to write out[row, col].
        unsafe { out[row, col] = if sum > 0.0 { sum } else { 0.0 } }
    }
}
```

Inside the body, `thread_id` is the thread's position in the whole grid and `block_id` is its block's. Each has three `u64` fields, `.x`, `.y` and `.z`, so they compare with an extent directly. A field is read directly (`thread_id.x`), or through a `val` that binds the position whole (`val t = thread_id`, then `t.x`). Used as a value in any other way, either form is a compile error. `thread_id.x` is `block_id.x` times the block's `x` size, plus the thread's place in its block. It indexes the grid tensor directly, and an axis the grid does not have reads 0. Neither name exists outside a kernel body, and a local of the same name hides it.

Each axis runs enough blocks to cover its extent, rounded up. The 37 × 45 tensor above gets a 3 × 3 grid of 16 × 16 blocks, and the threads past the edge have no element. A GPU runs at most 2^31 - 1 blocks along `x` and 65535 along `y` and `z`, so a grid that needs more along an axis is a compile error naming the axis. A body guards its writes, as `row < 37 && col < 45` does. Every index is still checked when the kernel runs. A thread that reads or writes past an extent, divides an integer by zero or, in a debug build, overflows an integer stops the kernel. The program then aborts at the call with a `panic:` naming the GPU error.

A kernel takes tensors it reads as `Tensor<T, S>`, tensors it writes as `KernelOut<Tensor<T, S>>`, and numbers or `bool`s by value. Every tensor has a static shape. The kernel returns nothing: what it computes is what it writes.

The call borrows every tensor it passes. An input is passed bare, as in `add_relu(a, b, &mut out)`, and the caller still owns it afterwards. A temporary such as `a + b` is bound for the call and dropped after it. Writing `&a` is an error (`` `@kernel` call borrows its `Tensor` inputs itself: pass the tensor, not `&` ``). A kernel is called by name and cannot be stored as a function value. An output is passed as `&mut out`, and the compiler turns that borrow into the kernel's `KernelOut` handle. One call may not pass the same tensor as an input and an output. A call copies each host tensor to the GPU and copies each output back after the kernel has run. A tensor already moved with [`.to(Device::GPU(n))`](#device-transfer) is used where it is, and an output is written in place.

Inside the body, a `KernelOut` handle is only ever indexed (`out[i, j] = v`, `out[i] += v`, or a read of one element) or partitioned (below). Indexing must sit in an `unsafe { }` block. The compiler cannot prove that no other thread writes the same element, so `unsafe` marks each place where you vouch for it, and searching a kernel for `unsafe` finds every one. A read needs it too, since it can race with another thread's write to that element. `partition` is the safe alternative. The handle cannot be bound to a local, borrowed, passed to a function, captured by a closure or returned. `KernelOut` is not a type anywhere except a kernel's parameter list. An input is read-only, and writing one of its elements is an error.

### Writing through `partition`

An index write puts a value wherever its index says, and nothing stops two threads writing the same element. `out.partition(|base, slice| { ... })` gives each thread its own run of `out` instead, so no two threads meet:

```neuro
@kernel(threads: [4])
func scale_rows(m: Tensor<f32, [6, 5]>, factor: Tensor<f32, [6]>, totals: KernelOut<Tensor<f32, [6]>>, scaled: KernelOut<Tensor<f32, [6, 5]>>) {
    scaled.partition(|base, row| {
        val f = factor.flat(base / row.len())
        for i in 0u64..row.len() {
            row[i] = m.flat(base + i) * f
        }
    })
    totals.partition(|base, total| {
        mut sum = 0.0f32
        for k in 0u64..5u64 {
            sum += m.flat(base * 5u64 + k)
        }
        total[0] = sum * factor.flat(base)
    })
}
```

The threads that share `out` each own an element of the grid tensor, and a thread's number is the row-major position of that element. With `k` elements of `out` per thread, thread `n` gets elements `n * k` up to `(n + 1) * k` in row-major order. `slice` is that run: a `&mut [T]` indexed from 0 up to `slice.len()` and bounds-checked against the run. `base` is the `u64` position of its first element. [`.flat(base + i)`](#reading-by-row-major-position) reads the input element that `slice[i]` corresponds to. A thread in the overhang owns no grid element and runs nothing, so the body needs no `row < M` guard. Partitioning the grid tensor itself gives every thread one element.

The element count of `out` must be a multiple of the grid tensor's, and the compiler refuses a partition where it is not: `` `@kernel` output 'out' has 6 elements, which the grid's 4 threads cannot share equally ``. The argument is one closure of two parameters, whose types may be left out (`u64` and `&mut [T]` are the only ones it takes). It returns nothing, and a `return` in it ends the closure, not the kernel. The closure may read the kernel's inputs and scalars, but it cannot name a `KernelOut` handle. Inside it, `slice` is only indexed or asked for its `.len()`.

The body may use locals, arithmetic and comparisons, casts, `&&` and `||`, and `if` (as a statement or with a value). It may use `match` on an integer, `bool` or `char` (literal, range and `_` patterns, guards), `while`, `loop` (with a `break` value too), and `for` over a range (`.rev()` and `.step(n)` included). It may also use `break`, `continue` and `return`, the integer methods `.wrapping_*`, `.saturating_*` and `.shr(n)`, shifts with `<<`, `.is_nan()`, reads and writes of single tensor elements, `.flat(i)` and `partition`. Integer arithmetic follows the build's tier, as on the host. An overflow stops the kernel in a debug build (`-O0`) and wraps from `-O1` up. Anything else, such as a function call, is a compile error at that construct: ``` `@kernel` function 'k' cannot lower a function call to the GPU```.

The attribute goes on a free function, never beside `@gpu` or `@grad`. A kernel needs a GPU and a Linux `neuroc`, exactly as bare `@gpu` does. [examples/tensors/tensor_kernel.nr](../../examples/tensors/tensor_kernel.nr) runs the first example above, and [examples/tensors/kernel_partition.nr](../../examples/tensors/kernel_partition.nr) runs the second. [examples/showcase/heat_diffusion.nr](../../examples/showcase/heat_diffusion.nr) steps a simulation with two kernels over tensors that stay on the GPU.

## What tensors cannot do yet

These are the limits a reader is most likely to hit. Each one is described in full in the section linked.

- On a GPU tensor with a dynamic axis, compound assignment and `.map` / `.zip` / `.reduce` run on the host only, so they panic; every other operation runs on the device. A `@grad`, `@gpu` or `@kernel` signature cannot name a dynamic axis (see [Dynamic shapes](#dynamic-shapes)).
- There is no `.filter`, and no operation produces a `Tensor<bool, S>` mask, so a mask is written by hand (see [Functional traversals](#functional-traversals)).
- `@gpu` is not accepted on a method (see [Running on a GPU](#running-on-a-gpu-gpu)).
- Operations on `f16`, `bf16` and `bool` tensors have no device form (see [Device transfer](#device-transfer)).

## See also

- [Types](types.md): the scalar element types a tensor holds, and the arrays that coerce to tensors
- [Generics and traits](generics-and-traits.md#generic-functions): how shape parameters are monomorphized
- [Operators](operators.md): the arithmetic and compound-assignment families tensors implement
- [Automatic Differentiation](autodiff.md): `@grad` over tensors, `.backward()` and `.detach()`
- [Memory model](memory-model.md): when a tensor's buffer is released
- [CLI usage](../guides/cli-usage.md): `--gpu-arch` and emitting object files
- [examples/tensors/](../../examples/tensors/): a runnable program per feature on this page
