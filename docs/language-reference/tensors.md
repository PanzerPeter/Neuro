# Tensors

`Tensor<T, [d0, d1, ...]>` is a tensor: the element type and every extent are part of the
type, and an extent is ordinarily known at compile time. An axis may instead be written
`?`, which defers that one extent to run time (see [Dynamic shapes](#dynamic-shapes)).

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

The shape is written as a bracketed list of non-negative integer literals, of shape
parameters inside a generic definition (see [Shape generics](#shape-generics)), or of `?`
for a dynamic axis. An empty list `[]` is the rank-0 scalar tensor. The element must be a fixed-width scalar: any
integer type, `f16` / `bf16` / `f32` / `f64`, or `bool`.

Rank and every extent are part of the type, so `Tensor<f32, [2, 2]>` and
`Tensor<f32, [3, 3]>` are different types and a mismatch is a compile error naming both:

```neuro
func takes_square(t: Tensor<f32, [3, 3]>) { }

func pass_through(t: Tensor<f32, [2, 2]>) {
    takes_square(t)                       // error: expected Tensor<f32, [3, 3]>,
}                                         //        found Tensor<f32, [2, 2]>
```

A tensor owns its buffer, so it is **not** `Copy`. Passing one to a function moves it;
pass `&Tensor<T, S>` or `&mut Tensor<T, S>` to share it.

```neuro
func consume(t: Tensor<f32, [2, 2]>) { }

func twice(t: Tensor<f32, [2, 2]>) {
    consume(t)
    consume(t)                            // error: use of moved value 't'
}
```

`.clone()` is the explicit way to get a second owner. It takes no arguments, yields a
tensor of the same type, and leaves the receiver usable, including when it is called
through a borrow, where the result is an owned tensor rather than the borrow.

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

## Device transfer

`.to(device)` **consumes** the tensor and returns one whose buffer lives on the requested
device. Its argument is the prelude enum `Device`:

```neuro
enum Device {
    CPU,
    GPU(i32)                              // GPU index
}
```

```neuro
func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val here = a.to(Device::CPU)
    return 0                              // `a` is moved: using it here is an error
}
```

To keep the source, clone first: `t.clone().to(Device::CPU)`.

A borrow cannot be consumed, so `.to` is not offered on `&Tensor<T, S>`: calling it there
reports that the borrowed type has no such method.

The host is the only device this compiler can lower to today; the GPU backend is later
work. `.to(Device::CPU)` is therefore the move itself and copies nothing, and a transfer to
any other device aborts at run time with a diagnostic rather than quietly leaving the
buffer on the host.

`Tensor` is a prelude name rather than a keyword, so a module declaring its own
`Tensor` shadows it; a shape argument is what marks a type application as a tensor, and
writing one under any other name is a parse error.

## Building a tensor

A nested array literal becomes a tensor wherever an explicit `Tensor<...>` annotation is
in scope. The annotation supplies the element type and every extent, and it types the
literal's leaves: `1.0` under a `Tensor<f32, ...>` annotation is an `f32` literal, not
an `f64` one being narrowed, exactly as `val x: f32 = 0.01` types its literal.

```neuro
val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]

val m: Tensor<f32, [2, 3]> = [
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0]
]

val arr = [1.0, 2.0, 3.0]          // no annotation: a plain [f64; 3], not a tensor
```

A nested literal must be **rectangular**: every sub-array at a given depth has the length
the corresponding extent declares, and the nesting is as deep as the shape is long. A
ragged literal, a wrong extent, and a literal shallower than the shape are all compile
errors. A value that already has a type is not converted for the annotation's benefit: a
non-literal element must already be the element type.

Where no annotation reaches, name the type with a turbofish and use a constructor:

```neuro
val zeros = Tensor::<f32, [3, 3]>::zeros()
val ones  = Tensor::<f32, [3, 3]>::ones()
val eye   = Tensor::<f32, [4, 4]>::identity()
val w     = Tensor::<f32, [128, 64]>::random_normal(mean: 0.0f32, std: 0.02f32)
val v     = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])
val loss: Tensor<f32, []> = Tensor::scalar(0.5)
```

`identity()` applies only to a square rank-2 shape, `random_normal` draws only into `f32`
or `f64`, `scalar` builds only the rank-0 tensor, and `from` takes the same nested literal
the annotated form coerces. A rank-0 tensor has no array-literal form at all: it is
written with `Tensor::scalar(value)`. The generator behind `random_normal` is seeded from a
fixed constant, so a compiled program draws the same values on every run.

A tensor **owns** its buffer, and that buffer lives out of line. The value itself is a
[DLPack](https://dmlc.github.io/dlpack/latest/) handle: a pointer to a
`DLManagedTensorVersioned` whose `data` field addresses a flat, row-major run of the
elements, allocated when the tensor is constructed and released when its binding leaves
scope. The handle carries the tensor's rank, shape, strides, element dtype, and device, so
the pointer a Neuro program passes around is the pointer a foreign consumer such as NumPy or
PyTorch reads: nothing is wrapped or converted at the boundary. Release runs through the
handle's own `deleter`, which is the single release path: a tensor leaving scope and a
foreign owner of the handle call the same function.

The buffer keeps one address for its whole life and is aligned to 64 bytes, which is what
DLPack requires; a tensor of any size compiles at every optimization level. `.clone()`
allocates a second handle and a second buffer and copies into it, so the copy is independent
of the original. The buffer is host memory: the handle reports the `kDLCPU` device, and
device placement is later work.

A tensor moves like any other non-`Copy` value, and the move hands the buffer on rather than
copying it: binding it, passing it to a function, returning it, storing it in a struct
field, and `.to(device)` all transfer ownership, and only the last owner releases it. A
tensor held in a struct field is not released when the struct goes out of scope; that gap is
shared with the standard collections.

## Updating a tensor in place

The compound assignment operators `+=`, `-=`, `*=`, `/=` and `%=` update a `mut` tensor's
own buffer element by element. They do not desugar to `w = w OP g` the way they do for
every other type, so nothing is allocated and the tensor's DLPack handle and `data` pointer
are unchanged across the statement.

```neuro
mut w = Tensor::<f32, [784, 128]>::random_normal(mean: 0.0f32, std: 0.02f32)
val step = Tensor::<f32, [784, 128]>::zeros()

for i in 0..8 {
    w -= &step                            // in place: one buffer for the whole loop
}
```

The operand is a tensor of the same element type and shape, owned or borrowed: `w += g`
consumes `g`, while `w += &g` reads it, so one operand can serve every iteration of a loop.
The operand is evaluated before the target is borrowed for the update. The element type
must have arithmetic (any integer, `f32`, or `f64`), and element arithmetic carries the
same guards the scalar operator does. See
[Compound Assignment Operators](operators.md#compound-assignment-operators).

## Slicing and indexing

A tensor index gives **one argument per axis**, and each argument is a position, a range,
or the whole axis `..`. An axis given a position is **dropped** from the result; an axis
given a range or `..` **survives** at the extent that range names. So naming every axis
with a position reads one element, and anything else builds a smaller tensor.

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

A **position** may be any integer expression, including one only known at run time, which
is what lets a loop walk a tensor. A **range bound** must fold to a compile-time constant:
the extent it produces is part of the result's type, and a type cannot wait for a value.
`t[0..k]` with a `mut k` is therefore a compile error naming that rule.

A constant position outside its axis, a range whose start is past its end, and a range
reaching past the extent are all compile errors. A run-time position is bounds-checked on the debug tier,
the same tier an array index sits on: it panics in a debug build and the check is omitted
under `-O 1` and above.

A range argument may wear `.rev()`, which reads that axis back to front. Only the order
changes: the surviving extent is the range's either way, and a sub-range reverses within
its own bounds rather than the axis's.

```neuro
val samples: Tensor<i32, [5]> = [10, 20, 30, 40, 50]

val newest: Tensor<i32, [5]> = samples[(0..5).rev()]   // 50, 40, 30, 20, 10
val middle: Tensor<i32, [3]> = samples[(1..4).rev()]   // 40, 30, 20
```

Each axis decides its own direction, so `m[(0..2).rev(), ..]` flips the rows and leaves
each row's own order alone. The same `.rev()` reverses a `for` range; see
[Iterating Backwards](control-flow.md#iterating-backwards-rev).

A slice is a **fresh owned tensor holding a copy**, not a view into the source. A tensor
owns its buffer and releases it through its own DLPack deleter, so two tensors never share
one buffer; the slice may be sliced again, cloned, passed by value, and returned, and it
is freed at the end of its own scope. Indexing **reads** its receiver rather than
consuming it, and it reads through a borrow, so `t[i, j]` on a `&Tensor<T, S>` parameter is
how a borrowed weight is inspected.

Range indexing is a tensor form. An array or a `Vec` takes one integer index and offers
`.slice(a..b)` for a sub-range; writing `xs[0..2]` on one reports that.

## Shape generics

A tensor extent may be a **generic parameter** rather than a literal, so one function
serves every shape it is written for. A bare name in a shape position is sugar for a
`const NAME: u32` parameter: it is a compile-time *value*, inferred from the shape of the
argument the caller passes, and each distinct set of extents is monomorphized into its own
specialization.

```neuro
func corner<M, K>(t: &Tensor<i32, [M, K]>) -> i32 {
    return t[0, 0]
}

val wide: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val tall: Tensor<i32, [4, 2]> = Tensor::<i32, [4, 2]>::ones()
val a = corner(&wide)                     // instantiated at M = 2, K = 3
val b = corner(&tall)                     // and again at M = 4, K = 2
```

A parameter written more than once must be the same extent everywhere, which is what makes
a shape mismatch a compile error rather than a wrong answer at run time:

```neuro
func pair<M, N, K>(a: &Tensor<i32, [M, K]>, b: &Tensor<i32, [K, N]>) -> i32 {
    return a[0, 0] + b[0, 0]
}

val x: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
val y: Tensor<i32, [5, 6]> = Tensor::<i32, [5, 6]>::zeros()
val z = pair(&x, &y)   // error: shape parameter 'K' is already 3 here,
                       //        but this argument makes it 5
```

A shape parameter is an ordinary const parameter, so everything const parameters already do
applies: it can be read as a value in the body, it can be constrained by a `where` predicate
checked at the call that supplies the offending extent, and it may be spelled out in full as
`const K: u32` where that reads better.

```neuro
func last<N>(t: &Tensor<i32, [N]>) -> i32
where N > 0
{
    return t[N - 1]
}
```

An extent also flows into the **return** type, so a function may hand back a shape it
derived from its argument's:

```neuro
func top_row<M, K>(t: &Tensor<i32, [M, K]>) -> Tensor<i32, [K]> {
    return t[0, ..]
}
```

Two things a shape parameter does not do. A tensor **literal** cannot be written against a
symbolic extent (one literal would have to serve every instantiation, so there is no length
to check it against): build the tensor with a constructor instead. And a name that no
enclosing signature declares is an error naming the dimension, not a silently accepted
extent.

## Named dimensions

An axis may carry a name before its extent. The name is part of the type, so a signature
says which axis is which, and the compiler checks the claim:

```neuro
func batch_norm(
    x: &Tensor<f32, [batch: 32, channels: 3, height: 224, width: 224]>
) -> i32 {
    return 32
}
```

A name does not create a new type. Two tensor types agree when their element types agree,
their ranks agree, and each pair of extents agrees; a name is compared only at a position
where **both** sides write one. So `Tensor<f32, [3, 224, 224]>` and
`Tensor<f32, [channels: 3, height: 224, width: 224]>` are interchangeable, and a function
written before the names existed still takes a named tensor:

```neuro
val plane: Tensor<i32, [height: 2, width: 3]> = [[1, 2, 3], [4, 5, 6]]
val same:  Tensor<i32, [2, 3]> = plane          // fine: one side names nothing
```

What is rejected is the transposition the feature exists to catch — the same extents, the
wrong way round:

```neuro
func normalize(x: Tensor<f32, [height: 4, width: 4]>) { }

val t: Tensor<f32, [width: 4, height: 4]> = Tensor::<f32, [4, 4]>::zeros()
normalize(t)   // error: tensor axis 0 is named 'height' here but 'width'
```

The extents are identical, so nothing else would have caught it.

Names live in the tensor type's own namespace rather than the surrounding scope: a local
variable called `height` neither shadows an axis nor collides with one. Each axis of one
shape needs its own name (`[side: 4, side: 4]` is an error), and the name is not a generic
parameter — in `[batch: N]`, `batch` names the axis and `N` is the shape parameter the call
infers. An axis keeps its name through an index, at whatever extent survives:

```neuro
val row: Tensor<i32, [width: 3]> = plane[1, ..]   // `height` was dropped, `width` kept
```

Names are checked and then erased: a named tensor compiles to exactly the code the unnamed
one does. They are read back by the two name-driven operations `.permute([height, width])`
and `.flatten(dims: [...])`, described under
[Rearranging a shape](#rearranging-a-shape).

## Rearranging a shape

Four methods build a tensor of a different shape from the receiver's elements. Each one's
result shape is computed at compile time, so what comes back is as statically shaped as
what went in.

```neuro
val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]

val t: Tensor<i32, [3, 2]> = m.t()                 // matrix transpose
val flat: Tensor<i32, [6]> = t.reshape([-1])       // same order, new extents
```

| Method | Result |
|---|---|
| `.t()` | the rank-2 transpose; any other rank is an error suggesting `.permute` |
| `.reshape([d0, ...])` | the same elements at new extents; `-1` in at most one position takes whatever extent the others leave over |
| `.permute([...])` | the axes reordered; every axis named exactly once |
| `.flatten()` | every axis merged into one |
| `.flatten(dims: [...])` | an adjacent run of axes merged into one, the rest untouched |

All four **consume** the receiver. A tensor owns its buffer, so handing that buffer on is
what a move means here; a transpose that quietly left a second copy of a weight matrix
alive is exactly what move-by-default exists to prevent. Use `.clone()` where the original
has to stay readable, and note that a borrow cannot be consumed, so none of the four is
offered on `&Tensor<T, S>`.

```neuro
val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
val t = m.clone().t()
val first = m[0, 0]                       // fine: the clone was consumed, not `m`
```

`.permute` and `.flatten` take **dimension names** as well as positions, which is what the
names in a shape are for. A name resolves against the receiver's own shape, never against
the surrounding scope, so a local called `height` neither shadows the axis nor is shadowed
by it; naming a dimension the shape does not declare is an error listing the ones it does.

```neuro
val image: Tensor<i32, [channels: 3, height: 2, width: 4]> = Tensor::<i32, [3, 2, 4]>::ones()
val hwc = image.permute([height, width, channels])    // [height: 2, width: 4, channels: 3]

val batch: Tensor<i32, [batch: 2, seq_len: 3, embed: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
val tokens = batch.flatten(dims: [seq_len, embed])    // [batch: 2, 12]
```

`.t()` and `.permute` carry each axis's name along with it, so a transposed
`[height: H, width: W]` is a `[width: W, height: H]` and is still rejected where the
original was expected. A reshaped extent is not the axis its old name documented, so
`.reshape` and a merged `.flatten` axis are unnamed.

The errors are compile-time: a `.reshape` that would change the element count names both
counts, a `-1` that no whole extent satisfies is rejected, a `.permute` that names an axis
twice or leaves one out is rejected, and `.flatten` requires its axes to be adjacent,
because flattening re-describes the element order rather than moving elements. A receiver
whose extent is a shape parameter has no element count to check against, so `.reshape` and
`.flatten` are not available inside a shape-generic function; `.t()` and `.permute` are,
since they only reorder axes.

## Reductions

A reduction folds a tensor's elements. Written with no argument it folds all of them and
produces one scalar of the element type; written with `axis:` it folds along that axis
alone, which drops that axis from the result and leaves every other one — extent and
dimension name both — exactly as it was.

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
| `.mean()` / `.mean(axis: k)` | the arithmetic mean; `f32`/`f64` elements only |
| `.max()` / `.min()`, with or without `axis:` | the largest / smallest element |

Unlike the shape casts, a reduction **reads** its receiver. It produces a scalar, or
allocates a fresh and smaller tensor, and leaves the buffer it summarised where it was, so
it is offered on `&Tensor<T, S>` too: a weight can be summarised without being moved out of
whatever owns it.

```neuro
func spread(scores: &Tensor<i32, [4]>) -> i32 {
    scores.max() - scores.min()
}

val scores: Tensor<i32, [4]> = [4, 9, 2, 7]
val range = spread(&scores)
val total = scores.sum()                  // fine: `scores` was never moved
```

The axis may be written as a position, as a **dimension name** the receiver's type
declares, or as a negative index counting from the end, so `axis: -1` is the last axis
whatever the rank is. A name resolves against the receiver's own shape, the same rule
`.permute` follows.

```neuro
val frame: Tensor<f64, [height: 2, width: 3]> = [[0.0, 3.0, 6.0], [1.0, 4.0, 9.0]]

val column_means: Tensor<f64, [width: 3]> = frame.mean(axis: height)
val row_peaks: Tensor<f64, [height: 2]> = frame.max(axis: -1)
```

Reducing a rank-1 tensor along its only axis leaves the rank-0 `Tensor<T, []>`, the shape
`Tensor::scalar` builds.

Three rules are compile-time errors. The element type must be an integer or `f32`/`f64`: a
`bool` tensor has nothing to fold. `.mean()` narrows that to `f32`/`f64`, because an
integer mean would have to pick a rounding rule the language does not give — sum and divide
explicitly instead. And a reduction over **no** elements is rejected outright rather than
given an identity value, since `.max()` of nothing has no answer. A receiver whose extent is
a shape parameter or a `?` has no run length either, so a reduction needs a tensor whose
shape is numbers.

## Sorting and selection

`.sort()`, `.argsort()` and `.topk()` order one axis of a tensor. They differ only in what
they hand back: the elements in that order, the receiver positions that produce that order,
or the leading `k` of both.

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

`axis:` takes a position, a dimension **name** the receiver's type declares, or a negative
index counting from the end, and defaults to the last axis — the row of a matrix. `.topk`
selects the `n` **greatest**, so it has no `descending:` of its own; its selected axis is
`n` long in both results and carries no dimension name, a truncated axis no longer being
the thing its name documented.

An `argsort` entry is an ordinary integer, so it reads back into the tensor that produced
it, which is what a reduction cannot do: `.max()` answers *what* the best element was and
never *where*.

```neuro
val order: Tensor<i32, [4]> = v.argsort(descending: true)
val best = order[0] as u64
val value = v[best]
```

These methods are **native to the element dtype**. `f32` and `f64` sort directly, without
wrapping every element in an ordered-float type first, and the comparator is IEEE-754
ordered with one rule: `NaN` sorts to the **end**, whether the order is ascending or
descending. Real workloads treat `NaN` as invalid, so pushing it to the back leaves the
best candidates at the front where top-k expects them. A program that wants the "partial
order returns nothing on `NaN`" semantics maps through an ordered-float wrapper first.

Equal elements keep the order they were written in, which is what makes an `argsort` of a
tensor with ties reproducible.

Like a reduction, a selection **reads** its receiver: it allocates its own result and
leaves the ordered buffer where it was, so it is offered on `&Tensor<T, S>` too.

`k:` and `descending:` must be constants, because the result's shape and the comparator are
both settled before any element is read; `k` must lie between `1` and the sorted axis's
extent. The element type must be an integer or `f32`/`f64`, the receiver must have at least
one axis, and its shape must be numbers rather than shape parameters or `?`.

## Dynamic shapes

An axis written `?` has no compile-time extent. It opts that one axis out of
compile-time shape checking and leaves every other axis checked exactly as before, so one
signature serves every extent at that position:

```neuro
func embed_width(batch: &Tensor<f32, [?, 4]>) -> i32 {
    return 4
}

val pair: Tensor<f32, [2, 4]>  = [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]]
val seven = Tensor::<f32, [7, 4]>::zeros()

embed_width(&pair)                        // ok
embed_width(&seven)                       // ok, same function
```

A dimension name may sit on a dynamic axis like any other (`[batch: ?, embed: 768]`), and
the name rule is unchanged: compared wherever both shapes supply one, so a transposed
argument is still rejected even when the extents say nothing.

**The `?` is an expectation, not a value.** A statically shaped tensor is accepted where a
`?` axis is expected; the reverse is not, because a dynamic tensor's run-time shape could
be anything and a static annotation would let the next reader index it at strides its
buffer may not have:

```neuro
func widen(t: Tensor<f32, [2, 4]>) -> Tensor<f32, [?, 4]> {
    return t                              // ok: widening
}

val back: Tensor<f32, [2, 4]> = widen(Tensor::<f32, [2, 4]>::zeros())
//                              error: expected Tensor<f32, [2, 4]>,
//                                     found Tensor<f32, [?, 4]>
```

For the same reason a `?` binds no shape parameter: a call to
`func rows<N>(t: &Tensor<f32, [N, 4]>)` with a dynamic argument leaves `N` uninferable.

A `?`-shaped tensor binds, moves, crosses a call boundary, is returned, and is released at
scope exit like any other, because a tensor value is a DLPack handle and none of that
needs an extent. What does need one is a compile error naming the axis: the construction
helpers and tensor literals (no size to allocate), `.clone()` and `.to(device)` (no size to
copy), indexing and slicing (no strides), the four shape casts (no element count), and
in-place compound assignment. Build such a tensor at a static shape and pass it where the
`?` is expected.

## What tensors cannot do yet

A tensor can be built, bound, moved, cloned, passed, returned, transferred with
`.to(device)`, updated in place, stored in a struct, indexed, sliced, reshaped with
`.t()` / `.reshape(...)` / `.permute(...)` / `.flatten(...)`, and reduced with
`.sum()` / `.mean()` / `.max()` / `.min()`. What is still
later work is writing through an index (`t[i, j] = v`), by-value tensor arithmetic
(`a + b`, `a @ b`), the functional `.reduce(init, |acc, x| ...)`, and the step index form
(`t[(0..n).step(2)]`), which waits on `.step(n)` existing on ranges at all. The reverse
form `t[(0..n).rev()]` is implemented. A dynamic `?` axis is accepted, but only as a widening: nothing that needs
an extent works on one, and there is no run-time shape check that would let a `?` be
narrowed back to a literal. Symbolic
extents are accepted on functions: a shape-generic struct, enum, or `impl` block is
later work, so a shape parameter is a function's to declare.

## References

- [Types](types.md): the scalar element types a tensor holds, and the arrays it coerces from
- [Functions](functions.md#generic-functions): how shape parameters are monomorphized
- [Operators](operators.md): the compound-assignment family tensors implement
- [examples/tensors/](../../examples/tensors/): a runnable program per feature on this page
