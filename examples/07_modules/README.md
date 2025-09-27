# 07_modules - Module System and Imports

This directory contains examples demonstrating NEURO's module system for code organization and reuse across multiple files.

## Examples

### 1. [01_import_basics.nr](./01_import_basics.nr)
- Import statement syntax with string paths
- Import statement syntax with identifier chains
- Relative and absolute path imports
- Module resolution concepts

## Key Concepts Covered

### Import Syntax
- **String paths**: `import "./utils.nr";`
- **Identifier chains**: `import std::math;`
- **Relative paths**: `import "../lib/helper.nr";`
- **Nested modules**: `import std::collections::vector;`

### Module Resolution
Search order for module resolution:
1. Current directory (`./`)
2. Source directory (`src/`)
3. Library directory (`lib/`)
4. Standard library paths

### Import Types
- **File imports**: Direct file references with `.nr` extension
- **Package imports**: Hierarchical namespace-based imports
- **Standard library**: Built-in module access (future)
- **Third-party**: External package imports (future)

## Directory Structure

```
07_modules/
├── 01_import_basics.nr     # Main example file
├── lib/
│   └── math_utils.nr       # Importable utility module
└── README.md
```

## Current Status

✅ **Working Features**:
- Import statement parsing (both syntaxes)
- Module path resolution framework
- Relative path handling
- Basic dependency tracking

⚠️ **Partial Implementation**:
- Module resolution (parsing works, full resolution in progress)
- Symbol importing from modules
- Circular dependency detection

❌ **Not Yet Implemented**:
- Actual symbol resolution from imported modules
- Selective imports: `import module::{ item1, item2 }`
- Import aliasing: `import module as alias`
- Re-exports and visibility control

## Future Features

```neuro
// Selective imports (planned)
import std::math::{ sin, cos, tan };

// Import aliasing (planned)
import very_long_module_name as short;

// Re-exports (planned)
pub import std::collections;
```

## Running Examples

```bash
neurc run examples/07_modules/01_import_basics.nr
```

## File Organization

The module system encourages clean file organization:
- Keep related functionality in the same module
- Use clear, descriptive module names
- Organize modules in logical directory hierarchies
- Separate public interfaces from implementation details