# Structs

NEURO supports struct declarations for creating custom data types with named fields. Structs provide a way to group related data together.

## Struct Declaration Syntax

### Basic Syntax
```neuro
struct StructName {
    field1: Type1,
    field2: Type2,
    // ... more fields
}
```

### Example
```neuro
struct Pair {
    left: int,
    right: int,
}

struct Person {
    age: int,
    height: float,
    name: string,
    active: bool,
}
```

## Field Declarations

### Field Syntax
Each field in a struct must have:
- A field name (identifier)
- A colon (`:`)
- A type annotation
- A trailing comma (`,`)

### Supported Field Types
```neuro
struct DataTypes {
    count: int,
    ratio: float,
    message: string,
    enabled: bool,
    vector: Tensor<float, [3]>,
}
```

## Struct Usage

### Current Implementation Status
```neuro
fn main() -> int {
    // Field initialization and usage are not yet fully implemented
    // This demonstrates current parsing support
    return 0;
}
```

## Nested Structs

Structs can contain other struct types as fields:
```neuro
struct Point {
    x: float,
    y: float,
}

struct Rectangle {
    top_left: Point,     // Nested struct (when struct types are fully supported)
    width: float,
    height: float,
}
```

## Complete Example

```neuro
// Basic struct declarations
struct Coordinate {
    x: int,
    y: int,
    z: int,
}

struct Entity {
    id: int,
    position: Coordinate,
    health: float,
    name: string,
    alive: bool,
}

fn main() -> int {
    // Struct instantiation and field access not yet fully implemented
    return 0;
}
```

## Current Implementation Status

### Fully Implemented ✅
- Struct declaration parsing
- Field name and type parsing
- Nested struct type references in field declarations
- Proper syntax validation for struct definitions

### Partially Implemented ⚠️
- Struct type integration with the type system
- Struct instantiation syntax
- Field access expressions (parsed but not semantically analyzed)

### Not Yet Implemented ❌
- Struct literal expressions: `Point { x: 1, y: 2 }`
- Field access via dot notation: `point.x`
- Struct methods and associated functions
- Struct pattern matching
- Default field values
- Struct inheritance or composition
- Struct visibility modifiers

## Planned Features

When fully implemented, structs will support:

### Instantiation
```neuro
// Planned syntax (not yet implemented)
let point = Point { x: 10, y: 20 };
let person = Person {
    age: 25,
    height: 5.8,
    name: "Alice",
    active: true,
};
```

### Field Access
```neuro
// Planned syntax (not yet implemented)
let x_coord = point.x;
let person_name = person.name;
```

### Methods
```neuro
// Planned syntax (not yet implemented)
impl Point {
    fn distance(self) -> float {
        return sqrt(self.x * self.x + self.y * self.y);
    }
}
```

## Notes

- Struct declarations are fully parsed and validated
- Type checking for struct fields works correctly
- Struct types can be referenced in function signatures
- Full struct functionality requires completion of the semantic analysis phase
- Current implementation focuses on syntax parsing rather than runtime behavior

