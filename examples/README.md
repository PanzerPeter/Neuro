# NEURO Programming Language Examples

This directory contains comprehensive examples demonstrating all features of the NEURO programming language. The examples are organized into progressive learning modules, from basic language concepts to advanced programming patterns.

## 🎯 Quick Start

```bash
# Your first NEURO program
neurc run examples/01_basics/01_hello_world.nr

# Or explore by category
neurc run examples/02_functions/01_basic_functions.nr
neurc run examples/03_control_flow/01_if_statements.nr
```

## 📚 Learning Path

Follow this progression for structured learning:

### 🔰 **Beginner Level (01-03)**
Master core language fundamentals

### 🔧 **Intermediate Level (04-06)**
Learn advanced language features

### 🚀 **Advanced Level (07-09)**
Real-world patterns and complete projects

## 📁 Directory Structure

```
examples/
├── 01_basics/              # Language fundamentals
│   ├── 01_hello_world.nr      # First program and output
│   ├── 02_comments.nr          # Comment styles and documentation
│   ├── 03_literals.nr          # All literal types and usage
│   ├── 04_variables.nr         # Variables, mutability, and scope
│   ├── 05_operators.nr         # Operators and expressions
│   └── README.md
├── 02_functions/            # Code organization
│   ├── 01_basic_functions.nr   # Function syntax and calling
│   ├── 02_parameters_return.nr # Parameters and return patterns
│   ├── 03_recursion.nr         # Recursive algorithms
│   ├── 04_scope_examples.nr    # Function scope and variables
│   └── README.md
├── 03_control_flow/         # Program flow control
│   ├── 01_if_statements.nr     # Conditional execution
│   ├── 02_while_loops.nr       # Iterative patterns
│   ├── 03_break_continue.nr    # Loop control statements
│   ├── 04_nested_control.nr    # Complex control structures
│   └── README.md
├── 04_types/                # Type system [Coming Soon]
├── 05_data_structures/      # Structs and organization [Coming Soon]
├── 06_pattern_matching/     # Match expressions
│   ├── 01_basic_patterns.nr    # Pattern matching fundamentals
│   └── README.md [Coming Soon]
├── 07_modules/              # Module system [Coming Soon]
├── 08_advanced/             # Advanced patterns [Coming Soon]
└── 09_projects/             # Complete applications [Coming Soon]
```

## 🌟 Features Demonstrated

### ✅ Fully Implemented Features
- **Variables**: `let` declarations, mutability with `mut`, type inference
- **Functions**: Parameters, return values, recursion, scope
- **Control Flow**: `if/else`, `while` loops, `break/continue`
- **Operators**: Arithmetic, comparison, logical with proper precedence
- **Pattern Matching**: `match` expressions with literal and wildcard patterns
- **Comments**: Line (`//`) and block (`/* */`) comments with nesting
- **Types**: `int`, `float`, `bool`, `string` with inference

### 🚧 Partially Implemented
- **Structs**: Declaration syntax (instantiation coming soon)
- **Modules**: `import`/`use` statements (resolution in progress)
- **Tensors**: Type declarations (operations coming soon)

## 💡 Example Quality Standards

Every example includes:
- **📝 Documentation header** explaining purpose and concepts
- **💬 Inline comments** explaining each major concept
- **📊 Expected output** for verification
- **🔧 Compilation instructions** for easy testing
- **📈 Progressive complexity** building on previous concepts

### Example Template
```neuro
//! Example: [Title]
//!
//! Purpose: [What this demonstrates]
//!
//! Concepts covered:
//! - [Concept 1]
//! - [Concept 2]
//!
//! Expected output: [Expected results]
//! Compilation: neurc run path/to/example.nr

// Detailed explanatory comments
fn main() -> int {
    // Implementation with educational focus
    return 0;
}
```

## 🚀 Running Examples

### Individual Examples
```bash
# Run any example directly
neurc run examples/01_basics/01_hello_world.nr
neurc run examples/02_functions/03_recursion.nr
neurc run examples/03_control_flow/02_while_loops.nr
```

### Compilation Mode
```bash
# Compile to executable (requires LLVM)
neurc build examples/01_basics/01_hello_world.nr
./01_hello_world

# Check syntax without running
neurc check examples/01_basics/01_hello_world.nr
```

### Development Tools
```bash
# View tokenization
neurc tokenize examples/01_basics/01_hello_world.nr

# View parsed AST
neurc parse examples/01_basics/01_hello_world.nr

# View LLVM IR
neurc llvm examples/01_basics/01_hello_world.nr
```

## 📖 Learning Modules

### 01_basics - Language Fundamentals
**Purpose**: Master essential NEURO syntax and concepts
- Program structure and main function
- Variable declarations and type system
- Operators and expression evaluation
- Comments and code documentation

**Key Takeaways**: Basic program structure, variable usage, expression evaluation

### 02_functions - Code Organization
**Purpose**: Learn to structure code with functions
- Function definition and calling syntax
- Parameter passing and return values
- Recursive programming patterns
- Scope rules and variable interactions

**Key Takeaways**: Code reuse, algorithm implementation, scope management

### 03_control_flow - Program Logic
**Purpose**: Control program execution flow
- Conditional execution with if/else
- Iterative processing with while loops
- Loop control with break/continue
- Complex nested control structures

**Key Takeaways**: Decision making, repetitive operations, algorithm control

### 06_pattern_matching - Value-Based Control
**Purpose**: Pattern-based program logic
- Match expressions for value checking
- Literal and wildcard patterns
- Nested pattern matching
- Pattern matching in different contexts

**Key Takeaways**: Elegant conditional logic, value-based dispatch

## 🎓 Algorithmic Patterns Covered

### Mathematical Algorithms
- **Factorial**: Iterative and recursive implementations
- **Fibonacci**: Multiple calculation approaches
- **GCD**: Euclidean algorithm demonstration
- **Prime checking**: Efficient divisibility testing
- **Power calculation**: Loop-based exponentiation

### Data Processing Patterns
- **Accumulation**: Building results through iteration
- **Filtering**: Conditional data selection
- **Search**: Linear search with early termination
- **Counting**: Conditional counting patterns
- **Validation**: Input checking and range validation

### Control Flow Patterns
- **Guard clauses**: Early return for special cases
- **State machines**: Transition-based logic
- **Nested processing**: Multi-dimensional operations
- **Loop optimization**: Efficient iteration patterns

## 🔍 Code Quality Features

### Educational Focus
- **Progressive complexity**: Examples build on previous concepts
- **Real-world relevance**: Practical programming patterns
- **Best practices**: Idiomatic NEURO code style
- **Performance awareness**: Efficient algorithm choices

### Documentation Quality
- **Purpose-driven**: Each example teaches specific concepts
- **Self-contained**: Examples work independently
- **Well-commented**: Educational explanations throughout
- **Expected outputs**: Verifiable results provided

### Technical Accuracy
- **Compilation verified**: All examples work with current `neurc`
- **Feature complete**: Only demonstrates implemented features
- **Error-free**: No compilation warnings or errors
- **Type safe**: Proper type usage throughout

## 🛠️ Development Workflow

### Testing Your Understanding
1. **Read** the example and its documentation
2. **Predict** what the output should be
3. **Run** the example to verify your understanding
4. **Modify** the example to explore variations
5. **Create** your own examples using the patterns

### Building Your Own Programs
```bash
# Create a new program
echo 'fn main() -> int { print(42); return 0; }' > my_program.nr

# Test it
neurc run my_program.nr

# Explore with development tools
neurc check my_program.nr
neurc parse my_program.nr
```

## 🎯 Next Steps

After working through these examples:
1. **Experiment** with combining patterns from different modules
2. **Create** your own algorithms using learned techniques
3. **Explore** the compiler's development tools
4. **Contribute** your own example improvements

## 🔧 Implementation Status

These examples demonstrate **currently working** NEURO features:
- ✅ **All syntax shown compiles successfully**
- ✅ **All examples produce expected output**
- ✅ **All features are fully implemented in `neurc`**
- ✅ **Examples follow established NEURO conventions**

For the latest language development status, see `docs/specification/index.md`.

## 🤝 Contributing

To improve these examples:
1. **Follow the established template format**
2. **Ensure examples compile with current `neurc`**
3. **Include comprehensive documentation**
4. **Test examples thoroughly before submission**
5. **Focus on educational value over complexity**

## 📝 License

These examples are part of the NEURO programming language project and follow the same licensing terms as the main project.