# Semantic Analysis

**Status**: ✅ Complete (Phase 1)
**Crate**: `compiler/semantic-analysis`
**Entry Point**: `pub fn type_check(items: &[Item]) -> Result<(), Vec<TypeError>>`

## Overview

The semantic analysis feature slice performs type checking and semantic validation on the AST produced by the parser. It ensures type safety, validates variable and function scoping, and provides comprehensive error reporting with source location information.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Dependencies**: `syntax-parsing` (AST types), `shared-types` (common types)
- **Public API**: Single entry point (`type_check`)
- **Public types**: `Type` enum (semantic type representation), `TypeError` enum
- **Internal implementation**: Type checker logic is `pub(crate)`

## Features

### Type System (Phase 1)

#### Primitive Types

```rust
pub enum Type {
    I32,        // 32-bit signed integer
    I64,        // 64-bit signed integer
    F32,        // 32-bit floating point
    F64,        // 64-bit floating point
    Bool,       // Boolean
    Void,       // No return value
    Function {  // Function type
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Unknown,    // Error recovery only
}
```

#### Type Compatibility

```rust
impl Type {
    pub fn is_compatible_with(&self, other: &Type) -> bool {
        // Exact type matching (no implicit conversions in Phase 1)
        // Function types must match parameters and return type
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I32 | Type::I64 | Type::F32 | Type::F64)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Type::Bool)
    }
}
```

### Semantic Checks

#### 1. Type Checking

**Expression type checking**:
- Literals have their default types (i32, f64, bool)
- Identifiers lookup their declared type
- Binary operators check operand types and return result type
- Unary operators validate operand types
- Function calls check argument types and count

**Statement type checking**:
- Variable declarations validate initializer type matches declared type
- Return statements check return value matches function signature
- If/else conditions must be boolean
- Expression statements validate expression types

#### 2. Scope Validation

**Lexical scoping** with shadowing support:
- Global scope for functions
- Function-level scope for parameters and local variables
- Block scope for if/else statements
- Inner scopes can shadow outer scopes

**Example**:
```neuro
func test() -> i32 {
    val x: i32 = 1       // Outer scope
    if true {
        val x: i32 = 2   // Shadows outer x (allowed)
        return x         // Returns 2
    }
    return x             // Returns 1
}
```

#### 3. Function Signature Validation

- Parameter types must be resolved
- Return type must match all return statements
- Function names must be unique (no overloading in Phase 1)
- Forward references are supported

#### 4. Variable Declaration Validation

- Variables must have a type (explicit or inferred)
- Variables cannot be used before declaration
- Variable names must be unique in scope
- Uninitialized variables require explicit type annotation

## Usage

### Basic Type Checking

```rust
use syntax_parsing::parse;
use semantic_analysis::type_check;

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        return a + b
    }
"#;

let ast = parse(source)?;
match type_check(&ast) {
    Ok(()) => println!("✓ Program is type-correct"),
    Err(errors) => {
        for error in errors {
            eprintln!("Type error: {}", error);
        }
    }
}
```

### Error Collection

The type checker uses a **fail-slow** approach, collecting all errors:

```rust
let source = r#"
    func bad() -> i32 {
        val x: i32 = true    // Error 1: type mismatch
        return undefined_var  // Error 2: undefined variable
    }
"#;

let ast = parse(source).unwrap();
let errors = type_check(&ast).unwrap_err();

assert_eq!(errors.len(), 2);
// Both errors are reported
```

## Error Types

### Comprehensive Error Reporting

```rust
pub enum TypeError {
    Mismatch {
        expected: Type,
        found: Type,
        span: Span,
    },
    UndefinedVariable {
        name: String,
        span: Span,
    },
    UndefinedFunction {
        name: String,
        span: Span,
    },
    VariableAlreadyDefined {
        name: String,
        span: Span,
    },
    FunctionAlreadyDefined {
        name: String,
        span: Span,
    },
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },
    InvalidOperator {
        op: String,
        ty: Type,
        span: Span,
    },
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },
    MissingReturn {
        expected: Type,
    },
    UnknownTypeName {
        name: String,
        span: Span,
    },
    NotCallable {
        ty: Type,
        span: Span,
    },
    UninitializedVariable {
        name: String,
        span: Span,
    },
}
```

All errors include span information for precise error reporting.

## Implementation Details

### Type Checker State

```rust
struct TypeChecker {
    /// Symbol table for variables (with lexical scoping)
    symbols: SymbolTable,

    /// Function signatures (global scope)
    functions: HashMap<String, Type>,

    /// Collected errors (fail-slow approach)
    errors: Vec<TypeError>,

    /// Current function's return type (for return validation)
    current_function_return_type: Option<Type>,
}
```

### Symbol Table

```rust
struct SymbolTable {
    /// Stack of scopes (innermost scope is last)
    scopes: Vec<HashMap<String, Type>>,
}

impl SymbolTable {
    fn push_scope(&mut self);     // Enter new scope
    fn pop_scope(&mut self);       // Exit scope
    fn define(&mut self, name: String, ty: Type);
    fn lookup(&self, name: &str) -> Option<&Type>;
}
```

### Type Inference (Simple)

Phase 1 supports basic type inference:

```neuro
val x = 42        // Inferred as i32
val y = 3.14      // Inferred as f64
val z = true      // Inferred as bool

val a: i32 = 42   // Explicit type (checked)
```

No inference for:
- Function parameters (must be explicit)
- Function return types (must be explicit)
- Uninitialized variables (must have explicit type)

## Testing

**Test coverage**: 24 comprehensive tests

Test categories:
- **Positive tests**: Valid programs that should type check
- **Negative tests**: Invalid programs with specific error types
- **Scoping tests**: Variable shadowing, nested scopes
- **Function tests**: Calls, signatures, parameter type checking
- **Operator tests**: Binary/unary operators with various types
- **Control flow tests**: If/else with different types

Example test:
```rust
#[test]
fn error_type_mismatch() {
    let source = r#"func test() -> i32 {
        val x: i32 = true
        return x
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = type_check(&items);

    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| matches!(e, TypeError::Mismatch { .. })));
}
```

## Design Decisions

### Fail-Slow Error Collection

**Why collect all errors?**
- Better developer experience
- See all issues at once
- No need to fix-compile-repeat cycle

**Implementation**:
- Type checker continues after errors
- Uses `Unknown` type for error recovery
- Returns all collected errors at end

### Strict Type System (Phase 1)

**No implicit conversions**:
```neuro
val x: i64 = 42  // Error: expected i64, found i32
```

Why:
- Simplicity for Phase 1
- Predictable behavior
- Easy to add explicit conversions later

### Lexical Scoping with Shadowing

**Why allow shadowing?**
- Functional programming pattern
- Useful for local scope redefinition
- Prevents accidental name clashes

Example:
```neuro
val x = 1
if condition {
    val x = 2  // New variable, shadows outer x
}
```

## Type Checking Algorithm

### Two-Pass Algorithm

**Pass 1: Collect function signatures**
```rust
for item in items {
    if let Item::Function(func) = item {
        let func_type = extract_signature(func);
        functions.insert(func.name, func_type);
    }
}
```

**Pass 2: Type check function bodies**
```rust
for item in items {
    if let Item::Function(func) = item {
        check_function_body(func);
    }
}
```

This allows:
- Forward function references
- Recursive functions
- Functions calling functions defined later

### Expression Type Checking

```rust
fn check_expr(&mut self, expr: &Expr) -> Result<Type, ()> {
    match expr {
        Expr::Literal(lit, _) => infer_literal_type(lit),
        Expr::Identifier(ident) => self.symbols.lookup(&ident.name),
        Expr::Binary { left, op, right, .. } => {
            let left_ty = self.check_expr(left)?;
            let right_ty = self.check_expr(right)?;
            validate_binary_op(op, left_ty, right_ty)
        }
        // ... other cases
    }
}
```

## API Reference

### Public Functions

```rust
/// Type check a NEURO program
pub fn type_check(items: &[Item]) -> Result<(), Vec<TypeError>>
```

### Public Types

```rust
pub enum Type { ... }
pub enum TypeError { ... }
```

## Integration Points

### Upstream Dependencies

- **syntax-parsing**: AST types (Item, Expr, Stmt, etc.)
- **shared-types**: Common types (Span, Identifier, Literal)

### Downstream Consumers

- **llvm-backend**: Uses `Type` for code generation
- **neurc**: Reports type errors to user
- **LSP server** (Phase 7): Type information for IDE features

## Examples

### Type Checking Success

```neuro
func factorial(n: i32) -> i32 {
    if n <= 1 {
        return 1
    } else {
        return n * factorial(n - 1)
    }
}
```

**Type check**: ✅ Pass
- `n` has type `i32`
- `n <= 1` returns `bool` (valid condition)
- `factorial(n - 1)` recursive call type checks
- All return statements return `i32`

### Type Checking Errors

```neuro
func bad_example() -> i32 {
    val x = "string"       // x has type string (inferred)
    return x + 1           // Error: can't add string and i32
}
```

**Errors**:
1. `TypeError::UnsupportedType`: String literals not in Phase 1
2. `TypeError::InvalidBinaryOperator`: Can't apply `+` to these types

## Future Enhancements (Post-Phase 1)

### Phase 2: Enhanced Type System
- [ ] **Type inference**: Infer function return types
- [ ] **Generic functions**: Monomorphization
- [ ] **Structs**: User-defined types
- [ ] **Arrays**: Fixed-size arrays `[i32; 10]`
- [ ] **Explicit conversions**: `as i64`, `as f32`

### Phase 3: Tensor Types
- [ ] **Static tensor types**: `Tensor<f32, [3, 3]>`
- [ ] **Shape checking**: Compile-time tensor dimension validation
- [ ] **Broadcasting**: NumPy-style broadcasting rules

### Phase 4+: Advanced Features
- [ ] **Traits**: Type classes for polymorphism
- [ ] **Associated types**: Types associated with traits
- [ ] **Lifetime analysis**: Borrow checker (if needed)

## Troubleshooting

### "Type mismatch" errors

**Problem**: Types don't match expectations

**Solution**:
- Check variable and function type annotations
- Ensure return type matches all return statements
- Verify operator operand types are compatible

### "Undefined variable" errors

**Problem**: Variable not in scope

**Solution**:
- Check variable is declared before use
- Verify variable name spelling
- Check scope (variable may be in different block)

### "Argument count mismatch" errors

**Problem**: Wrong number of function arguments

**Solution**:
- Check function signature
- Ensure all required arguments are provided
- Verify no extra arguments

## Performance

- **Type checking speed**: <1ms for small programs
- **Memory**: O(n) for symbol table
- **Single-pass**: Type checks in one traversal (after signature collection)

## References

- [Type Systems](https://www.cs.cornell.edu/courses/cs4110/2018fa/lectures/)
- [Bidirectional Type Checking](https://arxiv.org/abs/1908.05839)
- Source: [compiler/semantic-analysis/src/lib.rs](../../compiler/semantic-analysis/src/lib.rs)
