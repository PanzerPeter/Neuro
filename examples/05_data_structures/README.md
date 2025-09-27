# 05_data_structures - Data Organization with Structs

This directory contains examples demonstrating NEURO's struct system for organizing related data into custom types.

## Examples

### 1. [01_struct_basics.nr](./01_struct_basics.nr)
- Struct declaration syntax
- Field definitions with different types
- Nested struct type references
- Current implementation limitations

## Key Concepts Covered

### Struct Declaration
- Basic syntax: `struct Name { field: Type, ... }`
- Mixed field types in single struct
- Meaningful field names and organization
- Struct type references in other structs

### Field Types
- All primitive types as fields
- Other struct types as fields (when fully implemented)
- Consistent naming conventions
- Type safety in field declarations

### Usage Patterns
- Data modeling with structs
- Complex data organization
- Type-safe data structures
- Future instantiation and field access

## Current Status

✅ **Working Features**:
- Struct declaration parsing
- Field type checking
- Struct type references
- Function signatures with struct types

⚠️ **Limitations**:
- Struct instantiation not yet implemented
- Field access (`.` operator) not yet working
- Struct methods not yet available
- Pattern matching on structs not implemented

## Future Syntax

When fully implemented, structs will support:

```neuro
// Instantiation
let point = Point { x: 10, y: 20 };

// Field access
let x_coord = point.x;

// Methods
impl Point {
    fn distance(self) -> float { ... }
}
```

## Running Examples

```bash
neurc run examples/05_data_structures/01_struct_basics.nr
```