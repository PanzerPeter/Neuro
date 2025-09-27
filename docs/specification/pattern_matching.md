# Pattern Matching

NEURO provides pattern matching through the `match` expression, which allows for efficient and readable conditional logic based on value structure and content.

## Match Expression Syntax

### Basic Syntax
```neuro
match expression {
    pattern1 => result1,
    pattern2 => result2,
    _ => default_result
}
```

The `match` expression:
- Evaluates the input expression once
- Compares it against each pattern in order
- Returns the result of the first matching pattern
- Requires all match arms to have the same type

## Pattern Types

### Literal Patterns
Match against specific literal values:

```neuro
fn classify_number(x: int) -> string {
    return match x {
        0 => "zero",
        1 => "one",
        2 => "two",
        _ => "other"
    };
}
```

### Boolean Patterns
```neuro
fn bool_to_string(flag: bool) -> string {
    return match flag {
        true => "yes",
        false => "no"
    };
}
```

### String Patterns
```neuro
fn greet(name: string) -> string {
    return match name {
        "Alice" => "Hello Alice!",
        "Bob" => "Hi Bob!",
        _ => "Hello stranger!"
    };
}
```

### Wildcard Pattern
The underscore `_` pattern matches any value and is typically used as the final catch-all case:

```neuro
fn categorize(value: int) -> string {
    return match value {
        1 => "small",
        10 => "medium",
        100 => "large",
        _ => "unknown"
    };
}
```

## Complete Examples

### Simple Value Matching
```neuro
fn day_type(day: int) -> string {
    return match day {
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        6 => "Saturday",
        7 => "Sunday",
        _ => "Invalid day"
    };
}

fn main() -> int {
    let day = 3;
    let name = day_type(day);
    // name is "Wednesday"
    return 0;
}
```

### Nested Match Expressions
```neuro
fn complex_logic(x: int, y: int) -> int {
    return match x {
        0 => match y {
            0 => 1,
            _ => 2
        },
        1 => match y {
            0 => 3,
            1 => 4,
            _ => 5
        },
        _ => 0
    };
}
```

### Match in Function Calls
```neuro
fn is_special(x: int) -> bool {
    return match x {
        7 => true,
        13 => true,
        42 => true,
        _ => false
    };
}

fn main() -> int {
    let result = is_special(42);  // true
    return match result {
        true => 1,
        false => 0
    };
}
```

## Expression Context

Match expressions can be used anywhere expressions are valid:

### In Return Statements
```neuro
fn process(value: int) -> int {
    return match value {
        0 => 100,
        1 => 200,
        _ => 300
    };
}
```

### In Variable Assignments
```neuro
fn main() -> int {
    let x = 5;
    let result = match x {
        5 => 42,
        _ => 0
    };
    return result;
}
```

### In Function Arguments
```neuro
fn compute(a: int, b: int) -> int {
    return a + b;
}

fn main() -> int {
    let x = 3;
    return compute(
        match x { 3 => 10, _ => 0 },
        match x { 3 => 20, _ => 5 }
    );
}
```

## Type Requirements

All match arms must have compatible types:

```neuro
// ✅ Valid - all arms return int
fn valid_match(x: int) -> int {
    return match x {
        1 => 100,
        2 => 200,
        _ => 0
    };
}

// ❌ Invalid - mixed return types (not yet enforced)
// fn invalid_match(x: int) -> ??? {
//     return match x {
//         1 => 100,      // int
//         2 => "hello",  // string
//         _ => true      // bool
//     };
// }
```

## Current Implementation Status

### Fully Implemented ✅
- Basic match expression parsing
- Literal patterns (integers, floats, strings, booleans)
- Wildcard patterns (`_`)
- Identifier patterns (variable binding)
- Expression result evaluation
- Type checking for consistent arm types
- Nested match expressions
- Match in all expression contexts

### Limitations ⚠️
- No destructuring patterns (tuples, structs)
- No range patterns (`1..10`)
- No guard expressions (`pattern if condition`)
- No exhaustiveness checking
- Limited pattern complexity

### Not Yet Implemented ❌
- Struct destructuring: `Point { x, y } => ...`
- Tuple patterns: `(a, b) => ...`
- Array patterns: `[first, rest...] => ...`
- Range patterns: `1..=10 => ...`
- Guard clauses: `x if x > 0 => ...`
- Or patterns: `1 | 2 | 3 => ...`
- Pattern aliases: `x @ Point { .. } => ...`

## Error Handling

The compiler provides clear error messages for:
- Missing wildcard patterns (when needed)
- Type mismatches between match arms
- Invalid pattern syntax
- Unreachable patterns

## Future Enhancements

Planned pattern matching features include:

### Struct Destructuring
```neuro
// Planned syntax (not yet implemented)
struct Point { x: int, y: int }

fn classify_point(p: Point) -> string {
    return match p {
        Point { x: 0, y: 0 } => "origin",
        Point { x: 0, y: _ } => "y-axis",
        Point { x: _, y: 0 } => "x-axis",
        Point { x, y } => "general"
    };
}
```

### Guard Expressions
```neuro
// Planned syntax (not yet implemented)
fn categorize_number(x: int) -> string {
    return match x {
        n if n < 0 => "negative",
        n if n == 0 => "zero",
        n if n > 100 => "large",
        _ => "positive"
    };
}
```

### Range Patterns
```neuro
// Planned syntax (not yet implemented)
fn grade_letter(score: int) -> string {
    return match score {
        90..=100 => "A",
        80..=89 => "B",
        70..=79 => "C",
        60..=69 => "D",
        _ => "F"
    };
}
```