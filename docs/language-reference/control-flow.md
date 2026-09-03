# Control Flow

Control flow statements determine the execution path of your program.

## If Expressions

### Basic If

```neuro
if condition {
    // Execute if condition is true
}
```

### If-Else

```neuro
if condition {
    // Execute if condition is true
} else {
    // Execute if condition is false
}
```

### Else-If Chains

```neuro
if condition1 {
    // Execute if condition1 is true
} else if condition2 {
    // Execute if condition2 is true
} else if condition3 {
    // Execute if condition3 is true
} else {
    // Execute if all conditions are false
}
```

## Conditional Requirements

Conditions must be boolean expressions:

```neuro
// Valid conditions
if x > 0 { }
if flag { }
if a == b { }
if is_valid && is_ready { }

// Invalid conditions
// if x { }           // Error: x is i32, not bool
// if 1 { }           // Error: 1 is i32, not bool
```

## If in Functions

### Early Return

```neuro
func clamp(x: i32, min: i32, max: i32) -> i32 {
    if x < min {
        return min
    }
    if x > max {
        return max
    }
    return x
}
```

### Conditional Return

```neuro
func sign(x: i32) -> i32 {
    if x > 0 {
        return 1
    } else if x < 0 {
        return -1
    } else {
        return 0
    }
}
```

### Expression-Based Return

An `if`/`else` is an expression: its value is the trailing expression of the taken
branch. It can be a function's implicit return value or be bound to a variable. Both
branches must yield the same type, so an `else` is required when the value is used.

A branch that leaves the scope instead of producing a value (`return`, `break`,
`continue`, `panic(...)`, `unreachable()`) is exempt from that agreement: it never
reaches the point where the `if` has a value, so the other branch decides the type.
`if n > 0 { return 1 } else { 2 }` is an `i32`. The same rule applies to `match` arms.

```neuro
func abs(x: i32) -> i32 {
    if x >= 0 {
        x  // Implicit return
    } else {
        -x  // Implicit return
    }
}

func clamp_low(x: i32) -> i32 {
    val y = if x < 0 { 0 } else { x }  // bound to a variable
    y
}
```

## Block Scopes

Variables declared in if/else blocks are scoped to that block:

```neuro
func scoped() -> i32 {
    val x: i32 = 10
    if true {
        val y: i32 = 20  // y only exists in this block
        // Both x and y are accessible here
    }
    // Only x is accessible here
    // return y  // Error: y not in scope
    return x
}
```

## Nested If Statements

```neuro
func nested(a: i32, b: i32) -> i32 {
    if a > 0 {
        if b > 0 {
            return a + b
        } else {
            return a - b
        }
    } else {
        if b > 0 {
            return b - a
        } else {
            return 0
        }
    }
}
```

## While Loops

Use `while` to repeat a block while a boolean condition is true:

```neuro
while condition {
    // loop body
}
```

### While Requirements

Loop conditions must be boolean expressions:

```neuro
mut i: i32 = 0
while i < 10 {
    i = i + 1
}

// Invalid
// while 42 { }  // Error: expected bool condition
```

### `prefer-loop-over-while-true` lint

`while true { ... }` compiles and runs, but the compiler emits a warning:

```text
warning[prefer-loop-over-while-true] at 23..27: `while true { ... }` should
be written as `loop { ... }`; silence with `@allow(prefer_loop_over_while_true)`
on the enclosing function
```

The motivation is style, not safety; both forms produce identical machine
code. To silence the warning on a function (typically when transcribing code
from C, Python, or JavaScript), attach the `@allow` attribute:

```neuro
@allow(prefer_loop_over_while_true)
func main() -> i32 {
    mut i: i32 = 0
    while true {
        if i == 7 { break }
        i = i + 1
    }
    return i
}
```

The lint only triggers on the bare literal `true`. Parenthesised
`while (true) { ... }` is treated as an explicit escape hatch and is not
flagged. The recommended replacement is the `loop { ... }` statement below.

## Loop (Infinite)

Use `loop` for an infinite loop. Unlike `while`, it has no condition: the only
way out is a `break`, and `continue` re-enters the body from the top. This is
the canonical infinite-loop form the `prefer-loop-over-while-true` lint
suggests.

```neuro
mut attempts: i32 = 0
loop {
    attempts = attempts + 1
    if attempts > 5 {
        break
    }
}
```

`break` and `continue` behave exactly as they do in `while` and `for` bodies.

### Loop as a value expression

Because blocks are expressions, a `loop` can produce a value: `break v` exits
the loop and makes `v` the value of the whole `loop` expression.

```neuro
mut i: i32 = 0
val first_even = loop {
    i = i + 1
    if i % 2 == 0 {
        break i          // the loop expression evaluates to i
    }
}
```

All value-carrying `break`s for one loop must agree on type. With a label,
`break outer value` carries the value out of an outer loop, and the labeled loop
may itself be used in value position (`val x = outer: loop { ... }`).

Only `loop` can yield a value: it is the one loop guaranteed (by the absence of
a fall-through exit) to leave solely via a `break`. `while` and `for` always
evaluate to unit `()`, so a `break value` targeting one is a compile error.

## For Loops

Use `for` to iterate over an integer range. Ranges can be exclusive (`..`) or inclusive (`..=`):

```neuro
// Exclusive range: 0, 1, 2, ..., 9
for i in 0..10 {
    // i takes values 0 through 9
}

// Inclusive range: 1, 2, ..., 5
for j in 1..=5 {
    // j takes values 1 through 5
}
```

### For-Range Requirements

- Range bounds must be integer-compatible expressions.
- The iteration variable is implicitly declared and its type is inferred from the range bounds.

```neuro
func sum_first_five() -> i32 {
    mut sum: i32 = 0
    for i in 0..=5 {
        sum = sum + i // 0 + 1 + 2 + 3 + 4 + 5 = 15
    }
    return sum
}
```

### Iterating with a Position — `.enumerate()`

A `for` head may bind a pair instead of a single variable when the iterable ends
in `.enumerate()`. The first binding is the **position** — a `u64` counting from
zero — and the second is the element:

```neuro
val scores: [i32; 3] = [5, 4, 3]

for (place, score) in scores.enumerate() {
    println("place {place} scored {score}")
}
```

The position is a `u64` because that is what an index expression and `.len()`
use, so it reads back into the sequence it walks without a cast:

```neuro
for (i, score) in scores.enumerate() {
    val same = scores[i] == score      // true, every iteration
}
```

`.enumerate()` applies to a fixed-size array, a `Vec<T>`, a borrow of either, and
a range. A range must be parenthesised, because `..` binds looser than a method
call — `0..n.enumerate()` would enumerate `n`:

```neuro
for (step, value) in (10..13).enumerate() {
    // step: 0, 1, 2   value: 10, 11, 12
}
```

The position is a count of iterations, not a value the sequence holds — which is
why the two columns above differ.

The pair head and `.enumerate()` imply each other: a pair head over a plain
iterable, or `.enumerate()` under a single-variable head, is a parse error. Both
bindings live in the loop's own scope, so they shadow any outer name of the same
spelling for the loop's duration and disappear at its exit; naming them alike
(`for (i, i) in ...`) is a redefinition error rather than a shadow.

### The iteration protocol

`for` is not limited to the built-in sequences. `for x in e` means:

1. call `e.into_iter()` once, producing an iterator;
2. call `.next()` on that iterator repeatedly;
3. bind each `Some(v)` to the loop variable and run the body;
4. stop when `.next()` answers `None`.

Two prelude traits state the contract, so no import is needed:

```neuro
trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

trait IntoIterator {
    type Item
    type Iter
    func into_iter(self) -> Self::Iter
}
```

Any type implementing either one may stand in a `for` head. A type that
implements `Iterator` **is** its own iterator — no second `IntoIterator` impl is
needed, and the loop uses the value directly:

```neuro
@derive(Copy, Clone)
struct Countdown { remaining: i32 }

impl Iterator for Countdown {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        if self.remaining <= 0 { return Option::None }
        self.remaining = self.remaining - 1
        Option::Some(self.remaining)
    }
}

val c = Countdown { remaining: 3 }
for n in c {
    println("{n}")          // 2, 1, 0
}
```

Implement `IntoIterator` when the container and the cursor are different types —
which is what lets a container be walked more than once, since each `for` head
asks it for a fresh cursor.

The loop variable's type is the `Item` the iterator's own `impl Iterator` bound,
`break` and `continue` work as they do in any other loop (a label lands on the
`for` and reaches out of a nested one), and `.enumerate()` applies here too, its
position counting the loop's own steps.

**Adapters.** An adapter is an iterator that wraps another one, so it composes in
a single `for` head. The `Iterator<Item = T>` bound on its type parameter is what
lets its body call `.next()` on what it holds:

```neuro
@derive(Copy, Clone)
struct Doubling<S> { inner: S }

impl<S: Iterator<Item = i32>> Iterator for Doubling<S> {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        match self.inner.next() {
            Option::Some(v) => Option::Some(v * 2),
            Option::None => Option::None
        }
    }
}

val doubled = Doubling { inner: Countdown { remaining: 3 } }
for n in doubled {
    println("{n}")          // 4, 2, 0
}
```

Nothing between the source and the loop is materialized: each step pulls one
element through the whole chain.

**What is not covered.** The built-in heads — a range, a fixed-size array, a
`Vec<T>`, a borrowed slice — do **not** go through the protocol. They compile to
counted loops directly, which is a lowering choice, not a difference in meaning.

### Adapter methods — `.map(f)` and `.filter(p)`

A `for` head may end in a chain of `.map(f)` / `.filter(p)` calls, which need no
adapter type of their own:

- `.map(f)` replaces each element with `f(element)`;
- `.filter(p)` drops each element for which `p(element)` is false.

```neuro
for value in [1, 2, 3, 4].map(|x: i32| -> i32 { x * 2 }) {
    println("{value}")        // 2, 4, 6, 8
}

for value in (0..12).filter(|x: i32| -> bool { x % 4 == 0 }) {
    println("{value}")        // 0, 4, 8
}
```

The chain runs left to right — a filter sees what the map before it produced —
and composes to any depth. Each element is pulled through the whole chain one at
a time; nothing in between is materialized:

```neuro
for value in [1, 2, 3, 4, 5]
    .map(|x: i32| -> i32 { x * x })
    .filter(|x: i32| -> bool { x > 4 }) {
    println("{value}")        // 9, 16, 25
}
```

`.map` may change the element type, so the loop binding is whatever the function
returns. The function is an ordinary expression of function type — usually a
closure literal — evaluated **once**, before the loop, in the scope around it: it
cannot name the binding it feeds, and a closure binding used by a head is still
usable afterwards.

Like `.enumerate()`, these apply to every head: a range (parenthesised), a
fixed-size array, a `Vec<T>`, a borrow of either, and any type that implements
`IntoIterator` or `Iterator`. `.enumerate()` stays outermost, and its position
counts what the chain **yielded** — so it stays dense however much a filter drops:

```neuro
for (rank, value) in [10, 3, 20, 4, 30].filter(|x: i32| -> bool { x >= 10 }).enumerate() {
    println("{rank}: {value}")     // 0: 10   1: 20   2: 30
}
```

A `.filter` predicate must return `bool` and a `.map` function must return a
value; a function that does not take the element type is rejected where the head
is written.

**Head form, not a value.** `.map` / `.filter` are part of the `for` head, exactly
as `.enumerate()` is — `val m = xs.map(f)` is not a method call that resolves.
Write the adapter type, as above, when the pipeline has to be stored or returned.

## Break and Continue

Use `break` to exit the nearest loop and `continue` to skip to the next iteration:

```neuro
while condition {
    if should_stop {
        break
    }

    if should_skip {
        continue
    }

    // normal loop body work
}
```

### Break/Continue Requirements

Both statements are only valid inside loops:

```neuro
func valid() -> i32 {
    mut i: i32 = 0
    while i < 10 {
        i = i + 1
        if i == 5 {
            break
        }
    }
    return i
}

// Invalid
// func invalid() -> i32 {
//     break      // Error: break used outside of a loop
//     return 0
// }
```

### Loop Labels

A `for`, `while`, or `loop` may be prefixed with a label, an identifier
followed by a colon (`outer:`). `break label` and `continue label` then target
the labeled loop rather than the innermost one, so an inner loop can exit or
re-enter an outer loop directly:

```neuro
func count() -> i32 {
    mut total: i32 = 0
    outer: for i in 0..5 {
        for j in 0..5 {
            total = total + 1
            if i + j >= 3 {
                break outer       // exits BOTH loops
            }
        }
    }
    return total
}
```

`continue label` re-enters the labeled loop's next iteration:

```neuro
outer: for i in 0..3 {
    for j in 0..3 {
        if j == 1 {
            continue outer        // skip to the outer loop's next i
        }
    }
}
```

A label on `break` / `continue` must name an enclosing loop; an unknown label
is a compile error (`use of undefined loop label`).

## Examples

### Range Check

```neuro
func in_range(x: i32, min: i32, max: i32) -> bool {
    if x >= min && x <= max {
        true
    } else {
        false
    }
}
```

### Maximum of Three

```neuro
func max3(a: i32, b: i32, c: i32) -> i32 {
    if a >= b && a >= c {
        a
    } else if b >= c {
        b
    } else {
        c
    }
}
```

### Grade Calculator

```neuro
func letter_grade(score: i32) -> i32 {
    if score >= 90 {
        return 4  // A
    } else if score >= 80 {
        return 3  // B
    } else if score >= 70 {
        return 2  // C
    } else if score >= 60 {
        return 1  // D
    } else {
        return 0  // F
    }
}
```

## Best Practices

### Prefer Early Returns

```neuro
// Good: early returns
func process(x: i32) -> i32 {
    if x < 0 {
        return 0
    }
    if x > 100 {
        return 100
    }
    return x * 2
}

// Less clear: nested conditions
func process_nested(x: i32) -> i32 {
    if x >= 0 {
        if x <= 100 {
            return x * 2
        } else {
            return 100
        }
    } else {
        return 0
    }
}
```

### Simplify Boolean Conditions

```neuro
// Good: direct boolean return
func is_positive(x: i32) -> bool {
    x > 0
}

// Unnecessary: explicit true/false
func is_positive_verbose(x: i32) -> bool {
    if x > 0 {
        return true
    } else {
        return false
    }
}
```

## Pattern Matching

`match` selects a branch by deconstructing a value, and is itself an expression.
The first arm whose pattern matches (and whose optional `if` guard holds) runs:

```neuro
func classify(n: i32) -> i32 {
    match n {
        0            => 1,        // literal
        1 | 2        => 2,        // or-pattern
        3..=9        => 3,        // inclusive range
        n if n < 0   => 4,        // bare binding + guard
        _            => 9         // wildcard
    }
}
```

An arm body is an expression, and may also be a bare `return`, `break`, or
`continue` — the statement that leaves the enclosing function or loop instead of
producing a value for the arm:

```neuro
func first_even(limit: i32) -> i32 {
    mut i: i32 = 0
    loop {
        i = i + 1
        match i % 2 {
            0 => break i,
            _ => continue
        }
        if i > limit { return 0 - 1 }
    }
}
```

Enum variants deconstruct and bind their payloads (`E::Tuple(a)`,
`E::Struct { field }`). A `match` must be exhaustive. See
[Expressions → Match Expressions](expressions.md) for the full pattern grammar,
exhaustiveness rules, and current Phase-1E limits.

## `val-else`, Unwrap or Leave the Scope

`val PATTERN = value else { ... }` binds a refutable pattern and hands the failure to
an `else` branch. The bindings are live for the **rest of the enclosing block**, not
just one arm, which is what makes it the straight-line alternative to a `match` whose
success arm would otherwise swallow the whole function:

```neuro
func doubled_or_error(raw: i32) -> i32 {
    val Result::Ok(value) = parse(raw) else |err| { return err }
    value + 1        // `value` is in scope from here on
}
```

The `else` branch **must exit the scope**: `return`, `break`, `continue`,
`panic(...)`, or `unreachable()`. A branch that can fall through is rejected, so the
binding is guaranteed initialized on the path that continues:

```
error: the `else` branch of a `val-else` can fall through: it must exit the scope
       with `return`, `break`, `continue`, `panic(...)`, or `unreachable()`
```

### The `else |binding|` form

Writing `else |name|` after `else` binds the value the failure carried. This is a
dedicated `val-else` production, **not** a closure literal. What `name` refers to is
decided by the scrutinee's type:

| Scrutinee type | `else \|name\|` binds |
| --- | --- |
| `Result<T, E>` | the `Err` payload (`Result::Err(e)` → `e: E`) |
| `Option<T>` | nothing, `None` is empty, so only `\|_\|` (or a bare `else`) is accepted |
| any other enum | the original scrutinee, unmodified, for a nested `match` |

`Option` and `Result` have exactly one "other" variant, so the payload to unwrap is
unambiguous. A general enum has several, so `|name|` hands back the whole value and the
branch discriminates further:

```neuro
func area(s: Shape) -> i32 {
    val Shape::Circle { radius } = s else |other| {
        match other {
            Shape::Square(side) => { return side * side },
            _ => { return 0 }
        }
    }
    radius * 3
}
```

Naming an `Option`'s binding is rejected, since there is nothing to name:

```
error: `else |e|` has nothing to bind: `Option::None` carries no payload;
       write `else` or `else |_|`
```

Because `break` counts as leaving the scope, `val-else` is also the natural drain loop:

```neuro
loop {
    val Option::Some(v) = next(i) else { break }
    total = total + v
    i = i + 1
}
```

Runnable program: [`examples/control_flow/val_else.nr`](../../examples/control_flow/val_else.nr).

## References

- [Expressions](expressions.md) - Boolean expressions
- [Operators](operators.md) - Comparison and logical operators
- [Variables](variables.md) - Variable scope in blocks
