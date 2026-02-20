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

```neuro
func abs(x: i32) -> i32 {
    if x >= 0 {
        x  // Implicit return
    } else {
        -x  // Implicit return
    }
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

## For Loops (Exclusive Range)

Use `for` to iterate over an exclusive integer range:

```neuro
for i in 0..10 {
    // i takes values 0 through 9
}
```

### For-Range Requirements

- Range bounds must be integer-compatible expressions.
- The upper bound is exclusive (`0..10` does not include `10`).
- `..=` inclusive ranges are not yet supported.

```neuro
func sum_first_five() -> i32 {
    mut sum: i32 = 0
    for i in 0..5 {
        sum = sum + i
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

## Future Features (Phase 2+)

### For Loops (Phase 2)

```neuro
// Not yet implemented
for i in 0..10 {
    // Iterate from 0 to 9
}
```

### Pattern Matching (Phase 2)

```neuro
// Not yet implemented
match value {
    0 => "zero",
    1 => "one",
    _ => "other",
}
```

### If as Expression (Phase 2)

```neuro
// Not yet implemented
val x: i32 = if condition { 1 } else { 0 }
```

## References

- [Expressions](expressions.md) - Boolean expressions
- [Operators](operators.md) - Comparison and logical operators
- [Variables](variables.md) - Variable scope in blocks
