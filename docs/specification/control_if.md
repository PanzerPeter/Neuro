# If/Else Statements

NEURO provides conditional execution through `if` and optional `else` statements. All conditional branches must use block syntax with braces.

## Basic If Statement

### Syntax
```neuro
if condition {
    // statements to execute if condition is true
}
```

The condition must be an expression that evaluates to a `bool` type.

### Example
```neuro
fn check_positive(x: int) {
    if x > 0 {
        // This block executes when x is positive
        return;
    }
    // Execution continues here if condition is false
}
```

## If/Else Statement

### Syntax
```neuro
if condition {
    // statements for true branch
} else {
    // statements for false branch
}
```

### Example
```neuro
fn sign(x: int) -> int {
    if x > 0 {
        return 1;
    } else {
        return 0;
    }
}
```

## If/Else If Chains

Multiple conditions can be chained using `else if`:

```neuro
fn classify_number(x: int) -> int {
    if x > 0 {
        return 1;   // Positive
    } else if x < 0 {
        return -1;  // Negative
    } else {
        return 0;   // Zero
    }
}
```

## Nested If Statements

If statements can be nested to handle complex conditional logic:

```neuro
fn check_range(x: int, min: int, max: int) -> bool {
    if x >= min {
        if x <= max {
            return true;  // Within range
        } else {
            return false; // Above range
        }
    } else {
        return false;     // Below range
    }
}
```

## Complex Conditions

Conditions can be complex boolean expressions using logical operators:

```neuro
fn is_valid_age(age: int) -> bool {
    if age >= 0 && age <= 150 {
        return true;
    } else {
        return false;
    }
}

fn should_process(count: int, ready: bool, enabled: bool) -> bool {
    if (count > 0 || ready) && enabled {
        return true;
    } else {
        return false;
    }
}
```

## If Statements in Different Contexts

### In Function Bodies
```neuro
fn abs(x: int) -> int {
    if x < 0 {
        return -x;
    } else {
        return x;
    }
}
```

### In Loops
```neuro
fn find_first_positive(start: int, limit: int) -> int {
    let mut i = start;
    while i < limit {
        if i > 0 {
            return i;
        }
        i = i + 1;
    }
    return -1;  // Not found
}
```

### In Nested Blocks
```neuro
fn complex_logic(x: int, y: int) -> int {
    {
        let threshold = 10;
        if x + y > threshold {
            if x > y {
                return x;
            } else {
                return y;
            }
        }
    }
    return 0;
}
```

## Statement vs Expression Context

In NEURO, `if` statements are statements, not expressions (they don't return values directly):

```neuro
// This works - if as statement
fn example(flag: bool) -> int {
    if flag {
        return 42;
    } else {
        return 0;
    }
}

// This doesn't work - if as expression (not yet supported)
// let value = if flag { 42 } else { 0 };  // Not implemented
```

## Braces Are Required

Unlike some languages, NEURO requires braces around all if/else blocks, even for single statements:

```neuro
// Correct - braces required
if x > 0 {
    return 1;
}

// Incorrect - this won't parse
// if x > 0
//     return 1;  // Syntax error
```

## Complete Example

```neuro
fn main() -> int {
    let x = 5;
    let y = -3;

    // Simple if/else
    if x > y {
        let result = x + y;
        if result > 0 {
            return result;
        } else {
            return 0;
        }
    } else {
        return -1;
    }
}
```

## Current Implementation Status

### Fully Implemented ✅
- Basic `if` statements with boolean conditions
- `if/else` statements
- `else if` chaining
- Nested if statements
- Complex boolean expressions in conditions
- Block-scoped variables within if blocks

### Current Limitations
- If expressions (conditional expressions) not yet implemented
- Pattern matching in conditions not available
- Guard clauses or match statements not implemented

