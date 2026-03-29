# Semantic Analysis

**Status**: Complete (Phase 1)
**Crate**: `compiler/semantic-analysis`
**Entry Point**: `pub fn type_check(items: &[Item]) -> Result<(), Vec<TypeError>>`

## Overview

The semantic analysis feature slice performs type checking and semantic validation on the AST produced by the parser. It ensures type safety, validates variable and function scoping, and provides comprehensive error reporting with source location information.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Dependencies**: `ast-types` (AST types), `shared-types` (common values)
- **Public API**: Single entry point (`type_check`)
- **Public types**: `Type` enum (semantic type representation), `TypeError` enum
- **Internal implementation**: Type checker logic is `pub(crate)`

## Features

### Type System (Phase 1)

#### Primitive Types

```rust
pub enum Type {
    // Signed integers
    I8, I16, I32, I64,
    // Unsigned integers
    U8, U16, U32, U64,
    // Floating point
    F32, F64,
    Bool,
    String,
    Void,
    /// Named struct type — nominal: two Struct values are equal iff names match
    Struct(String),
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
    Unknown, // Error recovery
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
    Mismatch { expected: Type, found: Type, span: Span },
    UndefinedVariable { name: String, span: Span },
    UndefinedFunction { name: String, span: Span },
    VariableAlreadyDefined { name: String, span: Span },
    FunctionAlreadyDefined { name: String, span: Span },
    ArgumentCountMismatch { expected: usize, found: usize, span: Span },
    InvalidOperator { op: String, ty: Type, span: Span },
    InvalidBinaryOperator { op: String, left: Type, right: Type, span: Span },
    ReturnTypeMismatch { expected: Type, found: Type, span: Span },
    MissingReturn { expected: Type, span: Span },
    UnknownTypeName { name: String, span: Span },
    NotCallable { ty: Type, span: Span },
    UninitializedVariable { name: String, span: Span },
    // Mutability
    AssignToImmutable { name: String, span: Span },
    AssignToImmutableField { var_name: String, field_name: String, span: Span },
    // Integer literals
    IntegerLiteralOutOfRange { value: i64, ty: Type, span: Span },
    // Control flow
    BreakOutsideLoop { span: Span },
    ContinueOutsideLoop { span: Span },
    InvalidForRangeType { found: Type, span: Span },
    // Structs
    StructAlreadyDefined { name: String, span: Span },
    UnknownStruct { name: String, span: Span },
    UnknownField { struct_name: String, field_name: String, span: Span },
    MissingStructField { struct_name: String, field_name: String, span: Span },
    DuplicateStructField { field_name: String, span: Span },
    // Methods
    MethodNotFound { struct_name: String, method_name: String, span: Span },
    UnsupportedSelfParam { type_name: String, self_param: String, span: Span },
    // Path expressions
    UnknownPathType { type_name: String, member: String, span: Span },
    UnknownAssociatedFunction { type_name: String, member: String, span: Span },
}
```

All errors include span information for precise error reporting.

## Implementation Details

### Type Checker State

```rust
struct TypeChecker {
    /// Symbol table for variables (with lexical scoping)
    symbols: SymbolTable,
    /// All function signatures including mangled method names
    functions: HashMap<String, Type>,
    /// Struct definitions: name → ordered [(field_name, field_type)]
    struct_defs: HashMap<String, Vec<(String, Type)>>,
    /// Methods per struct: struct_name → method_name → mangled key
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Collected errors (fail-slow approach)
    errors: Vec<TypeError>,
    /// Current function's return type (for return validation)
    current_function_return_type: Option<Type>,
    /// Active loop nesting depth (for break/continue validation)
    loop_depth: u32,
}
```

### Symbol Table

```rust
struct SymbolTable {
    /// Stack of scopes (innermost scope is last)
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

struct SymbolInfo {
    ty: Type,
    mutable: bool,
}

impl SymbolTable {
    fn push_scope(&mut self);
    fn pop_scope(&mut self);
    fn define(&mut self, name: String, info: SymbolInfo);
    fn lookup(&self, name: &str) -> Option<&SymbolInfo>;
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

**Test coverage**: 78 tests

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

### Three-Pass Algorithm

**Pass 1: Register struct definitions**
```rust
for item in items {
    if let Item::Struct(def) = item {
        self.register_struct(def);
    }
}
```

**Pass 2: Register impl method signatures** (uses struct types from Pass 1)
```rust
for item in items {
    if let Item::Impl(def) = item {
        self.register_impl(def); // Mangles names as StructName__methodName
    }
}
```

**Pass 3: Type-check function and method bodies**
```rust
for item in items {
    match item {
        Item::Function(func) => self.check_function(func),
        Item::Impl(def) => self.check_impl(def),
        Item::Struct(_) => {}
    }
}
```

This order guarantees: all struct types are known before method signatures are parsed (Pass 2), and all function/method signatures are known before any body is checked (Pass 3), enabling forward references, mutual recursion, and definition-order independence.

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

**Type check**: Pass
- `n` has type `i32`
- `n <= 1` returns `bool` (valid condition)
- `factorial(n - 1)` recursive call type checks
- All return statements return `i32`

### Type Checking Errors

```neuro
func bad_example() -> i32 {
    val x = "hello"      // x has type string
    return x + 1         // Error: can't add string and i32
}
```

**Errors**:
1. `TypeError::InvalidBinaryOperator`: Cannot apply `+` to `string` and `i32`

## Future Enhancements (Post-Phase 1)

### Phase 2: Enhanced Type System
- [x] **Structs**: User-defined types with nominal typing
- [x] **Methods**: `impl` blocks with `&self` instance methods and associated functions
- [ ] **Enums**: Algebraic data types with associated data
- [ ] **Pattern matching**: `match` with exhaustiveness checking
- [ ] **Arrays**: Fixed-size arrays `[i32; 10]`
- [ ] **Explicit conversions**: `as i64`, `as f32`
- [ ] **Generic functions**: Monomorphization

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
