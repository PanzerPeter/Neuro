# NEURO Module System

The NEURO module system provides a comprehensive framework for organizing code across multiple files and managing dependencies. This document covers the current Phase 1 implementation and planned features.

## Module System Overview

The NEURO module system is designed with these principles:
- **File-based modules**: Each `.nr` file is a module
- **Explicit imports**: Dependencies must be explicitly declared
- **Path-based resolution**: Clear, predictable module resolution
- **Circular dependency detection**: Prevents import cycles
- **Standard library support**: Built-in module ecosystem

## Import Statement Syntax

### Basic Import Forms

#### Identifier Chain Import
```neuro
import identifier::chain;
```
Uses `::` separators to specify module paths through the module hierarchy.

#### String Path Import
```neuro
import "./relative.nr";
import "../lib/utils.nr";
import "absolute/path.nr";
```
Uses quoted strings for explicit file paths relative to the current file.

### Import Examples

```neuro
// Standard library imports
import std::math;
import std::io::filesystem;

// Local module imports
import utils;
import my_package::helpers;

// Relative file imports
import "./lib/utils.nr";
import "../shared/common.nr";
import "../../core/types.nr";

// Direct file imports
import "math_functions.nr";
import "data_structures.nr";
```

## Module Resolution Algorithm

### Resolution Search Order

The module resolver follows this systematic search process:

1. **Current Directory**: `./`
   - Look for exact filename matches
   - Try adding `.nr` extension if needed

2. **Source Directory**: `src/`
   - Check for module files in source tree
   - Support nested directory structures

3. **Library Directory**: `lib/`
   - Search for library modules
   - Support for third-party packages

4. **Standard Library**: Built-in modules
   - Core language functionality
   - Math, I/O, and utility modules

### File Resolution Rules

#### Direct File Resolution
```neuro
import "./utils.nr";     // Resolves to: ./utils.nr
import "../lib/math.nr"; // Resolves to: ../lib/math.nr
```

#### Module Path Resolution
For identifier chain imports like `import std::math;`:

1. Check `std/math.nr`
2. Check `std/math/mod.nr` (module index file)
3. Search through standard library paths
4. Search through configured module paths

#### Extension Handling
- `.nr` extension is automatically added if omitted
- `mod.nr` files serve as directory module entry points

### Resolution Examples

Given this project structure:
```
project/
├── src/
│   ├── main.nr
│   ├── utils.nr
│   └── algorithms/
│       ├── mod.nr
│       └── sorting.nr
├── lib/
│   ├── math.nr
│   └── data_structures.nr
└── examples/
    └── demo.nr
```

Resolution behavior:
```neuro
// From src/main.nr
import utils;                    // → src/utils.nr
import algorithms;               // → src/algorithms/mod.nr
import "./utils.nr";             // → src/utils.nr
import "../lib/math.nr";         // → lib/math.nr

// From examples/demo.nr
import "../src/utils.nr";        // → src/utils.nr
import "../lib/data_structures.nr"; // → lib/data_structures.nr
```

## Module Registry and Tracking

### Module Loading

The module system maintains a registry of loaded modules:

```rust
// Internal representation (conceptual)
struct ModuleRegistry {
    loaded_modules: HashMap<ModuleId, Module>,
    resolution_cache: HashMap<ImportPath, ModuleId>,
    dependency_graph: DependencyGraph,
}
```

### Module Identification

Each module receives a unique identifier based on:
- Canonical file path
- Module content hash (for change detection)
- Dependencies and their versions

### Dependency Tracking

The system tracks:
- **Direct dependencies**: Modules explicitly imported
- **Transitive dependencies**: Dependencies of dependencies
- **Circular dependencies**: Detected and prevented
- **Module hierarchy**: Parent-child relationships

## Circular Dependency Detection

### Detection Algorithm

The module system prevents circular dependencies by:

1. **Dependency Graph Construction**: Building a directed graph of module dependencies
2. **Cycle Detection**: Using depth-first search to detect cycles
3. **Error Reporting**: Clear error messages when cycles are found

### Circular Dependency Example

```neuro
// File: a.nr
import "./b.nr";  // A imports B

fn func_a() -> int {
    return func_b();
}

// File: b.nr
import "./a.nr";  // B imports A (creates cycle)

fn func_b() -> int {
    return func_a();  // Circular dependency!
}
```

This would be detected and reported as an error:
```
Error: Circular dependency detected
  → a.nr imports b.nr
  → b.nr imports a.nr
  → This creates a circular dependency cycle
```

## Module System Features

### Phase 1 Implementation Status

#### Fully Implemented ✅
- **Import statement parsing**: Both identifier chains and string paths
- **Basic resolution algorithm**: Search path logic implemented
- **File path handling**: Relative and absolute path resolution
- **Module registry foundation**: Infrastructure for tracking loaded modules
- **Dependency graph framework**: Basic structure for dependency tracking
- **Circular dependency detection**: Framework in place

#### Partially Implemented ⚠️
- **Symbol resolution**: Importing specific functions/types from modules
- **Module caching**: Optimization for repeated imports
- **Standard library**: Core modules being developed

#### Not Yet Implemented ❌
- **Selective imports**: `import module::{ item1, item2 }`
- **Import aliasing**: `import module as alias`
- **Re-exports**: `pub import` for module re-exporting
- **Module visibility**: Public/private module boundaries
- **Package management**: External dependency management
- **Module attributes**: Metadata and configuration

## Complete Module Example

### Project Structure
```
calculator/
├── src/
│   ├── main.nr
│   └── operations/
│       ├── mod.nr
│       ├── basic.nr
│       └── advanced.nr
├── lib/
│   └── math_utils.nr
└── std/
    └── math.nr
```

### Module Files

#### `lib/math_utils.nr`
```neuro
// Mathematical utility functions
fn abs(x: int) -> int {
    if x < 0 {
        return -x;
    } else {
        return x;
    }
}

fn max(a: int, b: int) -> int {
    if a > b {
        return a;
    } else {
        return b;
    }
}
```

#### `src/operations/basic.nr`
```neuro
// Basic arithmetic operations
import "../../lib/math_utils.nr";

fn add(a: int, b: int) -> int {
    return a + b;
}

fn subtract(a: int, b: int) -> int {
    return a - b;
}

fn safe_divide(a: int, b: int) -> int {
    if abs(b) > 0 {  // Using imported function
        return a / b;
    } else {
        return 0;
    }
}
```

#### `src/operations/mod.nr`
```neuro
// Module index file
import "./basic.nr";
import "./advanced.nr";

// Re-export commonly used functions
// (When re-exports are implemented)
```

#### `src/main.nr`
```neuro
import std::math;
import "./operations/mod.nr";
import "../lib/math_utils.nr";

fn main() -> int {
    // Use imported functionality
    let result = add(10, 20);
    let maximum = max(result, 50);

    return maximum;
}
```

## Error Handling and Diagnostics

### Module Resolution Errors

Common error types and their messages:

#### File Not Found
```
Error: Module not found
  → Could not resolve import: "./nonexistent.nr"
  → Searched in:
    - ./nonexistent.nr
    - src/nonexistent.nr
    - lib/nonexistent.nr
```

#### Circular Dependencies
```
Error: Circular dependency detected
  → Import cycle: main.nr → utils.nr → helpers.nr → main.nr
  → Consider restructuring your modules to break this cycle
```

#### Invalid Import Path
```
Error: Invalid import path
  → Import path "::invalid::path" is not valid
  → Use either identifier chains (std::math) or file paths ("./file.nr")
```

## Future Enhancements

### Planned Phase 2 Features

#### Selective Imports
```neuro
// Import specific items from modules
import std::math::{ sin, cos, tan, PI };
import "./utils.nr"::{ helper_function, CONSTANT };
```

#### Import Aliasing
```neuro
// Alias modules for convenience
import std::math as m;
import "./very_long_module_name.nr" as short;

fn calculate() -> float {
    return m::sin(3.14159);
}
```

#### Re-exports
```neuro
// Re-export modules for other modules to use
pub import std::math;  // Makes math available to importers of this module
pub import "./internal.nr"::{ public_function };
```

#### Package Management
```neuro
// External package imports (planned)
import package::neural_networks::{ Layer, Network };
import package::linear_algebra::Matrix;
```

## Performance Considerations

### Module Loading Optimization

- **Lazy Loading**: Modules loaded only when needed
- **Caching**: Compiled modules cached for faster subsequent loads
- **Parallel Resolution**: Multiple modules resolved concurrently
- **Incremental Compilation**: Only changed modules recompiled

### Memory Management

- **Module Sharing**: Common modules shared between dependents
- **Garbage Collection**: Unused modules can be unloaded
- **Memory Mapping**: Large modules mapped rather than loaded entirely

## Best Practices

### Module Organization
1. **One concept per module**: Keep modules focused
2. **Clear naming**: Use descriptive module and file names
3. **Logical hierarchy**: Organize related modules together
4. **Minimal dependencies**: Reduce coupling between modules

### Import Guidelines
1. **Explicit imports**: Always import what you use
2. **Group imports**: Organize imports by category
3. **Relative paths**: Use relative imports for local modules
4. **Standard library first**: Import standard library modules first

### Example of Good Module Organization
```neuro
// Group and order imports clearly
import std::math;
import std::io;

import "./types.nr";
import "./utils.nr";
import "../shared/common.nr";

fn main() -> int {
    // Implementation
    return 0;
}
```

