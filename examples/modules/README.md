# NEURO Module System Examples

This directory contains examples demonstrating the NEURO module system.

## Files

- **`math.nr`** - Mathematical functions library
- **`utils.nr`** - General utility functions
- **`calculator.nr`** - Main application that imports and uses both modules

## Module System Features Demonstrated

### 1. Module Definition
Modules in NEURO are simply `.nr` files containing functions, structs, and other items. All top-level items are automatically exported.

### 2. Import Syntax
```neuro
import "./math";      // Relative import
import "./utils";     // Another relative import
```

### 3. Module Usage
Once imported, all exported functions from the module can be used directly:

```neuro
let result = add(5, 3);        // From math module
let max_val = max(10, 20);     // From utils module
```

## Running the Examples

```bash
# Parse and check the calculator example
cargo run --bin neurc -- check examples/modules/calculator.nr

# The module system will automatically resolve dependencies:
# calculator.nr -> math.nr, utils.nr
```

## Module Resolution

The NEURO module system resolves imports as follows:

1. **Relative imports** (starting with `./` or `../`):
   - Resolved relative to the current module's directory
   - `./math` resolves to `math.nr` in the same directory

2. **Absolute imports** (no prefix):
   - Searched in standard library locations
   - Searched in project source directories

3. **Extension handling**:
   - `.nr` extension is automatically added if not present
   - `import "./math"` finds `math.nr`

## Dependency Management

The module system automatically:
- Detects circular dependencies
- Provides topological ordering for compilation
- Caches resolved modules to avoid duplicate work
- Tracks module-to-module dependencies

## Current Limitations

This is an early implementation with some limitations:
- No explicit export control (all top-level items are exported)
- No namespacing (imported items go into global scope)
- No package management (only file-based modules)

These will be addressed in future iterations of the language.