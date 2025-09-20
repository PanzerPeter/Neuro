# While Loops

NEURO provides iterative execution through `while` loops. These loops repeatedly execute a block of code as long as a condition remains true.

## Basic While Loop

### Syntax
```neuro
while condition {
    // statements to execute while condition is true
}
```

The condition must be an expression that evaluates to a `bool` type.

### Example
```neuro
fn count_down(start: int) {
    let mut counter = start;
    while counter > 0 {
        counter = counter - 1;
    }
}
```

## Complete While Loop Example

```neuro
fn sum_to(n: int) -> int {
    let mut i = 0;
    let mut s = 0;
    while i < n {
        s = s + i;
        i = i + 1;
    }
    return s;
}
```

## Loop Control Statements

### Break Statement

The `break;` statement immediately exits the loop:

```neuro
fn find_factor(n: int) -> int {
    let mut i = 2;
    while i < n {
        if n % i == 0 {
            break;  // Exit loop when factor found
        }
        i = i + 1;
    }
    return i;
}
```

### Continue Statement

The `continue;` statement skips the rest of the current iteration and jumps to the condition check:

```neuro
fn sum_even_numbers(limit: int) -> int {
    let mut i = 0;
    let mut sum = 0;
    while i < limit {
        i = i + 1;
        if i % 2 != 0 {
            continue;  // Skip odd numbers
        }
        sum = sum + i;
    }
    return sum;
}
```

## Nested While Loops

While loops can be nested for complex iteration patterns:

```neuro
fn find_pair_sum(target: int, max_val: int) -> bool {
    let mut i = 1;
    while i <= max_val {
        let mut j = i + 1;
        while j <= max_val {
            if i + j == target {
                return true;  // Found pair that sums to target
            }
            j = j + 1;
        }
        i = i + 1;
    }
    return false;
}
```

## Infinite Loops

Loops with constant `true` conditions create infinite loops:

```neuro
fn infinite_loop() {
    while true {
        // This loop runs forever unless broken
        if some_condition() {
            break;  // Only way to exit
        }
        // Process continues...
    }
}
```

## Complex Conditions

Loop conditions can be complex boolean expressions:

```neuro
fn process_until_ready(max_attempts: int) -> bool {
    let mut attempts = 0;
    let mut ready = false;
    let mut error = false;

    while !ready && !error && attempts < max_attempts {
        attempts = attempts + 1;
        // ... processing logic ...
        if attempts > 10 {
            error = true;
        }
    }

    return ready && !error;
}
```

## While Loops with Different Variable Patterns

### Single Counter
```neuro
fn count_digits(n: int) -> int {
    let mut count = 0;
    let mut num = n;
    while num > 0 {
        count = count + 1;
        num = num / 10;
    }
    return count;
}
```

### Multiple Variables
```neuro
fn fibonacci_nth(n: int) -> int {
    let mut a = 0;
    let mut b = 1;
    let mut i = 0;

    while i < n {
        let temp = a + b;
        a = b;
        b = temp;
        i = i + 1;
    }
    return a;
}
```

## While Loops in Different Contexts

### In Functions
```neuro
fn factorial(n: int) -> int {
    let mut result = 1;
    let mut i = 1;
    while i <= n {
        result = result * i;
        i = i + 1;
    }
    return result;
}
```

### With Conditional Logic
```neuro
fn find_first_even(start: int, limit: int) -> int {
    let mut current = start;
    while current < limit {
        if current % 2 == 0 {
            return current;
        }
        current = current + 1;
    }
    return -1;  // Not found
}
```

## Common Patterns

### Accumulator Pattern
```neuro
fn sum_range(start: int, end: int) -> int {
    let mut sum = 0;
    let mut i = start;
    while i <= end {
        sum = sum + i;
        i = i + 1;
    }
    return sum;
}
```

### Search Pattern
```neuro
fn linear_search(value: int, max_index: int) -> bool {
    let mut index = 0;
    while index < max_index {
        if get_value_at(index) == value {
            return true;
        }
        index = index + 1;
    }
    return false;
}
```

## Complete Example

```neuro
fn main() -> int {
    let mut total = 0;
    let mut i = 1;

    // Sum numbers 1 through 10, but skip multiples of 3
    while i <= 10 {
        if i % 3 == 0 {
            i = i + 1;
            continue;  // Skip multiples of 3
        }

        total = total + i;

        if total > 20 {
            break;     // Stop if total exceeds 20
        }

        i = i + 1;
    }

    return total;
}
```

## Current Implementation Status

### Fully Implemented ✅
- Basic `while` loops with boolean conditions
- `break` statements for early loop exit
- `continue` statements for skipping iterations
- Nested while loops
- Complex boolean expressions in conditions
- Loop-scoped variables and assignments

### Current Limitations
- For loops (`for item in collection`) not yet implemented
- Loop labels for breaking/continuing outer loops not available
- Iterator-based loops not implemented
- `do-while` loops not supported

## Performance Notes

- While loops compile to efficient native code via LLVM
- Simple counter loops are optimized by the compiler
- Complex conditions are evaluated each iteration

