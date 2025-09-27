//! Example: Basic Struct Declarations
//!
//! Purpose: Demonstrates struct syntax and basic struct usage patterns
//!
//! Concepts covered:
//! - Struct declaration syntax
//! - Field definitions with types
//! - Different field types in structs
//! - Nested struct type references
//! - Current implementation limitations
//!
//! Expected output: 0 (demonstrates syntax, not runtime behavior)
//! Compilation: neurc run examples/05_data_structures/01_struct_basics.nr

// Simple struct with basic fields
struct Point {
    x: int,
    y: int,
}

// Struct with mixed field types
struct Person {
    age: int,
    height: float,
    name: string,
    active: bool,
}

// Struct with many fields
struct GameCharacter {
    id: int,
    name: string,
    level: int,
    health: int,
    max_health: int,
    experience: int,
    alive: bool,
    x_position: int,
    y_position: int,
}

// Struct that references other struct types
struct Rectangle {
    top_left: Point,      // Reference to Point struct
    width: int,
    height: int,
}

// Struct for data organization
struct DatabaseRecord {
    record_id: int,
    created_timestamp: int,
    updated_timestamp: int,
    data_string: string,
    is_active: bool,
    priority: int,
}

// Struct demonstrating different naming patterns
struct NetworkConnection {
    connection_id: int,
    server_address: string,
    port_number: int,
    is_secure: bool,
    timeout_seconds: int,
    bytes_sent: int,
    bytes_received: int,
}

// Simple struct for mathematical operations
struct Vector2D {
    x: float,
    y: float,
}

// Struct representing a complex data type
struct StudentRecord {
    student_id: int,
    first_name: string,
    last_name: string,
    grade_level: int,
    gpa: float,
    enrolled: bool,
    credits_completed: int,
}

// Function that would work with structs (when fully implemented)
fn demonstrate_struct_usage() -> int {
    // Note: Struct instantiation is not yet fully implemented
    // This function demonstrates what the syntax would look like

    // Future syntax (not yet working):
    // let point = Point { x: 10, y: 20 };
    // let person = Person { age: 25, height: 5.8, name: "Alice", active: true };

    // For now, we return 0 to indicate the example compiles
    return 0;
}

// Function showing struct type usage in function signatures
fn point_functions() -> int {
    // When struct field access is implemented, this would take a Point parameter
    // For now, simplified since struct parameters aren't fully working

    // Return a placeholder
    return 42;
}

// Function that would return a struct (when implemented)
fn create_default_person() -> int {
    // Future implementation would create and return a Person instance
    // For now, we return an int since struct instantiation isn't implemented
    return 0;
}

// Function demonstrating struct as parameter and return type
fn process_rectangle() -> int {
    // Future implementation would take Rectangle parameter and return Rectangle
    // For now, simplified to demonstrate struct type parsing
    return 1;
}

fn main() -> int {
    // Demonstrate that struct declarations parse correctly
    let usage_result = demonstrate_struct_usage();

    // Show that struct types can be used in function signatures
    let point_result = point_functions();

    print(usage_result);   // Output: 0
    print(point_result);   // Output: 42

    // Return success
    return 0;
}