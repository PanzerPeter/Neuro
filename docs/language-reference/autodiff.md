# Automatic Differentiation

`@grad` on a function or method asks the compiler for its derivative. The derivative is generated
at compile time as ordinary code beside the annotated function. Nothing records a graph while the
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

The annotated function compiles as written and stays callable. Beside it the compiler generates a
derivative with the same parameters. The derivative returns the loss together with a bundle holding
**one gradient per differentiated parameter**. Each gradient is an owned tensor with the parameter's
element type and shape. In the example, the gradient for `w` is a `Tensor<f32, [2, 1]>` and the
gradient for `b` is a `Tensor<f32, [1]>`.

The derivative is reverse mode. It evaluates the loss once, then walks the computation backwards
from the loss, adding each value's contribution wherever that value was used. Shapes matter here: a
`[1]` bias broadcast down four rows gets its gradient summed over those rows.

## Running the derivative: `.backward()`, `.grad()`, `.zero_grad()`

> **Planned change.** `.backward()`, `.grad()`, `.zero_grad()` and the gradient slot are planned to go
> before the neural network library lands. The planned form is `val (loss, g) = grad(f(args))`, where
> a gradient is an ordinary value you own. Code written against this section will need that change.

The derivative runs when the loss returned by a `@grad` call is asked for it. `.backward()` runs it
and moves each gradient into a slot beside the tensor it belongs to. `.grad()` reads that gradient,
and `.zero_grad()` empties the slot. A training step from
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
            measured = loss.item()
        }
```

The spelling follows tape-based frameworks, but there is no tape. The derivative runs where the call
ran, on the arguments that call saw. A `@grad` call whose result never meets a `.backward()` runs no
derivative.

- **`.backward()`** must be called on a `val` bound directly to a `@grad` call's result, in the same
  block as that call, and only once. Anything else is a compile error: a loss computed further
  (`(loss * 2.0).backward()`), a `mut` binding, a call result used in place, a `.backward()` inside an
  `if` arm below the call, or a second `.backward()` on the same binding. A second `@grad` call
  followed by its own `.backward()` replaces a parameter's gradient. It does not add to it.
- **Differentiated arguments stay borrowed until `.backward()`.** The gradient is written after the
  call returns, so between the call and its `.backward()` nothing may read, borrow, move, assign or
  update the argument. The `.backward()` ends those borrows, which lets the update line use `w` again.
  Each differentiated argument must be `&mut name` or a `&mut` binding passed on. A call with no
  `.backward()` borrows only for the call. When a method's `wrt:` names a field of its receiver, the
  receiver is held the same way.
- **`.grad()`** borrows the gradient. `w.grad()` is a `&Tensor<T, S>` shaped like `w`, and no copy is
  made. Reading a slot that no `.backward()` has filled, or one emptied since, panics at run time. A
  live `.grad()` borrow blocks the `.zero_grad()` or `.backward()` that would release what it points
  at.
- **`.zero_grad()`** releases the gradient and empties the slot. It needs `w` mutably, like a
  `&mut self` method.
- **A gradient belongs to its tensor.** It is released with the tensor. It is never taken from a
  `pool` arena, even when `.backward()` runs inside one, because the slot's owner outlives the block.
  A shape cast (`.reshape`, `.t()`, `.permute`, `.flatten`) consumes the tensor, and its result starts
  with no gradient. `.clone()` does not copy one.

## Signature rules

A `@grad` function:

- returns its loss as a rank-0 `Tensor<f32, []>`. A bare `f32` is `Copy` and has no gradient to carry.
  The reverse pass starts from the loss with a seed of `1.0`.
- differentiates **every** tensor parameter it takes, unless a [`wrt:`](#choosing-what-is-differentiated-wrt)
  list says otherwise. Each differentiated parameter is borrowed `&mut` and has the type
  `Tensor<f32 | f64, [...]>` with literal extents, or shape parameters of a generic function. The
  borrow is mutable because `.backward()` writes the gradient into the caller's tensor.
- takes at least one tensor parameter to differentiate. A function with none is refused.
- may take other parameters of any type, passed however it likes. They are constants and have no
  gradient.
- is a free function or a method (see [Methods](#methods)). A generic function is differentiated once
  per instance the program uses, at that instance's concrete shapes.

Breaking any of these rules is a type error at the offending parameter or return type.

## Choosing what is differentiated: `wrt:`

`@grad(wrt: [...])` lists what the derivative is taken with respect to. Everything left out is a
constant: it has no gradient, `.backward()` leaves its slot alone, and it may be passed by value or
by `&`. `@grad(wrt: [w, b])` over `(w: &mut ..., b: &mut ..., data: &Tensor<...>)` differentiates `w`
and `b` and reads `data`.

An entry is either a parameter's name or, on a method, a path from `self` to a tensor: through fields
(`self.head.w`) and array elements at a literal position (`self.heads[1]`). A path may pass only
through **exported** fields, because naming a private one would tie the annotation to a layout the
type does not promise. A method whose `wrt:` names a path must take `&mut self`. `.backward()` writes
that field's gradient slot after the call returns, and the receiver stays mutably borrowed until then.
From [`examples/showcase/frozen_features.nr`](../../examples/showcase/frozen_features.nr), where only
the head of a model learns:

```neuro
    @grad(wrt: [self.head.w, self.head.b])
    func loss(&mut self, x: &Tensor<f32, [4, 2]>, y: &Tensor<f32, [4, 1]>) -> Tensor<f32, []> {
        val features = self.features.clone()
        val hidden = x @ features
        val w = self.head.w.clone()
        val out = hidden @ w
        val shifted = out + self.head.b.clone()
        val residual = shifted - y
        val squares = &residual * &residual
        return Tensor::scalar(squares.mean())
    }
```

After `loss.backward()`, `model.head.w.grad()` and `model.head.b.grad()` hold the gradients, and
`model.features` has none. Every read of a listed field, in a branch, a loop or a function the body
calls, adds to that field's one gradient.

A `wrt:` entry that is not a parameter, is not a tensor, is listed twice, is a tensor not borrowed
`&mut`, reaches through a private field, indexes past an array's end, or names `self` in a function
without a receiver is a type error at that entry. An empty list is a type error too.

## Methods

`@grad` on a method differentiates the method's tensor parameters under the same rules as a
function's. Without a `wrt:` path, the receiver is a **constant**: the body may read its fields, and
they steer the gradient without receiving one. A constant receiver is borrowed for the call alone.

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

- The receiver is borrowed, as `&self` or `&mut self`. A method taking `self` by value is a type
  error. Differentiating the receiver's fields writes their gradients into the receiver after the call
  returns, and a consumed receiver cannot hold them.
- The body may read a field of the receiver, or of a struct parameter, that is a number, a `bool`, a
  `char` or a tensor, through any chain of struct fields and literal array positions
  (`self.penalty.strength`, `self.heads[1]`). A tensor field is read the ways a borrowed receiver
  allows: its elements, reductions such as `.sum()`, and `.clone()`, which makes it an operand of `@`
  or `*`. Any other field, and an array element at a run-time position, is refused.
- `@grad` is not yet accepted on an associated function (one without `self`), on a method of a trait
  `impl`, or on a method of a generic `impl`. A `@grad` body still cannot call a method.
- A method takes first derivatives only. `@grad(order: 2)` on a method is refused at `order`.

## Second derivatives: `order: 2`

`@grad(order: 2)` also asks for each differentiated parameter's second derivative. After
`.backward()`, `w.hessian()` borrows it beside `w.grad()`. For a parameter of shape `S` it is a
`&Tensor<T, S ++ S>`: a `[2]` parameter's Hessian is `[2, 2]`, a `[2, 3]` one's is `[2, 3, 2, 3]`, and
the element at `[i..., j...]` is how gradient element `i` changes along parameter element `j`.
`.grad()` still means the first derivative, so adding `order: 2` never changes what existing code
reads. `order: 1` is the default spelled out. Any other order is a type error at the value.

```neuro
@grad(order: 2)
func logistic_loss(theta: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x: Tensor<f32, [6]> = [-2.0, -1.0, -0.5, 0.5, 1.0, 2.5]
    val y: Tensor<f32, [6]> = [-1.0, -1.0, 1.0, -1.0, 1.0, 1.0]
    val weight = theta[0]
    val bias = theta[1]
    val margin = (x * weight + bias) * y
    val loss = ((margin * -1.0).exp() + 1.0).log()
    val ridge = theta * theta
    return Tensor::scalar(loss.mean() + ridge.sum() * 0.05)
}
```

Its caller takes Newton steps, solving `H d = g` with the curvature that `.hessian()` reads:

```neuro
        pool {
            val l = logistic_loss(&mut theta)
            l.backward()
            loss = l.sum()
            val (g0, g1) = (theta.grad()[0], theta.grad()[1])
            val (a, b) = (theta.hessian()[0, 0], theta.hessian()[0, 1])
            val (c, d) = (theta.hessian()[1, 0], theta.hessian()[1, 1])
            slope = g0.abs() + g1.abs()
            // The inverse of a 2 x 2 matrix, applied to the gradient.
            val det = a * d - b * c
            val step: Tensor<f32, [2]> = [(d * g0 - b * g1) / det, (a * g1 - c * g0) / det]
            theta -= step
        }
```

Both come from [`examples/showcase/newton_fit.nr`](../../examples/showcase/newton_fit.nr). Newton's
method reaches the minimum in about a tenth of the steps gradient descent takes.

- **The Hessian has a slot of its own**, beside the gradient's, with the same rules. Reading it before
  an `order: 2` `.backward()` has filled it panics. It is released with its tensor and never taken from
  a `pool` arena. `.zero_grad()` and a shape cast empty it, and so does a first-order `.backward()`,
  which replaces the gradient. A Hessian taken at an earlier point is therefore never left behind.
- **Each parameter gets the Hessian with respect to itself.** With two differentiated parameters `w`
  and `b`, `w.hessian()` and `b.hessian()` are filled. The mixed derivatives between `w` and `b` are
  not computed.
- **The cost grows with the parameter.** The Hessian of an `n`-element parameter is built one row at a
  time, each row a derivative of the derivative. It costs about `n` times the derivative, and its
  buffer holds `n²` elements.
- Under `order: 2`, an element read at a position computed at run time (`w[i]` in a loop) is refused at
  the read. So is a parameter passed by value that is neither a number nor a tensor.

## Stopping the gradient: `.detach()` and `@no_grad`

Two fences keep a value out of the derivative, at two granularities. `t.detach()` returns `t`'s data
as a constant to the derivative, however it was computed. `@no_grad` on a free function makes every
call to it from a `@grad` body a constant. The call runs as written and its body is never
differentiated, so the body may use anything, `.max()` and method calls included.

Semi-gradient temporal-difference learning needs exactly this. Its target is built from the weights
being trained, and must not be trained itself. Either fence does it:

```neuro
// The bootstrapped target, in a function no derivative looks into.
@no_grad
func bootstrap(w: &Tensor<f32, [2, 1]>) -> Tensor<f32, [4, 1]> {
    return rewards() + (successors() @ w) * GAMMA
}
```

```neuro
@grad
func td_detached(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val target = rewards() + (successors() @ w) * GAMMA
    val fixed = target.detach()
    val error = features() @ w - fixed
    val squares = &error * &error
    return Tensor::scalar(squares.mean())
}

@grad
func td_no_grad(w: &mut Tensor<f32, [2, 1]>) -> Tensor<f32, []> {
    val error = features() @ w - bootstrap(w)
    val squares = &error * &error
    return Tensor::scalar(squares.mean())
}
```

Both come from [`examples/showcase/td_value_fit.nr`](../../examples/showcase/td_value_fit.nr). There
the two train to the same weights bit for bit. The same loss without a fence (the residual gradient)
settles on a different line.

- **`.detach()` consumes its receiver**, like a shape cast. The buffer is handed on, not shared, and
  the result starts with an empty gradient slot. Write `t.clone().detach()` to keep `t`. A borrowed
  tensor cannot be detached. Outside a `@grad` body, `.detach()` only moves the tensor and forgets its
  gradient.
- **`@no_grad` takes no arguments and annotates a free function.** It cannot share a function with
  `@grad`, and it is not accepted on a method yet. Outside a derivative the function is an ordinary one.
- **A `@no_grad` call only reads its arguments.** The derivative reads them again after the call, so an
  owned tensor argument is passed as a copy. An argument the function borrows `&mut`, or takes by value
  when it is neither a number nor a tensor, is refused at that argument. Like any call in a `@grad`
  body, the call must return a value. Inside a branch or a loop, the derivative may run it more than once.
- **Both fences hold at every order.** Under `order: 2` a fenced value is a constant to the second
  derivative too. So `.hessian()` is the derivative of the gradient with the fenced values held fixed.

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
| an element read `t[i, j]`, at literal positions or ones computed at run time, or `.item()` on a rank-0 tensor | the adjoint lands on that one element |
| a slice `t[1..3, ..]`, `t[(0..3).rev(), 1]` or `t[(0..6).step(2)]` at literal bounds | each element's adjoint lands where it was read from |
| `.t()`, `.permute(...)`, `.reshape(...)`, `.flatten(...)` | the adjoint put back in the receiver's shape and axis order |
| `einsum(...)` | each operand's adjoint is the contraction of the result's adjoint with the other operands |
| `as` between integer and float types | the adjoint converted back; zero through an integer |
| `.exp()`, `.log()`, `.sqrt()`, `.tanh()`, `.abs()`, `.pow(p)` on a float or a float tensor | `exp(x)`, `1/x`, `1/(2·sqrt(x))`, `1 - tanh(x)²`, `sign(x)` (0 at exactly 0), and `p·x^(p-1)` with the exponent not differentiated |
| `.clone()` of a tensor | the copy's adjoint goes to the original |
| `.detach()`, and a call to a `@no_grad` function | none: the value is a constant ([Stopping the gradient](#stopping-the-gradient-detach-and-no_grad)) |
| `Tensor::zeros()`, `ones()`, `identity()`, literals | constants |
| comparisons, `&&`, `||`, `!`, and integer arithmetic | none: they decide which path runs, and carry no gradient |

The statements may be:

- `val` and `mut` bindings, assignment (`x = ...`), and compound assignment (`x += ...`) to a binding
  the body declared;
- `if`, with any number of `else if` arms and an optional `else`, as a statement or as an expression.
  An arm of an `if` at the top level of the body may end in `return`. The rest of the body is then the
  other arm;
- `while`, including loops whose trip count depends on the differentiated parameters;
- `for` over a range, `a..b` or `a..=b`, forwards, `.rev()`, `.step(n)` or `.enumerate()`. It is
  differentiated as the counted `while` it is, with the bounds read once as the loop reads them. A
  `.step(n)` stride must be an integer literal, because the replay has no run-time check against a zero
  stride.

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

A `@grad` body may call its own functions, declared anywhere in the program, generic ones included.
The derivative goes through the callee. The callee's body is differentiated where it is called, with
its parameters standing for the arguments, so it may use exactly what a `@grad` body may use. A
construct it cannot use is reported inside the callee. The callee needs no annotation, and everything
else still compiles and calls it as usual.

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

A callee may read a `&mut` parameter but not write through it, and it must return a value. Every call
site gets its own copy of the callee in the derivative, so a function called in many places grows the
derivative accordingly. A recursive call, a method call and a call to a builtin such as `println` are
refused. A call to a `@no_grad` function is not differentiated through at all. It is a constant.

### Where the derivative runs

"GPU gradients" means different things, so each case is listed by what the `@grad` body does.

| In a `@grad` body | Result |
| --- | --- |
| Tensor operations on host tensors | Differentiated, on the host |
| Tensor operations on device tensors | Differentiated, with each operation on the tensor's GPU |
| A call to a `@gpu` function | Refused at compile time, unless the callee is `@no_grad`, which makes it a constant |
| A call to a `@kernel` function | Refused at compile time |
| `@grad` and `@gpu` on one function | Refused at compile time |

A loss built from `.sum()` or `.mean()` is a host tensor, since those reductions return a host scalar.
See [BUG-143](../BUGS.md#bug-143-the-gradient-of-a-device-parameter-can-land-on-the-host) for where a
gradient can land today.

### Function values

A call through a function value is differentiated too, when the compiler can tell which function it
calls. The derivative is built at compile time, and a function value carries no derivative at run time.
That covers a closure (its captures enter the derivative like any other value the body reads), a
function named as a value (`val f = square`), a `>>` composition, a `|>` pipeline stage, a helper's
function-typed parameter, and the function given to `.map`, `.zip` and `.reduce`.

A function-typed parameter of the `@grad` function itself gets its function from the call that runs
`.backward()`. Each distinct function passed there gets its own derivative, the way a generic function
gets one per type argument:

```neuro
@grad
func activated_error(w: &mut Tensor<f32, [2, 1]>, activate: (f32) -> f32) -> Tensor<f32, []> {
    val x: Tensor<f32, [4, 2]> = [[1.0, 2.0], [2.0, 0.0], [0.0, 1.0], [3.0, 1.0]]
    val y: Tensor<f32, [4, 1]> = [[5.0], [2.0], [2.0], [5.0]]
    val prediction = (x @ w).map(activate)
    val residual = prediction - y
    val squares = residual.map(|r: f32| -> f32 { r * r })
    return Tensor::scalar(squares.reduce(0.0f32, |acc: f32, s: f32| -> f32 { acc + s }) * 0.25)
}
```

`activated_error(&mut w, |v: f32| -> f32 { v * gain })` and
`activated_error(&mut w, lift >> soften)` each run a derivative made for the function they pass. Both
calls come from [`examples/showcase/activation_fit.nr`](../../examples/showcase/activation_fit.nr).

The function passed there must be written at the call: a closure or a composition, or a local bound
to one that captures nothing. A function chosen at run time is refused. That includes a binding set by
an `if` whose arms give different functions, a function reassigned inside a branch or a loop, and a
function-typed parameter of an ordinary function passed on into a `@grad` call whose `.backward()` runs.
Write the choice as an `if` around two direct calls instead.

`.map`, `.zip` and `.reduce` are unrolled in the derivative, one call per element, in the order the
traversal runs. A traversal over more than 1024 elements, or over a tensor with a dynamic axis, is
refused.

Any other construct in a `@grad` body is a compile error that points at it: a `for` over a collection,
a `loop` (unless its first way out is `if !condition { break }`, which makes it the `while` it spells),
`break` and `continue`, `match`, a `return` inside a loop or anywhere but the end of an `if` arm at the
top of the body, an assignment to a parameter, `.max()` / `.min()`, a slice at a position computed at
run time, and an `einsum` operand that repeats a letter (a diagonal, as in a trace). A value that an
`if` or a loop reassigns must be a float, an integer, a `bool`, or a float tensor.

### Control flow in the derivative

The derivative follows the path the call takes. An `if` sends the gradient back through the arm that
ran, and a `while` sends it back through exactly the iterations that ran, last one first. Nothing is
recorded to do this. The forward pass only counts iterations, and the backward pass recomputes each
iteration's values from the loop's starting point. A loop that runs `n` times therefore costs about
`n²/2` extra evaluations of its body in the derivative.

### Where the path changes

At a point where the control flow is about to change, the derivative is the derivative of the path the
call executes there. An `if x > y` evaluated at `x == y` takes its `else` arm, so the gradient there is
the `else` arm's. A `while` whose last test only just failed gives the gradient of the iterations that
ran. Neither is an average of the two sides, and neither is an error. This matches what the program
computes: the loss at that point is the executed path's loss, and its gradient is that path's gradient.

## How the derivative is checked

A derivative is trusted only once a second, independent computation agrees with it.
[`tools/grad_differential.py`](../../tools/grad_differential.py) runs each `@grad` case on real inputs
through `.backward()` and reads every gradient back with `.grad()`. It then computes central finite
differences of the compiled function at the same point, and requires the two to agree componentwise.
At a point where the path changes, a central difference would straddle both paths. Those cases compare
against finite differences of the executed path alone. A case with a `.detach()` or a `@no_grad` call
is compared against finite differences of the function with the fenced values frozen at the point.

A second derivative is read back with `.hessian()` and compared against central finite differences of
the compiled `.grad()`, which has itself just been checked against the loss. The harness runs as
`cargo test -p neurc --test grad_differential`, or directly with
`python tools/grad_differential.py --neurc target/debug/neurc`.

## See also

- [Functions](functions.md), the function forms a `@grad` function is written in
- [Tensors](tensors.md), the tensor type, its shapes and its methods
- [Ownership and Borrowing](ownership.md), the `&mut` borrows a differentiated argument holds
- [Structs and Enums](structs.md), the receiver fields a `@grad` method may read
