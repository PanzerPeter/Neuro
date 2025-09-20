# Literals

NEURO supports several types of literal values that can be used directly in expressions and variable assignments.

## Integer Literals

32-bit signed integers with decimal notation:

### Syntax and Examples
- Positive integers: `0`, `1`, `42`, `1000`
- Negative integers: Use unary minus: `-15`, `-100`

### Range
- -2^31 to 2^31-1 (-2,147,483,648 to 2,147,483,647)

## Float Literals

64-bit IEEE 754 double-precision floating-point numbers:

### Syntax and Examples
- Decimal notation: `0.0`, `3.14`, `123.456`
- Must contain a decimal point to distinguish from integers
- Negative floats: Use unary minus: `-2.5`, `-0.1`

## String Literals

UTF-8 encoded string values enclosed in double quotes:

### Syntax and Examples
- Basic strings: `"hello"`, `"world"`, `"hi"`
- Empty string: `""`

### Escape Sequences
Supported escape sequences within string literals:
- `\n` - Newline character
- `\t` - Tab character
- `\r` - Carriage return
- `\\` - Backslash character
- `\"` - Double quote character

### Example with Escapes
```neuro
let message: string = "Hello\nWorld";           // Multi-line
let quoted: string = "She said, \"Hello!\"";   // Embedded quotes
let path: string = "C:\\Users\\file.txt";      // Windows path
```

## Boolean Literals

Boolean values for logical operations:

### Syntax
- True value: `true`
- False value: `false`

## Complete Example

```neuro
fn main() -> int {
    let a: int = 42;         // Integer literal
    let b: float = 3.14;     // Float literal
    let s: string = "hi";    // String literal
    let t: bool = true;      // Boolean literal

    // Literals in expressions
    let sum = a + 10;                    // 52
    let product = b * 2.0;               // 6.28
    let flag = t && false;               // false

    return a;
}
```

## Type Inference

NEURO's type inference can often determine types from literal usage:

```neuro
let count = 42;        // inferred as int
let ratio = 3.14;      // inferred as float
let message = "hi";    // inferred as string
let active = true;     // inferred as bool
```

## Current Limitations

- Scientific notation for floats (e.g., `1.23e-4`) not yet implemented
- Hexadecimal, octal, or binary integer literals not supported
- Raw strings and string interpolation not available
- Unicode escape sequences not yet implemented

