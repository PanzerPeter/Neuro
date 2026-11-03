# Functions

A function is a named, typed block of code, and every program starts at `main`. Generic functions are covered in [Generics and Traits](generics-and-traits.md).

## Declaration

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b
}
```

The return type follows `->`. A function that returns nothing omits it and returns `void`:

```neuro
func announce() {
    println("ready")
}
```

## Parameters

Each parameter is written `name: Type`. Calls bind arguments to parameters in the order written, or by name (see [Named Arguments](#named-arguments)).

A parameter of scalar type is passed by value (copied). A reference parameter (`&T` or `&mut T`) receives a borrow, see [Ownership and Borrowing](ownership.md#borrows-t). Any other non-`Copy` value passed by value is moved into the callee, and the caller can no longer use it.

```neuro
func modify(x: i32) -> i32 {
    mut temp: i32 = x
    temp = temp + 10
    return temp  // Returns modified value, original unchanged
}

func test() -> i32 {
    val original: i32 = 5
    val result: i32 = modify(original)
    // original is still 5
    // result is 15
    return result
}
```

Neuro has no default parameter values and no variadic functions: every parameter of a declared function must be given. A function that takes any number of values takes a slice:

```neuro
func sum(values: &[i32]) -> i32 {
    mut total = 0
    for v in values { total = total + v }
    total
}

func main() -> i32 {
    val xs = [1, 2, 3]
    val s = sum(&xs)      // 6
    return s - 6
}
```

## Named Arguments

An argument may be passed by name at the call site. Named arguments may appear in any order, and any number of positional arguments may come first.

```neuro
func connect(host: string, port: i32, timeout: i32) -> i32 {
    return port + timeout
}

func main() -> i32 {
    val a = connect("localhost", 8080, 30)               // all positional
    val b = connect("localhost", port: 8080, timeout: 30)
    val c = connect("localhost", timeout: 30, port: 8080) // same call as b
    return a - b
}
```

Naming an argument does not make it optional. `connect("localhost", port: 8080)` is rejected with `'connect' takes 3 argument(s), but 1 given`.

### External labels

A parameter may be declared with two names: an **external label** the caller writes and an **internal name** the body uses, written `external internal: T`. The external label is then required at every call site.

```neuro
func clamp(_ value: f32, min lo: f32, max hi: f32) -> f32 {
    if value < lo { return lo }
    if value > hi { return hi }
    return value          // the body says lo / hi, never min / max
}

func main() -> i32 {
    val x = clamp(1.5, min: 0.0, max: 1.0)
    return 0
}
```

Writing `clamp(1.5, 0.0, 1.0)` is rejected with ``the 'min' argument of 'clamp' must be named: write `min: <value>` ``.

An external label of `_` suppresses the call-site name: the argument is positional, and naming it is an error.

```neuro
func scale(_ value: f32, _ factor: f32) -> f32 {
    return value * factor
}

func main() -> i32 {
    val r = scale(2.0, 3.0)      // no names accepted
    return 0
}
```

`scale(value: 2.0, factor: 3.0)` is rejected with ``the 'value' parameter of 'scale' is declared `_ value:`, which means it is passed positionally and its name is not written at the call site``.

### Rules

A plain `name: T` parameter may be passed positionally or as `name: value`. Free functions, associated functions (`Type::f(...)`) and methods (`x.m(...)`) all accept named arguments. Builtin tensor methods take labels too, such as `.sum(axis: 1)` (see [Tensors](tensors.md)).

Closures, the panic builtins, enum variants and newtype constructors declare no parameter names, so a label on one is an error. The compiler reports the rule each violation breaks:

| Violation | Diagnostic |
| --- | --- |
| A positional argument after a named one | ``in the call to 'connect': a positional argument cannot follow a named one; move it before `port:` `` |
| An `external internal: T` parameter passed positionally | ``the 'min' argument of 'clamp' must be named: write `min: <value>` `` |
| A `_ name: T` parameter passed by name | ``the 'value' parameter of 'scale' is declared `_ value:`, which means it is passed positionally and its name is not written at the call site`` |
| A label that matches no parameter | `'f' has no parameter named 'c'` |
| The same label twice | `argument 'a' is given twice in the call to 'f'` |
| A missing argument | `'f' takes 2 argument(s), but 1 given` |
| Two parameters of one function with the same call-site name (reported at the declaration) | `two parameters share the call-site name 'x'; a named argument must identify exactly one parameter` |
| A label on a closure, panic builtin, enum variant or newtype constructor | `named arguments are not available here: 'panic' has no declared parameter names` |

Arguments are bound to parameters before type checking, and are evaluated in the order they are written, exactly as a positional call evaluates them left to right. Naming them therefore changes which parameter each value reaches, never when it is computed:

```neuro
func bump(r: &mut i32) -> i32 { *r = *r + 1
    return 0 }
func combine(first: i32, second: i32) -> i32 { first * 10 + second }

func main() -> i32 {
    mut x = 0
    // `bump` is written first, so it runs first and `x` is read as 1: the call is
    // `combine(first: 1, second: 0)` and `n` is 10.
    val n = combine(second: bump(&mut x), first: x)
    return n - 10
}
```

A call that only passes values costs nothing at runtime: it produces exactly the IR the equivalent positional call produces. A call that reorders arguments carrying effects binds each of them to a temporary first, which is one binding per argument and no other cost.

### Limitation

A method call is matched on the method name alone, because the receiver's type is not yet known when arguments are bound. If two different types declare a method of the same name with *different* parameter names, a named argument on that method is rejected with ``named arguments cannot be used with 'm': more than one type declares a method of that name with different parameter names, so `x:` does not identify one parameter``. Write the call positionally, or rename one of the methods. Positional calls are never affected.

## Return Values

The last expression in a function body is its return value, and it must match the declared return type. `return` may appear anywhere in the body and exits at once. Statements after a `return` compile without a warning and never run.

### Expression-Based Returns

Neuro has no semicolons, and a statement ends at its newline, so the final expression needs no keyword:

```neuro
func add(a: i32, b: i32) -> i32 {
    a + b
}
```

### Explicit Return

`return` exits early. Mixing the two forms is common:

```neuro
func mixed(x: i32) -> i32 {
    if x > 10 {
        return 100  // explicit return for an early exit
    }
    x * 2           // the last expression is the return value
}
```

A bare `return` exits a function that returns `void`.

## Recursion

A function may call itself, and may call a function declared later in the file, so mutual recursion needs no forward declaration.

### Tail Recursion

Tail calls are not guaranteed to be eliminated. At `-O0` every call takes a stack frame, so a deep tail recursion can overflow the stack; from `-O1` up, LLVM usually turns a function's tail call to itself into a loop:

```neuro
func factorial_tail(n: i32, acc: i32) -> i32 {
    if n <= 1 {
        acc
    } else {
        factorial_tail(n - 1, n * acc)
    }
}

func factorial(n: i32) -> i32 {
    factorial_tail(n, 1)
}
```

## The main Function

Every program needs a `main` entry point, declared as `func main() -> i32` with no parameters. Only the root file's `main` is the entry point.

The return value is the process exit status: `0` for success, and any other value signals an error by convention.

Any other signature is refused. `func main(argc: i32) -> i32` gives `` `main` must be declared `func main() -> i32`: it takes no parameters, found 1 ``. The C runtime passes `argc` as the first argument and reads the return register as the exit status, so no other shape has a defined exit code.

## Scope

### Local Variables

Variables are local to the block that declares them. Parameters are local variables initialized from the arguments.

```neuro
func scoped() -> i32 {
    val x: i32 = 10
    return x
}

func other() -> i32 {
    val y: i32 = x  // error: undefined variable 'x'
    return y
}
```

### Shadowing

An inner scope may shadow an outer variable, and a body may shadow a parameter of the same name:

```neuro
func shadowing() -> i32 {
    val x: i32 = 1
    if true {
        val x: i32 = 2  // Shadows outer x
        // Inner x is 2
    }
    // Outer x is still 1
    return x
}
```

## Common Mistakes

A missing `return` on one path:

```neuro
func missing_return(x: i32) -> i32 {
    if x > 0 {
        return x
    }
    // error: missing return statement in function returning i32
}

// Fix: add else branch
func fixed(x: i32) -> i32 {
    if x > 0 {
        return x
    } else {
        return 0
    }
}
```

A semicolon, which Neuro does not use:

```neuro
func wrong() -> i32 {
    val x: i32 = 42
    x;  // error: unexpected token Semicolon, expected a newline or '}' to end the statement
}

// Fix: remove the semicolon
func right() -> i32 {
    val x: i32 = 42
    x  // Implicit return
}
```

Type, arity and scope errors:

| Mistake | Diagnostic |
| --- | --- |
| Passing an `i64` to an `i32` parameter | `type mismatch: expected i32, found i64` |
| `return 3.14` from a function returning `i32` | `return type mismatch: expected i32, found f64` |
| Calling a two-parameter function with one argument | `incorrect number of arguments: expected 2, found 1` |
| Reading a local of another function | `undefined variable 'x'` |
| Calling a function that is not declared | `undefined function 'x_fn'` |
| Using a `string` after passing it by value | ``use of moved value 's': bind a `.clone()` if you need an independent copy`` |

## Panic Builtins

Three compiler-known builtins terminate the program when an unrecoverable condition is reached. They abort without unwinding: each prints a diagnostic (`message at file:line:col`) to standard error and calls `abort()`. The stack is not unwound, so `Drop` destructors run only on normal scope exit, never during a panic.

```neuro
func divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        panic("division by zero")
    }
    a / b
}

func main() -> i32 {
    assert(divide(10, 2) == 5)   // passes silently
    val x = divide(1, 0)         // prints "panic: division by zero at main.nr:3:9" and aborts
    return 0
}
```

| Builtin | Signature | Behaviour |
|---|---|---|
| `panic` | `panic(msg: string)` | Print `panic: <msg> at <loc>` and abort. |
| `assert` | `assert(cond: bool)` | Abort with `assertion failed at <loc>` only when `cond` is false; otherwise continue. |
| `unreachable` | `unreachable()` | Print `internal error: entered unreachable code at <loc>` and abort. |

Because these builtins diverge (they never return), a call may appear anywhere a value is expected, including the implicit-return (tail) position of a non-`void` function:

```neuro
func parse_digit(c: i32) -> i32 {
    if c >= 48 && c <= 57 { c - 48 } else { panic("not a digit") }
}
```

A user-defined function named `panic`, `assert` or `unreachable` shadows the builtin within the program.

`return`, `break` and `continue` diverge the same way, so an `if` or `match` arm that uses one takes its type from its siblings, see [expressions.md](expressions.md#if-expressions).

## Standard Output Builtins

`print` and `println` write text to standard output. Unlike the panic builtins they return: the result is `void`, so a call is a statement and not a value.

| Builtin | Signature | Behaviour |
|---|---|---|
| `print` | `print(text: string)` | Write `text` to stdout. |
| `println` | `println(text: string)` | Write `text` followed by a newline. |

The newline is one `\n` byte. Standard output is the platform's text stream, so on Windows it reaches the console or pipe as `\r\n`, exactly as it would from a C program.

```neuro
func main() -> i32 {
    val name: string = "Neuro"
    val ratio: f64 = 0.875

    print("hello from ")
    println(name)
    println("phase 2 is {ratio:.2} done")
    return 0
}
```

```text
hello from Neuro
phase 2 is 0.88 done
```

Each takes exactly one argument, an owned `string` or an immutable `&string` sub-slice (`text.slice(1..4)`). There is no variadic form and no format string at the call site, because [string interpolation](strings.md#string-interpolation) has already rendered every hole into one `string` before the call. `println("{label:>10}: {count} of {total}")` is one argument.

The text is read, not consumed, so the value stays usable afterwards. A user-defined function named `print` or `println` shadows the builtin within the program, as one named `panic` does. `@no_prelude` does not remove them, because they are compiler builtins rather than declarations.

Output is buffered. A call copies its bytes into a page-sized buffer, which reaches the operating system when it fills, so a printing loop makes one write per buffer rather than one or two per call. The buffer is emptied on every path out of the program (`main` returning, a panic, a debug-build overflow trap), so buffering never loses output, and a line printed before a panic appears before the panic's diagnostic.

When standard output is a terminal, the buffer is emptied at the end of every `println`, so progress is visible while the program runs. A pipe or a file gets the full buffer, as with C's standard library. `print` writes no line terminator, so its bytes wait for the next `println` or for the program to finish.

## Closures and Lambdas

A **closure** is an anonymous callable written with pipe-delimited parameters, `|params| body`. It can capture variables from the enclosing scope.

```neuro
// Single-expression body (no braces): the return type is the body's.
val square = |x: i32| x * x
val nine   = square(3)              // 9

// Block body: the return type is its last expression's, or write it.
val clamp = |x: i32| {
    if x < 0 { 0 } else { x }
}
val wide = |x: i32| -> i64 { x as i64 * 2 }

// The binding's type gives `x` its type.
val cube: (i32) -> i32 = |x| x * x * x

// Zero parameters.
val make = || 42

// `move` takes every capture by value: the closure owns what it captured.
val label = 100
val tagged = move |x: i32| x + label
```

### Capture

A closure captures each free variable its body uses. How it holds one depends on how the body uses it, variable by variable:

| The body | The capture | Example |
| --- | --- | --- |
| only reads a `Copy` value | a copy | `\|n: i32\| n + offset` |
| only reads any other value | a shared borrow `&` | `\|n: i32\| name.len() + n` |
| writes it (assigns, takes `&mut`, calls a `&mut self` method) | a mutable borrow `&mut` | `\|\| -> void { count += 1 }` |

```neuro
func main() -> i32 {
    val offset = 10
    val shift  = |n: i32| n + offset   // copies `offset`
    val a = shift(5)                   // 15

    mut count = 0
    {
        val bump = || -> void { count += 1 }   // borrows `count` mutably
        bump()
        bump()
    }                                  // the borrow ends with `bump`
    a + offset + count                 // 27
}
```

A borrowing closure holds its borrow for as long as it lives, exactly as `val r = &x` does: the binding cannot be moved while a closure reads it, and cannot be used at all while one writes it. End the closure's scope, as the block above does, before using the binding again.

A `move` closure takes every capture by value. A `Copy` capture is copied into the closure, which keeps its own copy from call to call. Any other capture is moved in, so its source binding is gone, and the closure releases it when its last copy is gone:

```neuro
func main() -> i32 {
    mut n = 0
    val tick = move || -> i32 {
        n += 1
        n
    }
    val first = tick()        // 1
    val second = tick()       // 2, and `n` is still 0
    first + second - 3
}
```

A closure without `move` may not move a capture out (passing a captured `string` to a function that takes one by value, say), because it only borrowed it. The diagnostic asks for `move`.

### Call modes

How a closure uses its captures decides how it may be called. One that only reads them can be called any number of times. One that writes a capture is a `mut` function. A `move` closure that moves a capture out can be called once. A parameter's function type says which it accepts:

| Parameter type | Accepts | The callee may |
| --- | --- | --- |
| `f: (T) -> U` | closures that only read their captures, and named functions | call `f` any number of times |
| `f: mut (T) -> U` | those, and closures that write a capture | call `f` any number of times |
| `f: once (T) -> U` | every closure | call `f` once |

```neuro
func twice(f: mut () -> void) {
    f()
    f()
}

func finish(report: once () -> u64) -> u64 {
    report()
}

func shout(message: string) -> u64 { message.len() }

func main() -> i32 {
    mut count = 0
    {
        twice(|| -> void { count += 1 })
    }
    val banner = "done" + "!"
    val n = finish(move || -> u64 { shout(banner) })
    count + n as i32                   // 2 + 5
}
```

A closure that writes a capture passed where a plain `(T) -> U` is asked, or a `once` closure where a `mut` one is, is a type mismatch. Calling a `once` value a second time is a use of a moved value. `once` is a mode only in front of a function type; elsewhere it is an ordinary name.

### Inference

A closure parameter without an annotation takes its type from the function type its context gives it: the parameter it is passed to, the typed binding, field or place it initializes, or the position it is returned from. The return type is the annotation, else the context's, else the body's own. `.map`, `.zip` and `.reduce` on a tensor, and `.map` / `.filter` in a `for` head, hand their closure the element type:

```neuro
func twice(f: (i32) -> i32, x: i32) -> i32 { f(f(x)) }

func main() -> i32 {
    val k = 3
    val t: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val doubled = t.map(|x| x * 2.0)              // `x` is `f32`
    val total = t.reduce(0.0, |acc, x| acc + x)   // the seed types `acc`
    twice(|x| x + k, 1)                           // 7
}
```

A parameter that nothing types is an error, so `val f = |x| x + 1` alone is refused with ``closure parameter 'x' needs a type: annotate it (`|x: T| ...`), or write the closure where a function type says what it takes, such as a typed binding `val f: (T) -> R = ...` or an argument to a function-typed parameter``. A literal called where it is written takes its parameter types from the arguments, so `(|x| x + 1)(4)` needs none. A block body that takes its return type from its last expression may not `return`; write the return type if it needs one.

### Function types and higher-order functions

A closure or function value has the type `(T1, ...) -> R`. A function that takes one as a parameter and calls it is a **higher-order function**:

```neuro
func apply(v: i32, f: (i32) -> i32) -> i32 {
    f(v)
}

func main() -> i32 {
    val bump = 3
    apply(5, |x| x + bump)             // 8
}
```

A function's own name is a value of its function type too, so `apply(5, double)` passes `func double(x: i32) -> i32` directly, and `val f = double` binds it. An associated function is one through its path, `val s = Point::scale`, and so is an enum's tuple variant, its constructor: `val wrap = Error::Io`. A generic enum's variant needs a function type from its context to name its instance (`o.map(Option::Some)`, `val f: (i32) -> Option<i32> = Option::Some`). A generic function has no single type until its type arguments are named, so its bare name is refused. Name the instance with a turbofish instead: `val f = identity::<i32>`.

A higher-order function may be generic. The closure passed to it takes part in inferring its type parameters: it is typed after the call's other arguments, so it sees what they bound, and its result binds the rest:

```neuro
func map_one<T, U>(value: T, f: (T) -> U) -> U {
    f(value)
}

func main() -> i32 {
    val wide = map_one(7, |x| x as i64 * 3)   // T = i32, U = i64
    wide as i32
}
```

Any expression of function type can be called, not only a name: a closure literal called where it is written, `(|x: i32| -> i32 { x + 1 })(3)`, or a function chosen by an `if`, `(if fast { quick } else { exact })(v)`.

### Returning and storing closures

A closure that borrows nothing owns its environment, so it can be returned, stored in a struct or an enum, and moved like any other value. A plain `(T) -> U` value is also copied freely:

```neuro
struct Scaler { apply: (f32) -> f32 }

func make_adder(n: i32) -> (i32) -> i32 {
    |x| x + n                          // copies `n`
}

func scaler(factor: f32) -> Scaler {
    Scaler { apply: |x| x * factor }
}

func main() -> i32 {
    val add_two = make_adder(2)
    val again = add_two                // a copy: both call the same closure
    val half = scaler(0.5)
    val f = half.apply
    add_two(1) + again(2) + f(8.0) as i32   // 3 + 4 + 4
}
```

Every copy of a closure shares one environment, which is freed with the last copy, along with whatever a `move` closure moved into it. A closure with no captures has no environment.

### Moving and lending

Only a plain `(T) -> U` value copies, because its calls never write its environment. A `mut` or `once` value moves: `val g = f` hands the closure to `g`, and a later use of `f` is a use of a moved value. If copies of a `mut` closure shared one environment, a call through one copy could run a call through another and write that environment while the first call was still using it.

Passing a function value to a function-typed parameter does not move it. The call borrows it, shared for a plain parameter and exclusive for a `mut` one, and the caller has it back when the call returns. A `once` parameter takes the value for good:

```neuro
func twice(f: mut () -> void) {
    f()
    f()
}

func main() -> i32 {
    mut count = 0
    {
        val bump = || -> void { count += 1 }
        twice(bump)                     // lent, not moved
        bump()                          // still ours
        val other = bump                // moved
        twice(other)
    }
    count                               // 5
}
```

Calling a `mut` value borrows it exclusively too, so nothing else may reach it during the call: `f(|| f())` and `twice(f, f)` with two `mut` parameters are refused. The borrow starts once the arguments are evaluated, so `f(f(1))` is fine, because the inner call returns before the outer one starts. A closure that calls or lends a `mut` value it captured writes that capture, so the closure is `mut` itself and holds the value exclusively while it lives. A `mut` value reached through a shared `&` borrow, such as a field read through `&self`, can be neither called nor lent.

A value that may borrow lives only in the scope that made it:

- a closure that borrows a capture, or copies a reference into its environment;
- a function-typed parameter, since its caller may have passed such a closure;
- a closure that captures either.

Such a value may initialize a binding, be passed to a function-typed parameter, be read by a built-in that only calls it, or be called. It may not be returned, assigned, stored inside another value, or yielded from a block. Returning a closure over the function values a function was handed is refused with this diagnostic:

```text
error: this value may borrow from the scope that built it (a closure that borrows a capture, a function-typed parameter, or a closure holding either), so it lives only as long as that scope: bind it with `val` or `mut`, pass it to a function-typed parameter, or call it; it cannot be assigned, returned, stored inside another value, yielded from a block, or handed to a built-in method such as `.map` as a closure literal that holds a `&mut`
```

### Not yet supported

- Returning a closure over the function values a function was handed (`func compose(f: (i32) -> i32, g: (i32) -> i32) -> (i32) -> i32 { move |x| g(f(x)) }`).
- Storing a borrowing closure in a holder that dies before what it borrows.
- A closure literal that writes through a borrow, written as the argument of a built-in method (`.map`, `.filter`, `.zip`, `.reduce`). Bind it with `mut` first, so the borrow rules see it beside what the method walks. A literal that only reads what it borrows goes there directly, and a `for` head's literal keeps its borrows until the loop ends.
- A closure without `move` taking a capture by value because its body moves it out.
- An instantiated associated function or generic method as a value on its own (`val f = Cell::new::<i32>`). It works as a `|>` target and as a later `>>` stage.

## See also

- [Generics and Traits](generics-and-traits.md): generic functions, `impl Trait` and `dyn Trait` dispatch
- [Types](types.md): function types and type checking
- [Variables](variables.md): local bindings
- [Control Flow](control-flow.md): `if`, `match` and loops
- [Expressions](expressions.md): expression-based returns
- [Operators](operators.md): the `|>` pipeline and `>>` composition
