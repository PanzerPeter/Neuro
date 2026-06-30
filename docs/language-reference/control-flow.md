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

The motivation is style, not safety — both forms produce identical machine
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

Only `loop` can yield a value — it is the one loop guaranteed (by the absence of
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

A `for`, `while`, or `loop` may be prefixed with a label — an identifier
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

## Future Features (Phase 1)

### Pattern Matching (Phase 1)

```neuro
// Not yet implemented
match value {
    0 => "zero",
    1 => "one",
    _ => "other",
}
```

## References

- [Expressions](expressions.md) - Boolean expressions
- [Operators](operators.md) - Comparison and logical operators
- [Variables](variables.md) - Variable scope in blocks
