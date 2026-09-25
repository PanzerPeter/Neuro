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
which runs it.

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

The derivative function cannot be called from Neuro source yet. `.backward()` and `.grad()`,
which run it and put each gradient next to its tensor, come next. Until then the derivative
can be reached as a C symbol from an object built with `neurc compile --emit obj`, which is
how the compiler's own tests check it.

## Signature rules

A `@grad` function:

- returns its loss as a rank-0 `Tensor<f32, []>`. A bare `f32` is `Copy` and has no gradient
  to carry, and the reverse pass starts from a loss of that type with a seed of `1.0`;
- differentiates **every** tensor parameter it takes, so each one must be borrowed
  `&mut Tensor<f32 | f64, [...]>` with literal extents, or shape parameters of a generic
  function. The borrow is mutable because the gradient belongs to the caller's tensor and will
  be written back to it;
- may take other parameters of any type. They are constants and have no gradient;
- is a free function, with `@grad` written without arguments. A generic one is differentiated
  once per instance the program uses, at that instance's concrete shapes.

Breaking any of these rules is a type error at the offending parameter or return type.

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
`tools/grad_differential.py` calls each generated derivative on real inputs, then computes
central finite differences of the compiled function at the same point, and requires the two
to agree componentwise. At a point where the path changes, a central difference would straddle
both paths, so those cases compare against finite differences of the executed path alone. It
runs as `cargo test -p neurc --test grad_differential`.
