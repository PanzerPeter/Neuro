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
  `&mut Tensor<f32 | f64, [...]>` with literal extents. The borrow is mutable because the
  gradient belongs to the caller's tensor and will be written back to it;
- may take other parameters of any type. They are constants and have no gradient;
- is a free function that is not generic, with `@grad` written without arguments.

Breaking any of these rules is a type error at the offending parameter or return type.

## What a `@grad` body may contain

The body is a sequence of `val` bindings that ends in the loss, either as `return` or as the
tail expression. Each binding may use:

| Construct | Derivative |
|---|---|
| `+`, `-`, `*`, `/` on `f32` / `f64` values and tensors, broadcasting included | the product and quotient rules, with broadcast axes summed back |
| unary `-` on a float | sign flip |
| `a @ b` on matrices | `dA = dC @ Bᵀ`, `dB = Aᵀ @ dC` |
| `.sum()` and `.mean()`, over the whole tensor or along one `axis:` | the adjoint spread back over what was reduced |
| a tensor literal or `Tensor::scalar(v)` built from values | each element's adjoint sent back to the value it came from |
| an element read `t[i, j]` at literal positions | the adjoint lands on that one element |
| `Tensor::zeros()`, `ones()`, `identity()`, literals | constants |

Any other construct in a `@grad` body is a compile error pointing at it: a function or
method call, a `mut` binding or assignment, a loop or branch, `.max()` / `.min()`, `einsum`,
reshapes and slices, and a read at a position computed at run time.

## How the derivative is checked

A derivative is trusted only once a second, independent computation agrees with it.
`tools/grad_differential.py` calls each generated derivative on real inputs, then computes
central finite differences of the compiled function at the same point, and requires the two
to agree componentwise. It runs as `cargo test -p neurc --test grad_differential`.
