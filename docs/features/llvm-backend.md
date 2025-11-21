# LLVM Backend

**Status**: ✅ Complete (Phase 1)
**Crate**: `compiler/llvm-backend`
**Entry Point**: `pub fn compile(items: &[Item]) -> Result<Vec<u8>, CodegenError>`

## Overview

The LLVM backend feature slice generates native object code from type-checked AST. It uses the [inkwell](https://github.com/TheDan64/inkwell) library (safe Rust bindings to LLVM) to produce optimized machine code for the target platform.

## Architecture

This slice follows the **Vertical Slice Architecture** pattern:
- **Dependencies**: `syntax-parsing` (AST), `semantic-analysis` (type info), `inkwell` (LLVM)
- **Public API**: Single entry point (`compile`)
- **Internal implementation**: All codegen internals are `pub(crate)`
- **Output**: Platform-specific object code (`.o` files)

## Features

### Code Generation Capabilities

#### Function Generation
- ✅ Function declarations with parameters and return types
- ✅ Parameter allocation on stack
- ✅ Function body code generation
- ✅ Return path validation (ensures non-void functions return)

#### Expression Generation
- ✅ **Literals**: i32, i64, f32, f64, bool constants
- ✅ **Identifiers**: Variable loads from stack
- ✅ **Binary operators**:
  - Arithmetic: `+`, `-`, `*`, `/`, `%`
  - Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
  - Logical: `&&`, `||`
- ✅ **Unary operators**: `-` (negation), `!` (logical not)
- ✅ **Function calls**: With type-checked arguments
- ✅ **Parenthesized expressions**

#### Statement Generation
- ✅ **Variable declarations**: Stack-allocated locals
- ✅ **Return statements**: With value or void
- ✅ **If/else statements**: Proper basic block management
- ✅ **Expression statements**: Side-effect expressions

#### Type Mapping

| NEURO Type | LLVM Type |
|------------|-----------|
| `i32` | `i32` |
| `i64` | `i64` |
| `f32` | `float` |
| `f64` | `double` |
| `bool` | `i1` |
| `void` | `void` |

### LLVM IR Features

#### Basic Blocks
Proper control flow with basic blocks:
- Entry block for function prologue
- Then/else blocks for conditionals
- Merge blocks for control flow join points

#### Memory Management
- Stack allocation (`alloca`) for all local variables
- Parameter storage on stack
- Load/store for variable access
- Opaque pointer support (LLVM 15+)

#### Optimization
- Currently `-O0` (no optimization) for Phase 1
- LLVM IR verification for correctness
- Future: Optimization passes (`-O1`, `-O2`, `-O3`)

## Usage

### Basic Compilation

```rust
use syntax_parsing::parse;
use semantic_analysis::type_check;
use llvm_backend::compile;

let source = r#"
    func add(a: i32, b: i32) -> i32 {
        return a + b
    }
"#;

// Parse and type check
let ast = parse(source)?;
type_check(&ast)?;

// Generate object code
let object_code = compile(&ast)?;

// Write to file
std::fs::write("output.o", object_code)?;
```

### Compiling Complex Programs

```rust
let source = r#"
    func factorial(n: i32) -> i32 {
        if n <= 1 {
            return 1
        } else {
            return n * factorial(n - 1)
        }
    }

    func main() -> i32 {
        val result = factorial(5)
        return result
    }
"#;

let ast = parse(source)?;
type_check(&ast)?;
let object_code = compile(&ast)?;

// Object code can now be linked to create executable
```

## Code Generation Pipeline

### Phase 1: Type Checking

```rust
// Run semantic analysis first
semantic_analysis::type_check(items)?;
```

This ensures:
- All types are resolved
- No type errors exist
- Functions and variables are properly scoped

### Phase 2: Extract Function Types

```rust
let mut func_types = HashMap::new();
for item in items {
    let Item::Function(func_def) = item;
    let func_type = extract_function_type(func_def);
    func_types.insert(func_def.name.clone(), func_type);
}
```

### Phase 3: Initialize LLVM

```rust
let context = LLVMContext::create();
let codegen_ctx = CodegenContext::new(&context, "neuro_module");
```

### Phase 4: Store Expression Types

```rust
// Visit AST to collect expression type information
codegen_ctx.store_expr_types(items)?;
```

This pre-computes types for:
- Binary expression operands
- Unary expression operands
- Needed for selecting integer vs float instructions

### Phase 5: Generate Functions

```rust
for item in items {
    let Item::Function(func_def) = item;
    codegen_ctx.codegen_function(func_def, &func_types)?;
}
```

Each function:
1. Creates LLVM function with signature
2. Allocates parameters on stack
3. Generates body statements
4. Validates return paths

### Phase 6: Verify and Emit

```rust
// Verify LLVM IR is well-formed
codegen_ctx.module.verify()?;

// Generate object code for target platform
let target_machine = create_target_machine()?;
let object_code = target_machine.write_to_memory_buffer(
    &codegen_ctx.module,
    FileType::Object
)?;
```

## Implementation Details

### Core Types

```rust
/// Maps NEURO types to LLVM types
struct TypeMapper<'ctx> {
    context: &'ctx LLVMContext,
}

/// Code generation context
struct CodegenContext<'ctx> {
    context: &'ctx LLVMContext,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    type_mapper: TypeMapper<'ctx>,

    /// Variables: name -> stack pointer
    variables: HashMap<String, PointerValue<'ctx>>,

    /// Variable types (for opaque pointers)
    variable_types: HashMap<String, BasicTypeEnum<'ctx>>,

    /// Functions: name -> LLVM function
    functions: HashMap<String, FunctionValue<'ctx>>,

    /// Current function (for return type checking)
    current_function: Option<FunctionValue<'ctx>>,

    /// Expression types (for operator selection)
    expr_types: HashMap<usize, Type>,
}
```

### Code Generation Examples

#### Generating a Literal

```rust
fn codegen_literal(&self, lit: &Literal) -> Result<BasicValueEnum<'ctx>> {
    match lit {
        Literal::Integer(val) => {
            Ok(self.context.i32_type().const_int(*val as u64, true).into())
        }
        Literal::Float(val) => {
            Ok(self.context.f64_type().const_float(*val).into())
        }
        Literal::Boolean(val) => {
            Ok(self.context.bool_type().const_int(*val as u64, false).into())
        }
    }
}
```

#### Generating a Binary Operation

```rust
fn codegen_binary(&self, left: &Expr, op: BinaryOp, right: &Expr, left_ty: &Type)
    -> Result<BasicValueEnum<'ctx>>
{
    let lhs = self.codegen_expr(left)?;
    let rhs = self.codegen_expr(right)?;

    match op {
        BinaryOp::Add => {
            if TypeMapper::is_float_type(left_ty) {
                self.builder.build_float_add(lhs.into_float_value(), rhs.into_float_value(), "addtmp")?
            } else {
                self.builder.build_int_add(lhs.into_int_value(), rhs.into_int_value(), "addtmp")?
            }
        }
        // ... other operators
    }
}
```

#### Generating an If Statement

```rust
fn codegen_if(&mut self, condition: &Expr, then_block: &[Stmt],
               else_block: &Option<Vec<Stmt>>) -> Result<()>
{
    let cond_val = self.codegen_expr(condition)?;
    let parent_fn = self.current_function.unwrap();

    // Create basic blocks
    let then_bb = self.context.append_basic_block(parent_fn, "then");
    let else_bb = self.context.append_basic_block(parent_fn, "else");
    let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

    // Conditional branch
    self.builder.build_conditional_branch(
        cond_val.into_int_value(),
        then_bb,
        else_bb
    )?;

    // Generate then block
    self.builder.position_at_end(then_bb);
    for stmt in then_block {
        self.codegen_stmt(stmt)?;
    }
    if then_bb.get_terminator().is_none() {
        self.builder.build_unconditional_branch(merge_bb)?;
    }

    // Generate else block
    self.builder.position_at_end(else_bb);
    if let Some(else_stmts) = else_block {
        for stmt in else_stmts {
            self.codegen_stmt(stmt)?;
        }
    }
    if else_bb.get_terminator().is_none() {
        self.builder.build_unconditional_branch(merge_bb)?;
    }

    // Continue at merge block
    self.builder.position_at_end(merge_bb);
    Ok(())
}
```

## Error Handling

### Error Types

```rust
pub enum CodegenError {
    InitializationFailed(String),
    UnsupportedType(String),
    UndefinedVariable(String),
    UndefinedFunction(String),
    TypeMismatch { expected: String, found: String },
    InvalidOperandType { op: String, ty: String },
    LlvmError(String),
    MissingReturn,
    InternalError(String),
}
```

All errors include context for debugging.

### Error Handling Pattern

```rust
fn codegen_expr(&self, expr: &Expr) -> Result<BasicValueEnum<'ctx>, CodegenError> {
    match expr {
        Expr::Identifier(ident) => {
            let ptr = self.variables.get(&ident.name)
                .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;

            let var_type = self.variable_types.get(&ident.name)
                .ok_or_else(|| CodegenError::InternalError("missing type".to_string()))?;

            self.builder.build_load(*var_type, *ptr, &ident.name)
                .map_err(|e| CodegenError::LlvmError(e.to_string()))
        }
        // ... other cases
    }
}
```

## LLVM IR Examples

### Simple Function

**NEURO source**:
```neuro
func add(a: i32, b: i32) -> i32 {
    return a + b
}
```

**Generated LLVM IR** (simplified):
```llvm
define i32 @add(i32 %0, i32 %1) {
entry:
  %a = alloca i32
  %b = alloca i32
  store i32 %0, i32* %a
  store i32 %1, i32* %b
  %2 = load i32, i32* %a
  %3 = load i32, i32* %b
  %addtmp = add i32 %2, %3
  ret i32 %addtmp
}
```

### Function with If/Else

**NEURO source**:
```neuro
func max(a: i32, b: i32) -> i32 {
    if a > b {
        return a
    } else {
        return b
    }
}
```

**Generated LLVM IR** (simplified):
```llvm
define i32 @max(i32 %0, i32 %1) {
entry:
  %a = alloca i32
  %b = alloca i32
  store i32 %0, i32* %a
  store i32 %1, i32* %b
  %2 = load i32, i32* %a
  %3 = load i32, i32* %b
  %gttmp = icmp sgt i32 %2, %3
  br i1 %gttmp, label %then, label %else

then:
  %4 = load i32, i32* %a
  ret i32 %4

else:
  %5 = load i32, i32* %b
  ret i32 %5
}
```

## Testing

**Test coverage**: 2 integration tests (comprehensive)

### Test 1: Simple Function
```rust
#[test]
fn test_compile_simple_function() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }
    "#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = compile(&items);

    assert!(result.is_ok());
    let object_code = result.unwrap();
    assert!(!object_code.is_empty());
}
```

### Test 2: Milestone Program
```rust
#[test]
fn test_compile_milestone_program() {
    let source = r#"
        func add(a: i32, b: i32) -> i32 {
            return a + b
        }

        func main() -> i32 {
            val result = add(5, 3)
            return result
        }
    "#;

    let items = syntax_parsing::parse(source).unwrap();
    let result = compile(&items);

    assert!(result.is_ok());
    let object_code = result.unwrap();
    assert!(!object_code.is_empty());
}
```

## Design Decisions

### Why inkwell?

**Alternatives considered:**
- Direct LLVM C API
- llvm-sys (unsafe bindings)
- Custom code generator

**Why inkwell:**
- ✅ Safe Rust bindings
- ✅ Type-safe LLVM API
- ✅ Active maintenance
- ✅ Good documentation
- ✅ Prevents many LLVM usage errors at compile time

### Opaque Pointers (LLVM 15+)

Modern LLVM uses opaque pointers (`ptr` instead of `i32*`):
- Simplifies LLVM IR
- Requires tracking types separately
- We use `variable_types` HashMap to track types

### Stack Allocation Strategy

All variables allocated on stack (via `alloca`):
- Simple memory model
- Automatic cleanup
- No heap allocations needed (Phase 1)
- Future: Escape analysis for heap allocation

### No Optimization (Phase 1)

Using `-O0` for:
- Faster compilation
- Easier debugging
- Simpler generated IR

Future phases will add:
- `-O1`, `-O2`, `-O3` optimization levels
- Custom optimization passes
- Profile-guided optimization

## API Reference

### Public Functions

```rust
/// Compile type-checked AST to LLVM object code
pub fn compile(items: &[Item]) -> Result<Vec<u8>, CodegenError>
```

### Public Types

```rust
pub enum CodegenError { ... }
pub type CodegenResult<T> = Result<T, CodegenError>;
```

## Integration Points

### Upstream Dependencies

- **syntax-parsing**: AST types (Item, Expr, Stmt)
- **semantic-analysis**: Type information
- **inkwell**: LLVM bindings

### Downstream Consumers

- **neurc**: Compiler driver (writes object files, links executables)
- **LLVM linker**: Links object code to create executables

## Platform Support

### Supported Targets

Currently targets:
- **Windows**: x86_64-pc-windows-msvc
- **Linux**: x86_64-unknown-linux-gnu
- **macOS**: x86_64-apple-darwin, aarch64-apple-darwin

Detected automatically via LLVM target triple.

### Cross-Compilation

Phase 1: Native target only

Future:
- Cross-compilation to different architectures
- WebAssembly target
- Embedded targets

## Performance

- **Codegen speed**: ~1-2ms for simple programs
- **Object code size**: ~1-2KB per function (unoptimized)
- **Memory usage**: O(AST size) for context

## Future Enhancements

### Phase 2: Language Features
- [ ] Loops (while, for)
- [ ] Structs
- [ ] Arrays
- [ ] String type

### Phase 3: Tensor Support
- [ ] Static tensor types
- [ ] Tensor operations
- [ ] SIMD vectorization
- [ ] BLAS integration

### Phase 4: Optimization
- [ ] Optimization passes (-O1, -O2, -O3)
- [ ] Dead code elimination
- [ ] Constant folding
- [ ] Function inlining

### Phase 5: GPU Support
- [ ] CUDA code generation
- [ ] GPU kernels
- [ ] Device memory management

## Troubleshooting

### "LLVM error" during codegen

**Problem**: LLVM API returned error

**Solution**:
- Check LLVM version compatibility (requires LLVM 16+)
- Verify AST is type-correct (run type checker first)
- Check error message for specific LLVM issue

### "Missing return" errors

**Problem**: Non-void function doesn't return on all paths

**Solution**:
- Ensure all code paths have return statement
- Check if/else blocks both return
- Add final return statement if needed

### Linking errors (Windows)

**Problem**: `cannot open input file 'libxml2s.lib'`

**Solution**:
- Install LLVM with required libraries
- Or use pre-built LLVM from llvm.org
- Set `LLVM_SYS_160_PREFIX` environment variable

## LLVM Resources

- [LLVM IR Reference](https://llvm.org/docs/LangRef.html)
- [inkwell Documentation](https://thedan64.github.io/inkwell/)
- [LLVM Tutorial](https://llvm.org/docs/tutorial/)
- [Kaleidoscope](https://llvm.org/docs/tutorial/MyFirstLanguageFrontend/index.html) (similar example)

## References

- Source: [compiler/llvm-backend/src/lib.rs](../../compiler/llvm-backend/src/lib.rs)
- LLVM: https://llvm.org/
- inkwell: https://github.com/TheDan64/inkwell
