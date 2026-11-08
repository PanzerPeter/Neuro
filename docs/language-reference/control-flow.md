# Control Flow

Control flow decides which code runs and how often. `if` and `match` pick a branch, `while`, `loop` and `for` repeat a body, and `break`, `continue` and `return` leave one early. Most of these are expressions, so they can produce a value.

## If and Else

```neuro
func sign(x: i32) -> i32 {
    if x > 0 {
        1
    } else if x < 0 {
        -1
    } else {
        0
    }
}
```

A condition must be a `bool`. `if x { }` with `x: i32` fails with `type mismatch: expected bool, found i32`, and `while 42 { }` fails the same way.

### If as an Expression

An `if`/`else` is an expression: its value is the trailing expression of the branch taken. It can be a function's implicit return value or be bound to a variable. Both branches must yield the same type, so an `else` is required when the value is used:

```neuro
func abs(x: i32) -> i32 {
    if x >= 0 { x } else { -x }
}

func clamp_low(x: i32) -> i32 {
    val y = if x < 0 { 0 } else { x }
    y
}
```

Without an `else` the `if` is unit, and binding it is an error: `val z = if c { 1 }` fails with `cannot bind 'z': the initializer has type void, which is not a value`. A mismatch fails with `type mismatch: expected i32, found string` for `if x > 2 { 1 } else { "no" }`.

A branch that leaves the scope instead of producing a value (`return`, `break`, `continue`, `panic(...)`, `unreachable()`) is exempt from that agreement. It never reaches the point where the `if` has a value, so the other branch decides the type. The same rule applies to `match` arms.

```neuro
func or_two(n: i32) -> i32 {
    if n > 0 { return 1 } else { 2 }
}
```

## Blocks and Unsafe Blocks

### Blocks as Values

A bare `{ }` is an expression too: its trailing expression is its value.

```neuro
func area() -> i32 {
    val total = {
        val w: i32 = 6
        val h: i32 = 7
        w * h
    }
    total
}
```

### Unsafe Blocks

`unsafe { }` is a block expression with the same value rules. Today it has one effect: inside a `@kernel` body, reading or writing an output element by index must sit in an `unsafe` block, which marks each place where you vouch that no other thread writes the same element. Everywhere else it behaves like a bare block. See [Unsafe Block Expressions](expressions.md#unsafe-block-expressions) and [Writing a kernel](tensors.md#writing-a-kernel-kernel).

```neuro
func main() -> i32 {
    val base = unsafe {
        val a = 10
        a + 7
    }
    base
}
```

## Loops

### While

`while` repeats its body while a `bool` condition holds. It is a statement and yields unit.

```neuro
mut i: i32 = 0
while i < 10 {
    i = i + 1
}
```

#### The `prefer_loop_over_while_true` Lint

`while true { ... }` compiles and runs, but the compiler warns:

```text
warning[prefer_loop_over_while_true] at 23..27: `while true { ... }` should be written as `loop { ... }`; silence with `@allow(prefer_loop_over_while_true)` on the enclosing function
```

The lint has one name, in `snake_case`, and that is the name `@allow` takes. The motivation is style, not safety. The lint fires only on the bare literal `true`: a parenthesised `while (true) { ... }` is not flagged, and neither is a [`while val`](#if-val-and-while-val) loop. The replacement is [`loop`](#loop). To silence the warning on a function (typically when transcribing code from C, Python or JavaScript), attach the `@allow` attribute:

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

### Loop

`loop` is an infinite loop. It has no condition: the only way out is a `break`, and `continue` re-enters the body from the top.

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

#### Loop Values

Because blocks are expressions, a `loop` can produce a value: `break v` exits the loop and makes `v` the value of the whole `loop` expression.

```neuro
mut i: i32 = 0
val first_even = loop {
    i = i + 1
    if i % 2 == 0 {
        break i          // the loop expression evaluates to i
    }
}
```

All value-carrying `break`s for one loop must agree on type. A `break` with a label carries its value out of an outer loop (`break 'outer value`), and a labeled loop can itself be used as a value: `val x = 'outer: loop { ... }`.

Only `loop` yields a value, since it is the one loop with no exit except a `break`. `while` and `for` are statements that yield unit, so a `break` with a value inside one is rejected with `'break' with a value is only allowed in a 'loop'`.

### For Over Ranges

Use `for` to iterate over an integer range. Ranges are exclusive (`..`) or inclusive (`..=`):

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

The iteration variable is declared by the loop, and its type is inferred from the range bounds. The bounds must be integer expressions.

### Iterating Backwards: `.rev()`

`.rev()` on a range walks the same bounds from the top down. Like `.enumerate()`, a range must be parenthesised, because `..` binds looser than a method call:

```neuro
for i in (0..5).rev() {
    // i takes values 4, 3, 2, 1, 0
}

for i in (0..=5).rev() {
    // i takes values 5, 4, 3, 2, 1, 0
}
```

The two spellings differ where they always do: `5` is not in `0..5`, so reversing it starts at `4`; it is in `0..=5`, so reversing that starts at `5`. An empty range is empty in either direction.

`.rev()` applies to a **range only**: it reorders the bounds themselves, not an element stream. `xs.rev()` over an array or a `Vec<T>` fails with ``.rev() reverses a range, so its receiver must be one: write `(start..end).rev()` ``. It composes with everything else a head may carry, and sits innermost, beneath the adapters and `.enumerate()`:

```neuro
for (rank, value) in (10..14).rev().enumerate() {
    // rank:  0,  1,  2,  3     value: 13, 12, 11, 10
}
```

The position still counts iterations upward while the value descends, for the same reason it counts from zero over any other head: it is a position, not a value the range holds.

A range index into a tensor takes `.rev()` too, reading that axis back to front; see [Slicing and indexing](tensors.md#slicing-and-indexing).

### Striding: `.step(n)`

`.step(n)` on a range keeps every `n`th value of the walk. Like `.rev()` it applies to a parenthesised range only, and `.rev()` goes beneath it: the reversed walk starts at the last value and strides down.

```neuro
for i in (0..10).step(3) {
    // i takes values 0, 3, 6, 9
}

for i in (0..=12).step(4) {
    // 0, 4, 8, 12: an inclusive range still yields a bound the stride lands on
}

for i in (0..10).rev().step(3) {
    // 9, 6, 3, 0
}
```

The stride is an integer of the range's own type and must be positive. A constant stride below one is a compile error (``.step(n) must be a positive integer, found 0``); a stride computed at run time is checked when the loop starts, and a zero one panics rather than looping forever. The loop never adds past its bound, so `(0u8..255u8).step(10)` ends at `250` instead of wrapping. The other order, `(a..b).step(n).rev()`, walks different values and is rejected; write `.rev().step(n)`.

`.step(n)` sits beneath the adapters and `.enumerate()` exactly as `.rev()` does, and a tensor index takes it too (`t[(0..n).step(2)]`).

### Ranges as Values

A range written anywhere other than a `for` head or a slice argument is a value of the standard library's `Range<T>`, where `T` is the bounds' integer type. It binds, passes and returns like any struct, and it is an iterator, so a `for` loop, the adapter values and a generic `I: Iterator<Item = T>` parameter all take it:

```neuro
func evens(n: i32) -> Step<i32> {
    (0..n).step(2)
}

func main() -> i32 {
    val window = 3..7                  // Range<i32>
    mut total = 0
    for x in window { total += x }     // 3, 4, 5, 6
    for x in evens(9) { total += x }   // 0, 2, 4, 6, 8
    total
}
```

`.rev()` on a `Range<T>` returns a `Rev<T>`, `.step(n)` on either returns a `Step<T>`, and they walk exactly what the head forms above walk, including the last value of a `u8` and the first of an `i8`. The stride follows the head form's rules: a constant below one is a compile error, and a zero computed at run time panics at the `.step` call. A `Step` has no `.rev()`, so `.step(n).rev()` is refused here too.

A range value moves: `for x in r` consumes `r`, and a second loop over it is a use of a moved value. To walk part of one and keep the rest, borrow it mutably. `for x in &mut r` drives it in place, a `break` leaves it where it stopped, and `.rev()` or `.step(n)` afterwards walk only what is left:

```neuro
mut r = 0..10
for x in &mut r {
    if x == 3 { break }               // 0, 1, 2, then 3 ends the loop
}
for x in r.rev() { }                  // 9, 8, 7, 6, 5, 4
```

Written directly on a range literal in a `for` head, `.rev()` and `.step(n)` are still the head forms and build no value.

### Iterating a Collection: `xs`, `&xs` and `&mut xs`

A fixed-size array, a `Vec<T>` and a slice iterate element by element. How the head is written decides what the loop variable is, the same three receiver modes a method has:

```neuro
mut scores: Vec<i32> = Vec::new()
scores.push(1)
scores.push(2)

mut total = 0
for s in &scores {          // s: &i32, scores stays usable
    total += s
}
for s in &mut scores {      // s: &mut i32, writes the element in place
    *s += 10
}
for s in scores {           // s: i32, scores is consumed
    println("{s}")
}
```

- `for x in &xs` binds a `&T` to each element. Operators and method calls read through it, so `total += x` and `x.len()` need no `*`; a `val n: i32 = x` does.
- `for x in &mut xs` binds a `&mut T`. `*x = v`, `*x += v`, `x.field = v` and a `&mut self` method call all reach the element. The head must be a `mut` binding; an immutable one fails with ``cannot mutably borrow 'v': `&mut` requires a `mut` binding``.
- `for x in xs` moves each element out to `x` and consumes `xs`. A `Vec` is gone after the loop whatever its element type; an array of `Copy` elements is itself `Copy`, so the loop walks a copy and the array stays usable.

A slice is already a borrow, so `for x in xs` over a `&[T]` binds `&T`, and over a `&mut [T]` binds `&mut T`. A borrowed head stays borrowed for the whole loop, so the body cannot push to the `Vec` it walks, or read it while a `&mut` loop is running.

A `HashMap` or `BTreeMap` iterates entry by entry in the same three modes. Each pass yields a `(key, value)` pair, which a pair head takes apart:

```neuro
for (name, age) in &ages { }       // name: &string, age: &i32
for (name, age) in &mut ages { }   // age: &mut i32; the key is never lent mutably
for (name, age) in ages { }        // string, i32; ages is consumed
for name in ages.keys() { }        // name: &string
for age in ages.values() { }       // age: &i32
```

A `BTreeMap` walks in ascending key order; a `HashMap` promises no order. `.keys()` and `.values()` are head forms that lend one half of each entry, and they take the adapters and `.enumerate()` like any other head. See [Types](types.md#hashmapk-v-and-btreemapk-v) for the map surface.

### Pair Heads

A `for` head may bind a pair `(a, b)` instead of a single variable. Over `.enumerate()` and `.char_indices()` the first binding is a position; over any other head, the item itself must be a two-element tuple, and the pair takes it apart, as `val (a, b) = item` would. A map's entries, an array of pairs and a stored `Enumerate` value all qualify, and `_` skips a half:

```neuro
val pairs = [(1, 2), (3, 4)]
for (a, _) in pairs { }
```

A pair head over anything else is refused with ``a `for` head binding a pair `(a, b)` needs a two-element tuple item``, which names `.enumerate()` for the case where a position was meant.

### Iterating with a Position: `.enumerate()`

A `for` head may bind a pair instead of a single variable when the iterable ends in `.enumerate()`. The first binding is the **position** (a `u64` counting from zero), and the second is the element:

```neuro
val scores: [i32; 3] = [5, 4, 3]

for (place, score) in scores.enumerate() {
    println("place {place} scored {score}")
}
```

The position is a `u64` because that is what an index expression and `.len()` use, so it reads back into the sequence it walks without a cast:

```neuro
for (i, score) in scores.enumerate() {
    val same = scores[i] == score      // true, every iteration
}
```

`.enumerate()` applies to a fixed-size array, a `Vec<T>`, a borrow of either, and a range. A range must be parenthesised, because `..` binds looser than a method call: `0..n.enumerate()` would enumerate `n`:

```neuro
for (step, value) in (10..13).enumerate() {
    // step: 0, 1, 2   value: 10, 11, 12
}
```

The position is a count of iterations, not a value the sequence holds, which is why the two columns above differ.

`.enumerate()` needs a pair head: under a single variable it is a parse error. Both bindings live in the loop's own scope, so they shadow any outer name of the same spelling for the loop's duration and disappear at its exit. Naming them alike (`for (i, i) in ...`) is a redefinition error, ``variable 'i' already defined in this scope``, rather than a shadow.

### Adapter Methods: `.map(f)` and `.filter(p)`

A `for` head may end in a chain of `.map(f)` / `.filter(p)` calls, which need no adapter type of their own:

- `.map(f)` replaces each element with `f(element)`;
- `.filter(p)` drops each element for which `p(element)` is false.

```neuro
for value in [1, 2, 3, 4].map(|x| x * 2) {
    println("{value}")        // 2, 4, 6, 8
}

for value in (0..12).filter(|x| x % 4 == 0) {
    println("{value}")        // 0, 4, 8
}
```

A closure literal's parameter takes the type of the element it is handed, so it needs no annotation.

The chain runs left to right (a filter sees what the map before it produced) and composes to any depth. Each element is pulled through the whole chain one at a time; nothing in between is materialized:

```neuro
for value in [1, 2, 3, 4, 5]
    .map(|x| x * x)
    .filter(|x| x > 4) {
    println("{value}")        // 9, 16, 25
}
```

`.map` may change the element type, so the loop binding is whatever the function returns. The function is an ordinary expression of function type (usually a closure literal) evaluated **once**, before the loop, in the scope around it: it cannot name the binding it feeds, and a closure binding used by a head is still usable afterwards.

Like `.enumerate()`, these apply to every head: a range (parenthesised), a fixed-size array, a `Vec<T>`, a borrow of either, and any type that implements `IntoIterator` or `Iterator`. `.enumerate()` stays outermost, and its position counts what the chain **yielded**, so it stays dense however much a filter drops:

```neuro
for (rank, value) in [10, 3, 20, 4, 30].filter(|x: i32| -> bool { x >= 10 }).enumerate() {
    println("{rank}: {value}")     // 0: 10   1: 20   2: 30
}
```

A `.filter` predicate must return `bool` and a `.map` function must return a value; a function that does not take the element type is rejected where the head is written.

**Head form.** Written in a `for` head, `.map` / `.filter` fold into the loop, as `.enumerate()` does, and build no value. On an iterator outside a `for` head, a [range value](#ranges-as-values) included, the same calls return an adapter value that can be stored or returned (see [Adapter values](#the-iteration-protocol)). The head form's predicate is handed the element itself, while an adapter value's takes a borrow of it; an unannotated closure reads the same either way.

### The Iteration Protocol

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

Any type implementing either one may stand in a `for` head. A type that implements `Iterator` **is** its own iterator: no second `IntoIterator` impl is needed, and the loop uses the value directly:

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

func count_down() {
    val c = Countdown { remaining: 3 }
    for n in c {
        println("{n}")      // 2, 1, 0
    }
}
```

Implement `IntoIterator` when the container and the cursor are different types, which is what lets a container be walked more than once, since each `for` head asks it for a fresh cursor.

`for x in &mut it` drives an iterator in place instead of consuming it: `next` takes `&mut self` either way. A `break` leaves `it` where it stopped, and the next loop over it carries on from there:

```neuro
mut it = Countdown { remaining: 5 }
for n in &mut it {
    if n == 3 { break }     // 4, 3
}
for n in it {
    println("{n}")          // 2, 1, 0
}
```

A shared borrow `&it` is not a head, since `next` cannot run through it, and neither is a borrow of a type that only implements `IntoIterator`: its impl names the owned type.

The loop variable's type is the `Item` the iterator's own `impl Iterator` bound. `break` and `continue` work as they do in any other loop (a label lands on the `for` and reaches out of a nested one), and `.enumerate()` applies here too, its position counting the loop's own steps.

**Adapter types.** An adapter is an iterator that wraps another one, so it composes in a single `for` head. The `Iterator<Item = T>` bound on its type parameter is what lets its body call `.next()` on what it holds:

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

func double_down() {
    val doubled = Doubling { inner: Countdown { remaining: 3 } }
    for n in doubled {
        println("{n}")      // 4, 2, 0
    }
}
```

Nothing between the source and the loop is materialized: each step pulls one element through the whole chain.

**Adapter values.** On an iterator, `.map(f)`, `.filter(p)` and `.enumerate()` do that wrapping themselves. They return the prelude adapters `Map<I, A, B>`, `Filter<I, A>` and `Enumerate<I, A>`, where `I` is the iterator, `A` its `Item` and `B` what `f` returns. The result is an ordinary value and an iterator itself: it chains, binds, and is passed and returned under its written type:

```neuro
func evens_doubled(start: i32) -> Map<Filter<Countdown, i32>, i32, i32> {
    Countdown { remaining: start }.filter(|n| n % 2 == 0).map(|n| n * 2)
}

func show_evens() {
    for n in evens_doubled(6) {
        println("{n}")      // 8, 4, 0
    }
}
```

- The call consumes the iterator and moves the function in. A closure that writes what it captured can drive an adapter; one that can run only once is refused, since the adapter calls it per element.
- `.filter`'s predicate takes `&A`, a borrow of the element it may keep. Operators read through it, so `|n| n % 2 == 0` needs no `*`.
- `.enumerate()` yields `(u64, A)` pairs, counting from zero what it yielded. A [pair head](#pair-heads) takes them apart: `val e = it.enumerate(); for (i, x) in e`.
- A type parameter bounded by `Iterator<Item = T>` takes the adapters too, and is a `for` head by that bound.
- A type's own method of the same name wins. An array, a `Vec` or a range is not an iterator value, so outside a `for` head the adapters do not apply to it.

**What is not covered.** The built-in heads (a range, a fixed-size array, a `Vec<T>`, a borrowed slice) do **not** go through the protocol. They compile to counted loops directly, which is a lowering choice, not a difference in meaning.

### Walking Text: `.chars()` and `.char_indices()`

A `string` is UTF-8, so its characters are not its bytes. `.chars()` is an ordinary iterator over Unicode scalar values and stands in a `for` head like any other, with `.enumerate()` and the adapters composing over it. `.char_indices()` is the second **pair-yielding head form**: it binds the byte offset of each scalar beside it, which is the offset `.slice(range)` takes.

```neuro
for c in "héllo".chars() { }                       // 5 scalars, 6 bytes

for (offset, c) in "héllo".char_indices() {
    // offset: 0, 1, 3, 4, 5 ('é' occupies two bytes)
}
```

A `.char_indices()` head binds a pair and only a pair, and it takes no `.enumerate()` and no adapters: it already carries a position. See [Strings](strings.md#string-methods) for the method surface behind both.

## Break and Continue

Use `break` to exit the nearest loop and `continue` to skip to its next iteration:

```neuro
func first_big_odd(xs: [i32; 5]) -> i32 {
    mut found: i32 = 0
    for x in xs {
        if x > 100 { break }
        if x % 2 == 0 { continue }
        found = x
    }
    found
}
```

Both statements are valid only inside a loop. `break` elsewhere fails with `'break' used outside of a loop`.

### Loop Labels

A `for`, `while`, or `loop` may be prefixed with a label: a quote, a name and a colon (`'outer:`). `break 'outer` and `continue 'outer` then target the labeled loop rather than the innermost one, so an inner loop can exit or re-enter an outer loop directly:

```neuro
func count() -> i32 {
    mut total: i32 = 0
    'outer: for i in 0..5 {
        for j in 0..5 {
            total = total + 1
            if i + j >= 3 {
                break 'outer      // exits BOTH loops
            }
        }
    }
    return total
}
```

`continue 'outer` re-enters the labeled loop's next iteration:

```neuro
'outer: for i in 0..3 {
    for j in 0..3 {
        if j == 1 {
            continue 'outer       // skip to the outer loop's next i
        }
    }
}
```

- Only `for`, `while` and `loop` take a label. A label on any other statement, such as `if`, is a parse error.
- A label on `break` or `continue` must name an enclosing loop. An unknown one fails with ``use of undefined loop label `'nope` ``.
- The quote is what marks a label, so `break outer` without it carries the value of a binding named `outer`, and a label and a binding of the same name never clash. A label written without its quote (`outer: for`) is rejected with a message showing the quoted spelling.
- A nested loop may reuse an enclosing label. `break` and `continue` then target the innermost loop carrying it.

## Pattern Matching

`match` selects a branch by deconstructing a value, and is itself an expression. The first arm whose pattern matches (and whose optional `if` guard holds) runs:

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

An arm body is an expression, and may also be a bare `return`, `break`, or `continue`: the statement that leaves the enclosing function or loop instead of producing a value for the arm:

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

The `|` alternative belongs to the arm: `1 | 2 =>` is one arm with two patterns. An or-pattern nested in a payload (`Some(1 | 2)`) is a parse error, as it is in [`if val`](#if-val-and-while-val).

Enum variants deconstruct and bind their payloads (`E::Tuple(a)`, `E::Struct { field }`). A `match` must be exhaustive. See [Expressions → Match Expressions](expressions.md#match-expressions) for the full pattern grammar, exhaustiveness rules, and current limits.

### Patterns Inside Patterns

A payload takes any pattern, so one arm can test a value or another enum inside a variant. A struct variant field takes one after a colon, and `..` skips the rest:

```neuro
match result {
    Ok(Some(0))                    => 1,    // tests the value
    Ok(Some(n))                    => n,    // nests another enum
    Ok(None)                       => 2,
    Err(_)                         => 3
}

match shape {
    Shape::Rect { w: 1, h }        => h,
    Shape::Rect { w, .. }          => w,
    Shape::Dot                     => 0
}
```

Alternatives may bind, as long as each binds the same names at the same types. The arm has one body, and it sees the bindings of whichever alternative matched:

```neuro
match token {
    Tok::Num(x) | Tok::Neg(x) if x > 10 => x * 2,
    Tok::Num(x) | Tok::Neg(x)           => x,
    Tok::End                            => 0
}
```

With a guard, `A | B if g` means `A if g, B if g`: a failed guard moves on to the next alternative, so the guard can run more than once.

### `@` Bindings

`name @ pattern` matches what the pattern matches and also binds the whole value at that position:

```neuro
match code {
    n @ 1..=9 => n * 10,
    other     => other
}

match reading {
    Some(n @ 1..=9) => n,
    Some(_) | None  => 0
}
```

When the value is not `Copy`, the pattern after `@` may not bind anything, because the value cannot sit in two names at once. Use `_` inside it: `whole @ Some(_)`.

### Array Patterns

An array pattern matches a `[T; N]` or a borrowed slice `&[T]`. It lists leading elements, each any pattern, and may close with `..` or `..rest`:

```neuro
func sum(s: &[i32]) -> i32 {
    match s {
        [] => 0,
        [head, ..rest] => head + sum(rest)
    }
}
```

Over a slice, the pattern tests the length, and `rest` is a shorter `&[T]`. Over `[T; N]`, `rest` is a `[T; N - k]` value, and a pattern that cannot fit `N` is a compile error. A `Vec` is matched through a slice: `val s: &[i32] = &v`.

Binding an element by value moves it, so an owning element cannot leave a borrowed slice. Bind it with `_` or take the tail with `..rest`.

Runnable program: [`examples/control_flow/advanced_patterns.nr`](../../examples/control_flow/advanced_patterns.nr).

## `val-else`, Unwrap or Leave the Scope

`val PATTERN = value else { ... }` binds a refutable pattern and hands the failure to an `else` branch. The bindings are live for the **rest of the enclosing block**, not just one arm, so it is the straight-line alternative to a `match` whose success arm would otherwise swallow the whole function:

```neuro
func parse(raw: i32) -> Result<i32, i32> {
    if raw > 0 {
        Result::Ok(raw * 2)
    } else {
        Result::Err(9)
    }
}

func doubled_or_error(raw: i32) -> i32 {
    val Result::Ok(value) = parse(raw) else |err| { return err }
    value + 1        // `value` is in scope from here on
}
```

An array pattern over a slice is refutable, so it takes an `else` too: `val [first, ..] = s else { return 0 }`. The bindings are `val`s, so they may shadow a name of the block, the scrutinee's included.

The `else` branch **must exit the scope**: `return`, `break`, `continue`, `panic(...)`, or `unreachable()`. A branch that can fall through is rejected, so the binding is guaranteed initialized on the path that continues:

```text
error: the `else` branch of a `val-else` can fall through: it must exit the scope
       with `return`, `break`, `continue`, `panic(...)`, or `unreachable()`
```

### The `else |binding|` Form

Writing `else |name|` after `else` binds the value the failure carried. This is a dedicated `val-else` production, **not** a closure literal. What `name` refers to is decided by the scrutinee's type:

| Scrutinee type | `else \|name\|` binds |
| --- | --- |
| `Result<T, E>` | the `Err` payload (`Result::Err(e)` → `e: E`) |
| `Option<T>` | nothing: `None` is empty, so only `\|_\|` (or a bare `else`) is accepted |
| any other enum | the original scrutinee, unmodified, for a nested `match` |

`Option` and `Result` have exactly one "other" variant, so the payload to unwrap is unambiguous. A general enum has several, so `|name|` hands back the whole value and the branch discriminates further:

```neuro
enum Shape { Circle { radius: i32 }, Square(i32), Dot }

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

```text
error: `else |e|` has nothing to bind: `Option::None` carries no payload;
       write `else` or `else |_|`
```

Because `break` counts as leaving the scope, `val-else` is also the natural drain loop:

```neuro
func next(i: i32) -> Option<i32> {
    if i < 3 { Option::Some(i * 10) } else { Option::None }
}

func drain() -> i32 {
    mut i: i32 = 0
    mut total: i32 = 0
    loop {
        val Option::Some(v) = next(i) else { break }
        total = total + v
        i = i + 1
    }
    total
}
```

Runnable program: [`examples/control_flow/val_else.nr`](../../examples/control_flow/val_else.nr).

## `if val` and `while val`

`if val PATTERN = value { ... }` runs its block only when `value` matches the pattern, with the pattern's names bound inside that block. Use it when only the matching case has work to do; [`val`-else](#val-else-unwrap-or-leave-the-scope) covers the failing side.

```neuro
func show(found: Option<i32>) -> i32 {
    if val Some(t) = found {
        println("found {t}")
    }
    0
}
```

The pattern is one pattern, and it can be anything a `match` arm takes: a literal, a range, a variant with its payload (`Some(t)`, `Shape::Circle { radius }`), an `@` binding, an array pattern, a binding, or `_`. Two arm-level forms are not accepted: an or-pattern (`A | B`) and a guard (`if ...`). Use a `match` for those.

With an `else` it is an expression, like `if`, and its branches must agree in type. Without one it is unit, and its block's value is discarded. Links chain with plain `else if` conditions and `else if val` links in any order:

```neuro
enum Reading {
    Celsius(i32),
    Fahrenheit(i32),
    Missing
}

func to_celsius(r: Reading) -> i32 {
    if val Reading::Celsius(c) = r { c }
    else if val Reading::Fahrenheit(f) = r { (f - 32) * 5 / 9 }
    else { 0 }
}
```

`while val PATTERN = value { ... }` evaluates `value` before every pass and stops at the first one that does not match. It is a `while` loop: `break`, `continue` and labels work, and `break` takes no value. The `prefer_loop_over_while_true` lint does not apply to it.

```neuro
func odd_total(xs: Vec<i32>) -> i32 {
    mut stack: Vec<i32> = xs
    mut total: i32 = 0
    while val Some(n) = stack.pop() {
        if n % 2 == 0 { continue }
        total += n
    }
    total
}
```

Matching moves the value, as `match` does. Binding an owning payload uses up the scrutinee, so a second `if val` on the same `Option<string>` is a use of a moved value. A `Copy` payload such as `Option<i32>` does not move.

The mistakes below are the ones a user is likely to hit:

| Written | Compiler reports |
| --- | --- |
| `val z = if val Some(x) = o { x }` (no `else`) | `cannot bind 'z': the initializer has type void, which is not a value` |
| `if val Some(x) = o { x } else { "no" }` as an `i32` | `match arms have incompatible types: expected i32, found string` |
| `if val Light::Red = Some(1) { ... }` | `this pattern matches Light::Red but the value being matched has type Option<i32>` |
| `if val Some(1) \| Some(2) = o { ... }` | ``unexpected token Pipe, expected '=' after an `if val` pattern`` |
| `if val Some(x) o { ... }` | ``unexpected token Identifier("o"), expected '=' after an `if val` pattern`` |
| `if val Some(x) = o if x > 1 { ... }` | ``unexpected token If, expected '{'`` |
| `x` used after `if val Some(x) = o { ... }` | ``undefined variable 'x'`` |
| `while val Some(x) = v.pop() { break x }` | ``'break' with a value is only allowed in a 'loop'`` |

Runnable programs: [`examples/control_flow/if_val.nr`](../../examples/control_flow/if_val.nr) and [`examples/showcase/job_queue.nr`](../../examples/showcase/job_queue.nr), which drains a queue with `while val`.

## Pool Blocks

A `pool` block is a scope whose allocations come from one contiguous bump arena instead of the heap. Leaving the block moves the bump pointer back, releasing everything the body allocated in a single step, whatever it allocated and however much of it there was.

```neuro
pool {
    val label = "batch " + "one"
    val scratch = Tensor::<f32, [16, 16]>::zeros()
    println(label)
}
// the arena is back to its mark here
```

A pool may carry a label, which names the arena in a diagnostic and nothing else: a `pool` is not a value, and nothing can refer to its label or break out of it. Labels matter once pools nest, the loop shape they exist for:

```neuro
pool training {
    for epoch in 0..100 {
        pool batch {
            // every intermediate here is gone at the closing brace, so
            // the loop's memory use does not grow with the epoch count
        }
    }
}
```

The arena captures the allocations the block itself writes. One a called function makes belongs to whoever that function hands it to, which the compiler cannot prove here, so it stays an ordinary heap allocation: the arena's speed is lost on that path and never its safety.

Three rules keep arena memory from outliving the block, all checked at compile time:

- **Nothing the block already allocated may be written into something that outlives it.** Storing a `string`, a collection, a tensor or a struct that the block took from the arena into a binding declared before the `pool` would leave it addressing bytes the block's exit reclaims. Scalars cross freely: an `i32` carries no address. A value the statement itself builds crosses too, because the compiler routes it off the arena (see below).
- **Nothing the block allocated may be handed to a callee that can store it past the block.** The rule above watches stores written inside the block; this one watches the store a callee makes on your behalf. A `&mut self` receiver and a `&mut` parameter are the two channels through which a callee reaches memory the caller holds, so passing an arena value through either (to a place declared before the `pool`) is refused.
- **A `return` or `break` that leaves the block carries a number, `bool` or `char`, or nothing.** Leaving the block by `return`, `break` or `continue` runs its release on the way out, so anything that could point into the arena may not travel with the jump. A `break` targeting a loop opened *inside* the block stays in it, and carries what it likes. A `?` leaves the same way, so it may propagate an `Option`'s `None` or a `Result` whose error is a number, `bool` or `char`, and no other error.

```neuro
mut total: i32 = 0
mut name: string = ""
pool {
    val joined = "a" + "b"   // the block's own: allocated in the arena
    total = total + 1        // fine: a scalar carries no arena address
    name = "c" + "d"         // fine: routed off the arena, because `name` outlives it
    name = joined            // error: `joined` was already taken from the arena
}
```

### Routing

The compiler decides where an allocation comes from by asking who ends up owning it. Where the owner is a place that outlives the block (a binding declared before it, a field or element reached through one, or the referent of a reference) the allocation is routed to the ordinary heap and the arena never sees it. That is why `name = "c" + "d"` above compiles: the whole statement is emitted with the arena switched off, so the text it builds survives the release and reads correctly afterwards.

Routing applies to the value the statement builds, never to one built earlier. By the time `name = joined` runs, `joined`'s buffer is already in the bump region and no choice of allocator can move it, so that store is refused.

The compiler has to *prove* a routed value holds nothing the block built earlier, and it proves it through a struct or tuple literal, an array literal, and an `if` / `match` whose every arm is a single expression (a `match` also needs its scrutinee proven). So `entry = Entry { label: "a" + "b", n: 1 }` and `name = if c { "x" } else { "y" }` cross the boundary. An arm that declares a binding of its own is not looked into, and is refused.

An allocation the block keeps for itself is unaffected: `val joined = "a" + "b"` is owned by a binding that dies at the closing brace, so it takes the bump path, and a reassignment of such a binding does too. So does a store or a call through a reference the block takes to such a binding (`val r = &mut local`): it writes `local`, which dies with the block. Any other reference the block holds (a `mut` one, one a call returned, one to a binding declared before the `pool`) is assumed to reach past it.

### Values a Callee Builds

A function's body is emitted outside every arena, so what it returns is heap memory the block's release never touches. A call to a function you declared therefore crosses the boundary, as long as every argument and the receiver cross it too: otherwise the callee could be handing back the very pointer the block gave it.

```neuro
func render(n: i32) -> string {
    "row {n}"
}

func main() -> i32 {
    mut kept: string = ""
    pool {
        val step = 7
        kept = render(step)          // fine: `render`'s allocation is heap memory
        kept = render(step).clone()  // fine too: the store is routed off the arena
    }
    println(kept)                    // still valid after the release
    0
}
```

The same test decides the second rule, because the question is the same one: whether the value being handed over is arena memory. What differs is only where the store lands.

```neuro
func render(n: i32) -> string {
    "row {n}"
}

struct Box { s: string }

impl Box {
    func stash(&mut self, v: string) { self.s = v }
}

func main() -> i32 {
    mut b = Box { s: "" }
    pool scratch {
        b.stash(render(7))       // fine: `render`'s allocation is heap memory
        // b.stash("a" + "b")    // refused: `b` outlives the arena that built the text
    }
    0
}
```

A callee's write access is read from its signature, not from its body: a method taking `&mut self`, or a function taking `&mut T`, is assumed to keep what it is given. A `&self` receiver and a shared `&T` parameter have no way to write back, so they take arena values freely.

The two rules part company over a builtin method such as `.clone()`. It is not a function the backend emits somewhere else; it is instructions placed where you wrote them. In a routed store that is harmless, because the arena is switched off across the whole statement: `kept = render(step).clone()` compiles for the same reason `name = "c" + "d"` does. Passed to a callee it is not harmless, because the argument is emitted where it stands, in the arena, which is why `b.stash("a" + "b")` is refused.

A call through a trait object (`&dyn Renderer`) is refused under both rules, and for a different reason: the implementation behind the vtable is not known until the program runs, so neither is what it allocates. Where the compiler cannot prove the owner, the value stays in the block. That costs the arena's speed on such a path and never its safety.

### Destructors and `PoolAware`

A third rule is about what the block *owns* rather than about what escapes it. The arena is released in one store, so it cannot run a destructor per object without giving up the thing it exists for. A value of a type that implements `Drop` is therefore refused wherever the block would own it, whether it is written in the block or handed back by a function the block calls:

```neuro
struct Handle {
    id: i32
}

impl Drop for Handle {
    func drop(&mut self) {
        println("released {self.id}")
    }
}

pool scratch {
    val h = Handle { id: 1 }   // error: 'Handle' implements 'Drop' but not 'PoolAware'
}
```

`PoolAware` is how a type says its release can be batched instead. Implementing it registers the external resource the value holds with the arena, and the arena releases every registered resource in one sweep at the block's exit:

```neuro
impl PoolAware for Handle {
    func register_with_pool(&self, arena: &PoolHandle) {
    }
    func bulk_release(&mut self) {
        println("swept {self.id}")
    }
}
```

With that impl in place the same `pool` block compiles. Inside the block the two methods are what run, and the `Drop` above is not: `register_with_pool` is called where the value is constructed, and `bulk_release` at the closing brace. Outside a `pool` the opposite holds, and the destructor runs as it always did.

The sweep walks the registrations in **reverse order of construction**, the same order stack values are destroyed in. That is what makes a resource safe to depend on another: a view is released before the tensor it borrows from, and an operation before the stream it was queued on, because each registered after the thing it depends on.

```neuro
pool scratch {
    val first = Handle { id: 1 }
    val second = Handle { id: 2 }
}
// prints: swept 2, then swept 1
```

A type with no destructor at all needs neither method: its memory goes back with the bulk arena free and nothing else is owed. Building the value *before* the `pool` is the other way out: it never comes from the arena, so its ordinary destructor still applies.

| Type | In a `pool`? |
| --- | --- |
| No `Drop` | yes, reclaimed by the bulk arena free |
| `PoolAware` (with or without `Drop`) | yes, `bulk_release` in reverse construction order at the block's exit |
| `Drop` only | no, a compile error naming the pool |

See [`examples/ownership/pool_arena.nr`](../../examples/ownership/pool_arena.nr) for a runnable program, and [`examples/showcase/batch_arena.nr`](../../examples/showcase/batch_arena.nr) for a batched forward pass built inside one.

## See also

- [Expressions](expressions.md): `if`, `match`, block and `unsafe` expressions, and the full pattern grammar
- [Operators](operators.md): comparison and logical operators
- [Variables](variables.md): variable scope in blocks
- [Memory model](memory-model.md): where the `pool` arena sits among the other allocation rules
