# NEURO Module System

The NEURO module system provides a clean way to organize code into reusable components while maintaining the performance characteristics needed for ML workloads.

## Overview

NEURO uses a file-based module system where each `.nr` file represents a module. The system is designed to be:

- **Simple**: File-based modules with intuitive import syntax
- **Fast**: Efficient dependency resolution and caching
- **Safe**: Circular dependency detection and clear error messages
- **Scalable**: Supports large codebases with complex dependency graphs

## Module Definition

A module in NEURO is simply a `.nr` file containing functions, structs, and other top-level items:

```neuro
// math.nr - A math utilities module
fn add(a: int, b: int) -> int {
    a + b
}

fn multiply(a: int, b: int) -> int {
    a * b
}
```

All top-level items in a module are automatically exported and available to importing modules.

## Import Syntax

### Relative Imports

Import modules relative to the current file:

```neuro
import "./math";           // Import math.nr from same directory
import "../utils/helpers"; // Import from parent directory
```

### Absolute Imports

Import from standard search paths:

```neuro
import "std::collections"; // Standard library module
import "my_lib::tensor";   // Project library module
```

## Module Resolution

The module system resolves imports using the following algorithm:

1. **Relative imports** (starting with `./` or `../`):
   - Resolved relative to the importing file's directory
   - `./math` in `/project/src/main.nr` resolves to `/project/src/math.nr`

2. **Absolute imports**:
   - Searched in configured search paths
   - Default paths: current directory, `src/`, `lib/`

3. **File extensions**:
   - `.nr` extension automatically added if not present
   - Directory imports look for `mod.nr` file

## Dependency Management

### Automatic Dependency Tracking

The module system automatically tracks dependencies between modules:

```neuro
// main.nr
import "./math";
import "./utils";

// math.nr  
import "./constants";

// Dependency graph: main → math → constants
//                   main → utils
```

### Circular Dependency Detection

The system detects and prevents circular dependencies:

```neuro
// a.nr
import "./b";  // Error: circular dependency detected

// b.nr  
import "./a";
```

### Topological Ordering

Modules are compiled in dependency order:

1. Modules with no dependencies first
2. Modules depending on already compiled modules
3. Ensures all dependencies are available at compile time

## Implementation Architecture

The module system follows Vertical Slice Architecture (VSA) principles:

### Core Components

- **`ModuleRegistry`**: Tracks loaded modules and their metadata
- **`ImportResolver`**: Resolves import paths to filesystem locations
- **`DependencyGraph`**: Manages module dependencies and ordering
- **`ModuleSystem`**: Main interface coordinating all components

### Key Features

- **Caching**: Resolved imports are cached to avoid duplicate work
- **Error Recovery**: Clear error messages for missing or invalid modules
- **Performance**: Efficient algorithms for large dependency graphs

## Usage Examples

### Basic Module Usage

```neuro
// utils.nr
fn helper_function(x: int) -> int {
    x * 2
}

// main.nr
import "./utils";

fn main() -> int {
    let result = helper_function(21);
    print("Result: " + to_string(result));
    0
}
```

### Complex Dependencies

```neuro
// tensor_ops.nr
import "./math";
import "./memory";

fn tensor_add(a: Tensor<f32>, b: Tensor<f32>) -> Tensor<f32> {
    // Implementation using math and memory modules
}

// ml_model.nr  
import "./tensor_ops";
import "./activation";

fn neural_network(input: Tensor<f32>) -> Tensor<f32> {
    let hidden = tensor_add(input, weights);
    activation::relu(hidden)
}
```

## Future Enhancements

The module system will be extended with:

1. **Explicit Exports**: Control which items are exported from a module
2. **Namespacing**: Prevent naming conflicts between modules  
3. **Package Management**: Support for versioned packages and dependencies
4. **Standard Library**: Comprehensive standard library modules
5. **Precompiled Modules**: Cache compiled modules for faster builds

## Integration with NEURO Features

### Tensor Types

Modules can export tensor types and operations:

```neuro
// tensor_lib.nr
fn create_tensor(shape: [usize; 2]) -> Tensor<f32, shape> {
    // Create tensor with given shape
}

// main.nr
import "./tensor_lib";

fn main() {
    let t = create_tensor([3, 4]);
    // Use tensor...
}
```

### Automatic Differentiation

Modules can export functions with `#[grad]` attribute:

```neuro
// neural_ops.nr
#[grad]
fn linear_layer(input: Tensor<f32>) -> Tensor<f32> {
    // Neural network layer with automatic gradients
}
```

### GPU Kernels

Modules can export GPU-accelerated functions:

```neuro
// gpu_ops.nr
#[kernel(cuda)]
fn matrix_multiply(a: Tensor<f32>, b: Tensor<f32>) -> Tensor<f32> {
    // GPU-accelerated matrix multiplication
}
```

## API Reference

### ModuleSystem

Main interface for module operations:

```rust
let mut module_system = ModuleSystem::new();

// Register a module
let module_id = module_system.register_module(path, program);

// Resolve imports
let dependencies = module_system.resolve_imports(module_id)?;

// Get module information
let module = module_system.get_module(module_id);
```

### Error Handling

The module system provides detailed error information:

- `ModuleError::ModuleNotFound`: Module file not found
- `ModuleError::ImportResolutionFailed`: Cannot resolve import path
- `ModuleError::CircularDependency`: Circular dependency detected
- `ModuleError::FileReadError`: Error reading module file

## Best Practices

1. **Use descriptive module names**: `tensor_operations` not `ops`
2. **Keep modules focused**: Single responsibility per module
3. **Minimize dependencies**: Reduce coupling between modules
4. **Document public interfaces**: Clear documentation for exported functions
5. **Use relative imports**: For project-internal modules

## Testing

The module system is thoroughly tested with:

- Unit tests for each component
- Integration tests for end-to-end workflows
- Property-based tests for edge cases
- Performance tests for large dependency graphs

See `compiler/module-system/tests/` for comprehensive test coverage.