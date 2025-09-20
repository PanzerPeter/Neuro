# Semicolons

NEURO uses semicolons (`;`) to terminate statements, providing clear separation between different program instructions. Statement termination is required in most contexts.

## Basic Semicolon Rules

### Required Semicolons
Semicolons are **required** to terminate the following statements:
- Variable declarations: `let x = 42;`
- Assignments: `x = value;`
- Expression statements: `function_call();`
- Return statements: `return value;`
- Break statements: `break;`
- Continue statements: `continue;`

### Example of Required Semicolons
```neuro
fn main() -> int {
    let x = 1 + 2;      // Variable declaration - semicolon required
    x = x + 1;          // Assignment - semicolon required
    return x;           // Return statement - semicolon required
}
```

## Statement Types and Semicolon Usage

### Variable Declarations
All variable declarations must end with semicolons:
```neuro
let count: int = 10;
let mut sum = 0;
let name: string = "NEURO";
let ready: bool = true;
```

### Assignment Statements
All assignments require semicolons:
```neuro
fn example() -> int {
    let mut x = 5;
    x = 10;           // Assignment semicolon required
    x = x * 2;        // Assignment semicolon required
    return x;
}
```

### Expression Statements
Function calls and other expressions used as statements need semicolons:
```neuro
fn side_effect() {
    return;
}

fn main() -> int {
    side_effect();    // Function call as statement - semicolon required
    return 0;
}
```

### Control Flow Statements
- `return` statements: `return value;`
- `break` statements: `break;`
- `continue` statements: `continue;`

```neuro
fn loop_example() -> int {
    let mut i = 0;
    while i < 10 {
        if i == 5 {
            break;      // Break statement - semicolon required
        }
        if i % 2 == 0 {
            i = i + 1;
            continue;   // Continue statement - semicolon required
        }
        i = i + 1;
    }
    return i;           // Return statement - semicolon required
}
```

## Block Statements and Semicolons

### Control Flow Blocks
Block statements (if, while, function bodies) do **not** require semicolons after their closing braces:

```neuro
fn example() -> int {
    let x = 5;

    if x > 0 {
        return x;
    }              // No semicolon after if block

    while x < 10 {
        x = x + 1;
    }              // No semicolon after while block

    return x;
}                  // No semicolon after function body
```

### Nested Blocks
Standalone block statements also don't require trailing semicolons:

```neuro
fn scoped_example() -> int {
    let x = 1;

    {
        let y = 2;
        x = x + y;
    }              // No semicolon after block

    return x;
}
```

## Special Cases

### Return Statement Variants
Both forms of return statements require semicolons:
```neuro
fn void_function() {
    return;        // Void return - semicolon required
}

fn value_function() -> int {
    return 42;     // Value return - semicolon required
}
```

### Multiple Statements on One Line
While not recommended for readability, multiple statements can appear on one line:
```neuro
fn compact() -> int {
    let x = 1; let y = 2; return x + y;
}
```

## Eval Mode Context

The documentation mentions that the parser accepts optional semicolons after top-level expressions in eval mode. This refers to the compiler's expression evaluation feature:

```bash
# In eval mode, semicolon might be optional
neurc eval "2 + 3"     # No semicolon needed in eval
```

However, in regular source files, semicolon rules apply strictly.

## Complete Example

```neuro
fn factorial(n: int) -> int {
    let mut result = 1;    // Variable declaration - semicolon required
    let mut i = 1;         // Variable declaration - semicolon required

    while i <= n {         // While statement - no semicolon after block
        result = result * i;   // Assignment - semicolon required
        i = i + 1;             // Assignment - semicolon required
    }

    return result;         // Return statement - semicolon required
}

fn main() -> int {
    let value = factorial(5);  // Variable declaration - semicolon required
    return value;              // Return statement - semicolon required
}
```

## Error Examples

### Missing Semicolons (Compilation Errors)
```neuro
fn errors() -> int {
    let x = 42        // ERROR: Missing semicolon
    x = x + 1         // ERROR: Missing semicolon
    return x          // ERROR: Missing semicolon
}
```

### Incorrect Semicolon Usage
```neuro
fn incorrect() -> int {
    if x > 0 {
        return 1;
    };                // ERROR: Unnecessary semicolon after if block

    while x < 10 {
        x = x + 1;
    };                // ERROR: Unnecessary semicolon after while block

    return 0;
}
```

## Style Recommendations

### Consistent Formatting
```neuro
// Good: Clear statement separation
fn well_formatted() -> int {
    let x = 10;
    let y = 20;
    let sum = x + y;
    return sum;
}

// Avoid: Multiple statements per line
fn poorly_formatted() -> int {
    let x = 10; let y = 20; return x + y;
}
```

## Current Implementation Status

### Fully Implemented ✅
- Semicolon parsing for all statement types
- Required semicolons for variable declarations
- Required semicolons for assignments
- Required semicolons for expression statements
- Required semicolons for return/break/continue
- Proper error reporting for missing semicolons

### Special Behavior
- Optional semicolons in eval mode for top-level expressions
- No semicolons required after block statements

### Parsing Behavior
- The parser strictly enforces semicolon rules in source files
- Clear error messages when semicolons are missing
- Proper handling of semicolons in all statement contexts

