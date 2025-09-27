# Modules and Imports

NEURO provides a module system for organizing code across multiple files and enabling code reuse. The import system allows bringing functionality from other modules into the current scope.

## Import Statement Syntax

NEURO supports two different import syntaxes:

### Import Statement
```neuro
import path;
```

### Use Statement
```neuro
use path;
```

Both syntaxes are functionally equivalent for identifier chains. The `path` can be:
- An identifier chain: `a::b::c` (supported by both `import` and `use`)
- A string path: `"./file.nr"` or `"../lib/utils.nr"` (currently only supported by `import`)

### Examples
```neuro
import std::math;              // Standard library module
import "./lib/utils.nr";       // Relative file import
import "../shared/common.nr";  // Parent directory import
import my_module::functions;   // Nested module import

// Use syntax (for identifier chains only)
use std::math;                 // Standard library module
// use "./lib/utils.nr";       // String paths not yet supported with use
// use "../shared/common.nr";  // String paths not yet supported with use
use my_module::functions;      // Nested module import
```

## Import Path Types

### Identifier Chain Imports
Use `::` to separate module path components:
```neuro
import std::math;           // Standard library math module
import std::io::filesystem; // Nested standard library module
import my_library::utils;   // Local library module
```

### String Path Imports
Use quoted strings for explicit file paths:
```neuro
import "./utils.nr";         // Current directory
import "../shared/types.nr"; // Parent directory
import "lib/helpers.nr";     // Subdirectory
```

## Module Resolution

The NEURO module resolver follows a systematic search process:

### Search Path Order
1. **Current directory**: `./`
2. **Source directory**: `src/`
3. **Library directory**: `lib/`
4. **Standard library paths**

### File Resolution
- String imports are resolved relative to the current file
- Identifier imports are resolved through the search path
- All imports are normalized to `.nr` file extensions

### Examples of Resolution
```neuro
// File: src/main.nr
import "./helper.nr";      // Resolves to src/helper.nr
import "../lib/utils.nr";  // Resolves to lib/utils.nr
import std::math;          // Resolves to standard library
```

## Import Usage Examples

### Basic Module Import
```neuro
// File: lib/math_utils.nr
fn square(x: int) -> int {
    return x * x;
}

// File: src/main.nr
import "../lib/math_utils.nr";

fn main() -> int {
    // Usage of imported functions (when fully implemented)
    return 0;
}
```

### Multiple Imports
```neuro
import std::math;
import "./constants.nr";
import "../lib/helpers.nr";
import my_package::utilities;

// Mixed import and use syntax
use std::io;
use "./config.nr";

fn main() -> int {
    // Multiple imported modules available in scope
    return 0;
}
```

## Module Organization Patterns

### Directory Structure Example
```
project/
├── src/
│   ├── main.nr
│   └── utils.nr
├── lib/
│   ├── math.nr
│   └── string_ops.nr
└── examples/
    └── demo.nr
```

### Import Examples for Structure
```neuro
// In src/main.nr
import "./utils.nr";        // Same directory
import "../lib/math.nr";    // Library directory

// In examples/demo.nr
import "../src/utils.nr";   // Source directory
import "../lib/math.nr";    // Library directory
```

## Complete Example

```neuro
// File: lib/geometry.nr
struct Point {
    x: float,
    y: float,
}

fn distance(p1: Point, p2: Point) -> float {
    // Implementation would go here
    return 0.0;
}

// File: src/main.nr
import "../lib/geometry.nr";
import std::math;

fn main() -> int {
    // When module system is fully implemented:
    // - Can use Point struct from geometry module
    // - Can call distance function
    // - Can use std::math functions
    return 0;
}
```

## Current Implementation Status

### Fully Implemented ✅
- Import statement parsing
- Both identifier chain and string path syntax
- Basic module resolution framework
- Search path logic (current dir, src/, lib/)
- File path normalization to `.nr` extensions
- Relative import path handling

### Partially Implemented ⚠️
- Module resolver infrastructure
- Dependency graph construction
- Circular dependency detection (framework exists)

### Not Yet Implemented ❌
- Symbol resolution from imported modules
- Name collision handling
- Selective imports (`import module::{ function1, function2 }`)
- Import aliasing (`import module as alias`)
- Re-exports from modules
- Module visibility and access control
- Package management integration
- Standard library modules

## Error Handling

The module system will provide clear error messages for:
- File not found errors
- Circular dependency detection
- Import path resolution failures
- Symbol not found in imported modules

## Future Module Features

Planned enhancements include:

### Selective Imports
```neuro
// Planned syntax (not yet implemented)
import std::math::{ sin, cos, tan };
import "./utils.nr"::{ helper_function, CONSTANT };
```

### Import Aliases
```neuro
// Planned syntax (not yet implemented)
import std::math as m;
import "./very_long_module_name.nr" as short;
```

### Re-exports
```neuro
// Planned syntax (not yet implemented)
pub import std::math;  // Re-export for other modules to use
```

