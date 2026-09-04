# Semantic Analysis

**Status**: Complete (Phase 1)
**Crate**: `compiler/semantic-analysis`
**Entry Point**: `pub fn type_check(items: &[Item]) -> Result<Vec<Warning>, Vec<TypeError>>`

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

#### The Type Lattice

`Type` (see [`types.rs`](../../../compiler/semantic-analysis/src/types.rs)) covers:

- **Integers**: `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`
- **Floats**: `F16`, `BF16`, `F32`, `F64`
- **Other scalars**: `Bool`, `Char` (a 32-bit Unicode scalar, ordered and castable, but not
  arithmetic), `String`, `Void`
- **Nominal user types**: `Struct(name)`, `Enum(name)`, `Newtype(name)`, two values share a
  type only when the names match. A monomorphized generic is an ordinary nominal type with a
  mangled name, which is what keeps generics invisible to everything downstream
- **Compound**: `Array { element, size }` (length is part of the type), `Tuple`,
  `Reference { inner, mutable }`, `Function { params, ret }`, `DynObject` (a `&dyn Trait`
  receiver)
- **`Unknown`**: the error-recovery type. It is compatible with everything, so one reported
  error does not cascade into a second. Divergent expressions, `panic`, `unreachable`, and
  an `if`/`match` arm that `return`s, `break`s, or `continue`s, carry it for the same
  reason: they never produce a value, so they must not constrain the arms that do

#### Type Compatibility

There are **no implicit conversions**: compatibility is name-and-shape equality, with `as`
the only widening or narrowing mechanism. `Unknown` is the single exception, and is
compatible in both directions.

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

Every diagnostic is a `TypeError` variant carrying the span of the offending expression or item;
the checker collects them and keeps going, so one run reports every error it finds. The set is
large (core mismatches and undefined names; mutability; generics, turbofish, and trait bounds;
`Copy`/`Drop`/operator-trait rules; control flow) and grows with each feature, so this document
does not reproduce it. The authoritative list, with its user-facing messages, is
[`compiler/semantic-analysis/src/errors.rs`](../../../compiler/semantic-analysis/src/errors.rs).

## Implementation Details

### Type Checker State

The checker holds a scope stack of bindings (each with its type, mutability, and move state),
maps of registered functions, structs, methods, traits, generic enums, and newtypes, the
accumulated `TypeError` and `Warning` lists, and per-function context (return type, loop depth)
for validation. Field-level detail belongs to
[`type_checkers/mod.rs`](../../../compiler/semantic-analysis/src/type_checkers/mod.rs); a pasted
copy here would only drift.

The symbol table tracks lexical scopes plus ownership: whether a binding still owns its value,
which is what drives use-after-move detection in
[`type_checkers/moves.rs`](../../../compiler/semantic-analysis/src/type_checkers/moves.rs).

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

### Multi-Pass Algorithm

`check_program` walks the item list several times. Each pass registers only what the
next one needs, which is what makes declaration order irrelevant. Passes are lettered
where a later requirement was slotted between two existing ones.

| Pass | What it does | Why it sits here |
|---|---|---|
| 0a | Pre-register newtype *names* (`predeclare_newtype`) | a newtype may appear as a struct field, enum payload, or another newtype's inner before its own declaration |
| 0 | Register enum definitions (generic ones via `register_generic_enum`, which keeps the template under its base name for construction-site inference) | an enum may be a struct field type, and vice versa |
| 1 | Register struct definitions (generic ones via `register_generic_struct`); record `Copy`/`Clone` derive intent | type names must resolve in method signatures |
| 1c | Resolve and validate newtype inner types | every nominal name is known by now; enforces the `Copy`-inner rule and rejects cycles |
| 1b | Validate `@derive(Copy)`, every field of a `Copy` struct is itself `Copy` | runs after 1c so a newtype field reports its real `Copy`-ness |
| 1d | Register trait declarations | `impl Trait for T` conformance and generic trait bounds need the trait's method signatures |
| 2 | Register `impl` method signatures (generic ones via `register_generic_impl`) | uses the struct types from pass 1 |
| 2b | Operator-trait supertrait check (`check_operator_supertraits`) | all impls are registered, so `Comparable: PartialEq` is order-independent |
| 3 | Register module-level constants | they must be visible in every function body |
| 4 | Check function, method, and const **bodies** | every signature is known, so forward references and mutual recursion resolve |
| 5 | Lints (`run_lints`) | run independently of type errors so style guidance always reaches the developer |

Pass 2 registers each method under a mangled key, see
[Method Name Mangling](#method-name-mangling) below.

The ordering guarantees that all type names are known before any signature is read, and
all signatures are known before any body is checked, which is what enables forward
references, mutual recursion, and definition-order independence.

### Method Name Mangling

Methods live in the same flat function table as free functions, keyed by
`StructName__methodName`. `__` is reserved as the compiler's symbol separator: the
backend recovers a method's receiver struct by splitting its symbol on `__`, so no
generated name may introduce a second `__`.

Two consequences:

- Monomorphized instance names use a single-underscore marker, `_g_` for a generic
  struct instance (`Pair_g_i32_f64`) and for a generic function instance
  (`identity_g_i32`), never `__`.
- User-declared identifiers may not contain `__`. A declaration that does is rejected
  with `TypeError::ReservedNameSeparator`, which keeps a user method from ever colliding
  with a generated instance or vtable-thunk symbol.

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
/// Type check a Neuro program, returning the lint warnings it collected
pub fn type_check(items: &[Item]) -> Result<Vec<Warning>, Vec<TypeError>>
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
- **LSP server** (Phase 8): Type information for IDE features

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

## Future Enhancements

### Open checker items

Phase 1 (Core Language) is complete: structs, methods, arrays, tuples, `as` conversions,
the borrow checker, enums and pattern matching, generics, traits, and dispatch have all
landed. See the [Quick Roadmap](../../../README.md#quick-roadmap) for the phase now open.
What the checker still owes:

- [ ] **Generic type arguments beyond `Copy`**: type arguments are `Copy`-restricted, and a
      generic may not be instantiated with an enclosing type parameter
- [ ] **A `never` type**: divergence is modelled with `Unknown` today, which is compatible
      with everything by design; a dedicated bottom type would let the checker distinguish
      "diverges" from "unknown because an error was already reported"

### Phase 2: Tensor Types
- [x] **Static tensor types**: `Tensor<f32, [3, 3]>` resolves to `Type::Tensor { element, shape }`;
      the element is restricted to a fixed-width scalar and the type is non-`Copy`
- [ ] **Shape checking beyond identity**: broadcasting and shape generics; today two tensors
      match only when their elements and every extent are equal
- [ ] **Broadcasting**: NumPy-style broadcasting rules

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
- **Body checking**: one traversal per body, after the registration passes; the
  registration passes themselves are linear scans over the item list

## References

- [Type Systems](https://www.cs.cornell.edu/courses/cs4110/2018fa/lectures/)
- [Bidirectional Type Checking](https://arxiv.org/abs/1908.05839)
- Source: [compiler/semantic-analysis/src/lib.rs](../../compiler/semantic-analysis/src/lib.rs)
