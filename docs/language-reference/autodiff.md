# Automatic Differentiation

`@grad` on a function asks the compiler for its derivative. The derivative is produced while
compiling, as ordinary code placed beside the function: nothing records a graph while the
program runs, and there is no gradient tape.

```neuro
@grad
func squared_error(w: &mut Tensor<f32, [2, 1]>, b: &mut Tensor<f32, [1]>, scale: f32) -> Tensor<f32, []> {
    val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
    val y: Tensor<f32, [4, 1]> = [[6.0], [3.0], [3.0], [6.0]]
    val prediction = x @ w
    val shifted = prediction + b
    val residual = shifted - y
    val squares = &residual * &residual
    return Tensor::scalar(squares.mean() * scale)
}
```

This function comes from [`examples/showcase/gradient_loss.nr`](../../examples/showcase/gradient_loss.nr),
which trains it by gradient descent.

## What `@grad` produces

The annotated function is compiled exactly as written and stays callable as usual. Beside it
the compiler generates a derivative function. It takes the same parameters and returns the
loss together with a bundle holding **one gradient per differentiated parameter**. Each
gradient is an owned tensor with the parameter's own element type and shape: above, the
gradient for `w` is a `Tensor<f32, [2, 1]>` and the gradient for `b` is a
`Tensor<f32, [1]>`.

The derivative is reverse-mode. It evaluates the loss once, then walks the computation
backwards from the loss and adds up each value's contribution wherever it was used. This is
where tensor shapes matter: a `[1]` bias broadcast down four rows gets its gradient summed
back over those rows.

## Running the derivative: `.backward()`, `.grad()`, `.zero_grad()`

The derivative runs when the loss a `@grad` call returned is asked for it. `.backward()` runs
it and moves each gradient into a slot beside the tensor it belongs to, `.grad()` reads that
gradient, and `.zero_grad()` empties the slot. A training step from
[`examples/showcase/gradient_loss.nr`](../../examples/showcase/gradient_loss.nr):

```neuro
        pool {
            val loss = squared_error(&mut w, &mut b, 1.0f32)
            // From the call to this line `w` and `b` stay mutably borrowed, because
            // this is where their gradients are written.
            loss.backward()
            w -= RATE * w.grad()
            b -= RATE * b.grad()
            w.zero_grad()
            b.zero_grad()
            measured = loss.sum()
        }
```

The spelling is familiar from tape-based frameworks, but there is no tape. The derivative runs
where the call ran, on the arguments the call saw, and the loss is computed once. A `@grad`
call whose result never meets a `.backward()` runs no derivative at all.

- **`.backward()`** is called on a `val` bound directly to a `@grad` call's result, in the same
  block as that call, once. Anything else is a compile error: a loss computed further
  (`(loss * 2.0).backward()`), a `mut` binding, a call result used in place, a `.backward()`
  inside an `if` arm below the call, a second `.backward()`. A second `@grad` call followed by
  its own `.backward()` replaces a parameter's gradient; it does not add to it.
- **The differentiated arguments stay borrowed until the `.backward()`.** The gradient is
  written after the call has returned, so between the call and its `.backward()` nothing else
  may read, borrow, move, assign or update `w`. The `.backward()` ends those borrows, which is
  what lets the update line use `w` again. Each differentiated argument must therefore be
  `&mut name` or a `&mut` binding passed on. A call with no `.backward()` borrows only for the
  call, like any other.
- **`.grad()`** borrows the gradient: `w.grad()` is a `&Tensor<T, S>` shaped like `w`, and no
  copy is made. Reading a slot that no `.backward()` has filled, or one emptied since, panics
  at run time. A live `.grad()` borrow blocks the `.zero_grad()` or `.backward()` that would
  release what it points at.
- **`.zero_grad()`** releases the gradient and empties the slot. It needs `w` mutably, like a
  `&mut self` method.
- **A gradient belongs to its tensor.** It is released when the tensor is, and it is never
  taken from a `pool` arena, even when `.backward()` runs inside one: the slot's owner outlives
  the block. A shape cast (`.reshape`, `.t()`, `.permute`, `.flatten`) consumes the tensor, and
  its result starts with no gradient; `.clone()` does not copy one.

## Signature rules

A `@grad` function:

- returns its loss as a rank-0 `Tensor<f32, []>`. A bare `f32` is `Copy` and has no gradient
  to carry, and the reverse pass starts from a loss of that type with a seed of `1.0`;
- differentiates **every** tensor parameter it takes, so each one must be borrowed
  `&mut Tensor<f32 | f64, [...]>` with literal extents, or shape parameters of a generic
  function. The borrow is mutable because `.backward()` writes the gradient into the caller's
  tensor;
- may take other parameters of any type. They are constants and have no gradient;
- is a free function or a method (see [Methods](#methods)), with `@grad` written without
  arguments. A generic function is differentiated once per instance the program uses, at that
  instance's concrete shapes.

Breaking any of these rules is a type error at the offending parameter or return type.

## Methods

`@grad` on a method differentiates the method's tensor parameters, under the same rules as a
function's. The receiver is a **constant**: the body may read its fields, and they steer the
gradient without receiving one. A method's loss runs its derivative through `.backward()`
exactly as a function's does, and only the differentiated arguments stay borrowed until then;
the receiver is borrowed for the call alone.

```neuro
struct Penalty {
    strength: f32
}

struct Objective {
    penalty: Penalty,
    steps: i32
}

impl Objective {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2, 1]>, b: &mut Tensor<f32, [1]>) -> Tensor<f32, []> {
        val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
        val y: Tensor<f32, [4, 1]> = [[6.0], [3.0], [3.0], [6.0]]
        val prediction = x @ w
        val shifted = prediction + b
        val residual = shifted - y
        val squares = &residual * &residual
        val ridge = w * w
        return Tensor::scalar(squares.mean() + ridge.sum() * self.penalty.strength)
    }
}
```

This method comes from [`examples/showcase/ridge_objective.nr`](../../examples/showcase/ridge_objective.nr),
where an ordinary method of the same type trains it with `self.loss(&mut w, &mut b)` and
`loss.backward()`.

- The method borrows its receiver, as `&self` or `&mut self`. A method taking `self` by value is
  a type error. Differentiating a receiver's own fields writes their gradients into the receiver
  after the call returns, which a consumed receiver could not hold, so every `@grad` method keeps
  its receiver borrowed.
- The body may read a field of the receiver, or of a struct parameter, that is a number, a
  `bool`, a `char` or a tensor, through any chain of struct fields (`self.penalty.strength`). A
  tensor field is read the ways a borrowed receiver allows: its elements, and reductions such as
  `.sum()`. Any other field is refused.
- `@grad` is not yet accepted on an associated function (one without `self`), on a method of a
  trait `impl`, or on a method of a generic `impl`. A `@grad` body still cannot call a method.

## What a `@grad` body may contain

The body is a sequence of statements that ends in the loss, either as `return` or as the tail
expression. Its values may use:

| Construct | Derivative |
|---|---|
| `+`, `-`, `*`, `/` on `f32` / `f64` values and tensors, broadcasting included | the product and quotient rules, with broadcast axes summed back |
| unary `-` on a float | sign flip |
| `a @ b` on matrices | `dA = dC @ Bᵀ`, `dB = Aᵀ @ dC` |
| `.sum()` and `.mean()`, over the whole tensor or along one `axis:` | the adjoint spread back over what was reduced |
| a tensor literal or `Tensor::scalar(v)` built from values | each element's adjoint sent back to the value it came from |
| an element read `t[i, j]`, at literal positions or ones computed at run time | the adjoint lands on that one element |
| a slice `t[1..3, ..]` or `t[(0..3).rev(), 1]` at literal bounds | each element's adjoint lands where it was read from |
| `.t()`, `.permute(...)`, `.reshape(...)`, `.flatten(...)` | the adjoint put back in the receiver's shape and axis order |
| `einsum(...)` | each operand's adjoint is the contraction of the result's adjoint with the other operands |
| `as` between integer and float types | the adjoint converted back; zero through an integer |
| `Tensor::zeros()`, `ones()`, `identity()`, literals | constants |
| comparisons, `&&`, `||`, `!`, and integer arithmetic | none: they decide which path runs, and carry no gradient |

The statements may be:

- `val` and `mut` bindings, assignment (`x = ...`), and compound assignment (`x += ...`) to a
  binding the body declared;
- `if`, with any number of `else if` arms and an optional `else`, as a statement or as an
  expression. An arm of an `if` at the top level of the body may end in `return`: the rest of
  the body is then the other arm;
- `while`, including loops whose trip count depends on the differentiated parameters;
- `for` over a range, `a..b` or `a..=b`, forwards, `.rev()` or `.enumerate()`. It is
  differentiated as the counted `while` it is, with the bounds read once as the loop reads them.

```neuro
@grad
func robust_fit(w: &mut Tensor<f32, [2, 1]>, limit: f32, sweeps: i32) -> Tensor<f32, []> {
    val x: Tensor<f32, [3, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0]]
    val y: Tensor<f32, [3, 1]> = [[5.0], [2.0], [2.0]]
    mut prediction = x @ w
    mut done = 0
    while done < sweeps {
        prediction = &prediction * 0.5
        done += 1
    }
    val residual = prediction - y
    val squares = &residual * &residual
    val error = squares.mean()
    val clipped = if error <= limit { error } else { limit + (error - limit) * 0.1 }
    return Tensor::scalar(clipped)
}
```

This function comes from [`examples/showcase/robust_fit.nr`](../../examples/showcase/robust_fit.nr).

### Calls

A `@grad` body may call its own functions, declared anywhere in the program, generic ones
included. The derivative goes through the callee: its body is differentiated where it is
called, its parameters standing for the arguments, so it may use exactly what a `@grad` body
may use, and a construct it cannot use is reported inside the callee. The callee needs no
annotation, and it is still compiled and called as usual by everything else.

```neuro
func squared_distance(prediction: &Tensor<f32, [3, 1]>, target: &Tensor<f32, [3, 1]>) -> f32 {
    val residual = prediction - target
    val squares = &residual * &residual
    return squares.sum()
}

@grad
func fit(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [3, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0]]
    val y: Tensor<f32, [3, 1]> = [[5.0], [2.0], [2.0]]
    val prediction = x @ w
    return Tensor::scalar(squared_distance(&prediction, &y))
}
```

These functions come from [`examples/showcase/composed_loss.nr`](../../examples/showcase/composed_loss.nr),
which also calls a shape-generic helper that branches.

A callee may read a `&mut` parameter but not write through it, and it must return a value.
Every call site gets its own copy of the callee in the derivative, so a function called in many
places grows the derivative accordingly. A recursive call, a method call, a call to a builtin
such as `println`, and a call through a closure or a function-typed value are refused.

Any other construct in a `@grad` body is a compile error pointing at it: a `for` over a collection, `loop`, `break` and `continue`, `match`, a `return` inside a loop or
anywhere but the end of an `if` arm at the top of the body, an assignment to a parameter,
`.max()` / `.min()`, a slice at a position computed at run time, and an `einsum` operand that
repeats a letter (a diagonal, as in a trace). A value that an `if` or a loop reassigns must be a float, integer or `bool`, or a float
tensor.

### Control flow in the derivative

The derivative follows the path the call takes. An `if` sends the gradient back through the
arm that ran, and a `while` sends it back through exactly the iterations that ran, last one
first. Nothing is recorded to do this. The forward pass only counts iterations, and the
backward pass recomputes each iteration's values from the loop's starting point. A loop that
runs `n` times therefore costs about `n²/2` extra evaluations of its body in the derivative.

### Where the path changes

At a point where the control flow is about to change, the derivative is the derivative of the
path the call executes there. An `if x > y` evaluated at `x == y` takes its `else` arm, so the
gradient there is the `else` arm's. A `while` whose last test only just failed gives the
gradient of the iterations that ran. Neither is an average of the two sides, and neither is an
error. This matches what the program computes: the loss at that point is the executed path's
loss, and its gradient is that path's gradient.

## How the derivative is checked

A derivative is trusted only once a second, independent computation agrees with it.
`tools/grad_differential.py` runs each `@grad` case on real inputs through `.backward()` and
reads every gradient back with `.grad()`, then computes central finite differences of the
compiled function at the same point, and requires the two to agree componentwise. At a point
where the path changes, a central difference would straddle both paths, so those cases compare
against finite differences of the executed path alone. It runs as
`cargo test -p neurc --test grad_differential`.
