# NEURO Syntax Reference

This document provides the complete syntax reference for the NEURO programming language, with examples and implementation status.

## Table of Contents

1. [Lexical Structure](#lexical-structure)
2. [Expressions](#expressions)
3. [Statements](#statements)
4. [Declarations](#declarations)
5. [Types](#types)
6. [Patterns](#patterns)
7. [Attributes](#attributes)
8. [Macros](#macros)

---

## Lexical Structure

### Comments

```neuro
// Single-line comment

/* 
   Multi-line comment 
   Can span multiple lines
*/

/// Documentation comment
/// These comments document the following item
fn documented_function() -> int {
    return 42;
}

/**
 * Block documentation comment
 * Alternative documentation style
 */
struct DocumentedStruct {
    field: int
}
```

### Identifiers

```neuro
// Valid identifier patterns
identifier          // Simple identifier
snake_case          // Snake case (recommended)
camelCase           // Camel case
PascalCase          // Pascal case (for types)
SCREAMING_SNAKE     // Constants
_private            // Leading underscore indicates private/internal
__internal          // Double underscore for implementation details

// Unicode identifiers (supported)
δ                   // Greek letters
変数                 // Non-Latin scripts  
🚀                  // Emoji (discouraged in practice)

// Invalid identifiers
2invalid            // Cannot start with digit
my-var              // Hyphens not allowed
class               // Reserved keyword
```

### Keywords

```neuro
// Core language keywords
fn let mut if else while for loop break continue return match
struct enum trait impl type

// Module and visibility
import export pub priv mod crate super self

// Memory and ownership  
move copy ref mut unsafe

// Literals and constants
true false null

// AI/ML specific (✅ Implemented in parser)
tensor kernel grad model layer optimizer

// Advanced features (📅 Planned)
async await where const static

// Reserved for future use
abstract final virtual override yield
```

### Literals

#### Integer Literals ✅ IMPLEMENTED

```neuro
// Decimal integers
42                  // i32 (default)
42i8                // Explicit i8
42i16               // Explicit i16
42i32               // Explicit i32
42i64               // Explicit i64
42i128              // Explicit i128
42u8                // Unsigned u8
42u16               // Unsigned u16
42u32               // Unsigned u32  
42u64               // Unsigned u64
42u128              // Unsigned u128
42isize             // Platform-dependent signed
42usize             // Platform-dependent unsigned

// Binary literals
0b1010_1111         // Binary with underscore separators
0B0101_0000         // Uppercase B

// Octal literals  
0o755               // Octal permissions
0O644               // Uppercase O

// Hexadecimal literals
0xff                // Lowercase hex
0XFF                // Uppercase hex  
0x1a2b_3c4d         // With separators

// Underscores for readability
1_000_000           // One million
0xFF_FF_FF_FF       // Color value
```

#### Float Literals ✅ IMPLEMENTED

```neuro
// Decimal floats
3.14159             // f64 (default)
3.14f32             // Explicit f32
3.14f64             // Explicit f64
2.                  // 2.0
.5                  // 0.5
1_000.5             // With underscores

// Scientific notation  
1e6                 // 1,000,000
2.5e-3              // 0.0025
1.23E+4             // 12,300
6.022e23f64         // Avogadro's number as f64

// Special float values
f64::INFINITY       // Positive infinity
f64::NEG_INFINITY   // Negative infinity  
f64::NAN            // Not a number
```

#### String Literals ✅ IMPLEMENTED

```neuro
// Basic string literals
"Hello, world!"     // String literal
"Unicode: 🚀🦀"      // Unicode support
""                  // Empty string

// Escape sequences
"Line 1\nLine 2"    // Newline
"Tab\tSeparated"    // Tab
"Quote: \"Hello\""  // Escaped quote
"Backslash: \\"     // Escaped backslash
"Unicode: \u{1F680}" // Unicode escape (rocket emoji)

// Raw strings
r"No escapes: \n \t \"" // Raw string, no escape processing
r#"Can contain "quotes" without escaping"# // Raw string with # delimiters
r##"Can contain # and "# without issues"## // Multiple # delimiters

// Multiline strings
"This is a \
long string"        // Line continuation with \

"""
This is a multiline
string literal that
preserves newlines
"""

// String interpolation (🏗️ IN DEVELOPMENT)
let name = "NEURO";
let version = "1.0";
f"Welcome to {name} v{version}!" // Formatted string
f"Result: {2 + 2}"               // Expression interpolation
```

#### Character Literals ✅ IMPLEMENTED

```neuro
'a'                 // ASCII character
'🦀'                // Unicode character  
'\n'                // Newline escape
'\t'                // Tab escape
'\''                // Escaped single quote
'\\'                // Escaped backslash
'\u{41}'            // Unicode escape for 'A'
'\x41'              // Hex escape for 'A'
```

#### Boolean Literals ✅ IMPLEMENTED

```neuro
true                // Boolean true
false               // Boolean false
```

### Operators

#### Arithmetic Operators ✅ IMPLEMENTED

```neuro
+                   // Addition, unary plus
-                   // Subtraction, unary minus (negation)  
*                   // Multiplication
/                   // Division
%                   // Modulo/remainder

// Compound assignment
+=                  // Add and assign
-=                  // Subtract and assign
*=                  // Multiply and assign
/=                  // Divide and assign
%=                  // Modulo and assign
```

#### Comparison Operators ✅ IMPLEMENTED

```neuro
==                  // Equality
!=                  // Inequality
<                   // Less than
<=                  // Less than or equal
>                   // Greater than
>=                  // Greater than or equal
```

#### Logical Operators ✅ IMPLEMENTED

```neuro
&&                  // Logical AND (short-circuiting)
||                  // Logical OR (short-circuiting)
!                   // Logical NOT (unary)
```

#### Bitwise Operators (📅 Planned - Phase 2)

```neuro
&                   // Bitwise AND
|                   // Bitwise OR
^                   // Bitwise XOR
<<                  // Left shift
>>                  // Right shift
!                   // Bitwise NOT (unary)

// Compound assignment
&=                  // Bitwise AND and assign
|=                  // Bitwise OR and assign  
^=                  // Bitwise XOR and assign
<<=                 // Left shift and assign
>>=                 // Right shift and assign
```

#### Tensor Operators ✅ IMPLEMENTED (Parser)

```neuro
@                   // Matrix multiplication / tensor contraction
.*                  // Element-wise multiplication (Hadamard product)
.+                  // Element-wise addition
.-                  // Element-wise subtraction  
./                  // Element-wise division
.^                  // Element-wise power

// Broadcasting operators
.@                  // Broadcasting matrix multiplication
```

#### Other Operators

```neuro
=                   // Assignment
->                  // Function return type
=>                  // Match arm separator, closure syntax
.                   // Member access, method call
::                  // Path separator, associated function call
&                   // Reference (borrow)
&mut                // Mutable reference
*                   // Dereference
?                   // Error propagation (try operator)
..                  // Range (exclusive)
..=                 // Range (inclusive)
...                 // Variadic arguments
```

### Punctuation

```neuro
;                   // Statement terminator
,                   // Separator
:                   // Type annotation
()                  // Parentheses - grouping, function calls, tuples
{}                  // Braces - blocks, struct literals
[]                  // Brackets - arrays, indexing, attributes
#                   // Attribute prefix
@                   // Reserved for future use, tensor ops
$                   // Reserved for macros
```

---

## Expressions

### Literal Expressions ✅ IMPLEMENTED

```neuro
42                  // Integer literal
3.14                // Float literal  
true                // Boolean literal
'a'                 // Character literal
"hello"             // String literal
```

### Variable Expressions ✅ IMPLEMENTED

```neuro
variable_name       // Variable reference
self                // Self reference in methods
super               // Parent class reference (future)
```

### Path Expressions ✅ IMPLEMENTED

```neuro
module::function    // Absolute path
self::function      // Relative to current module
super::function     // Relative to parent module
crate::module::func // From crate root
std::collections::Vec // Standard library path
```

### Binary Expressions ✅ IMPLEMENTED

```neuro
a + b               // Addition
a - b               // Subtraction
a * b               // Multiplication
a / b               // Division
a % b               // Modulo
a == b              // Equality
a != b              // Inequality
a < b               // Less than
a <= b              // Less than or equal
a > b               // Greater than
a >= b              // Greater than or equal
a && b              // Logical AND
a || b              // Logical OR
a @ b               // Matrix multiplication
```

### Unary Expressions ✅ IMPLEMENTED

```neuro
-x                  // Negation
!x                  // Logical NOT
&x                  // Reference/borrow
&mut x              // Mutable reference  
*x                  // Dereference
```

### Function Call Expressions ✅ IMPLEMENTED

```neuro
function()          // No arguments
function(arg1)      // Single argument
function(arg1, arg2) // Multiple arguments
object.method()     // Method call
Type::function()    // Associated function call

// Named arguments (📅 Planned)
function(name: "value", count: 42)

// Variadic calls (📅 Planned)
sum(1, 2, 3, 4, 5)
```

### Array/Tensor Expressions ✅ IMPLEMENTED (Parser)

```neuro
[1, 2, 3, 4]        // Array literal
[0; 100]            // Array with repeated value
[]                  // Empty array

// Tensor literals
tensor![1, 2, 3, 4] // 1D tensor
tensor![
    [1, 2, 3],
    [4, 5, 6]
]                   // 2D tensor

// Multi-dimensional tensor literals
tensor![[[1, 2], [3, 4]], [[5, 6], [7, 8]]] // 3D tensor
```

### Index Expressions ✅ IMPLEMENTED (Parser)

```neuro
array[0]            // Single index
matrix[i][j]        // Nested indexing
matrix[i, j]        // Multi-dimensional indexing (tensors)
array[1..4]         // Slice (exclusive end)
array[1..=4]        // Slice (inclusive end)  
array[..4]          // Slice from start
array[1..]          // Slice to end
array[..]           // Full slice

// Advanced tensor indexing
tensor[0, :, 1..3]  // Mixed indexing and slicing
tensor[mask]        // Boolean mask indexing (planned)
```

### Struct Expressions ✅ IMPLEMENTED (Parser)

```neuro
// Struct literal
Point { x: 1.0, y: 2.0 }

// Struct update syntax
Point { x: 3.0, ..old_point }

// Tuple struct
Color(255, 0, 0)

// Unit struct
Marker
```

### Tuple Expressions ✅ IMPLEMENTED

```neuro
()                  // Unit tuple (empty tuple)
(42,)               // Single-element tuple (note the comma)
(1, 2)              // Two-element tuple
(1, 2, 3)           // Three-element tuple
(name, age, active) // Mixed types
```

### Block Expressions ✅ IMPLEMENTED

```neuro
{
    let x = 42;
    let y = 2;
    x + y               // Last expression is the value
}

// Block with explicit return
{
    let result = compute();
    return result;
}
```

### Control Flow Expressions ✅ IMPLEMENTED

#### If Expressions

```neuro
// Basic if expression
if condition {
    42
} else {
    0
}

// If-else if chain
if x < 0 {
    "negative"
} else if x == 0 {
    "zero"  
} else {
    "positive"
}

// If let expression
if let Some(value) = optional {
    value * 2
} else {
    0
}
```

#### Match Expressions ✅ IMPLEMENTED (Parser)

```neuro
// Basic match
match value {
    1 => "one",
    2 => "two",
    _ => "other"
}

// Pattern matching with destructuring
match point {
    Point { x: 0, y: 0 } => "origin",
    Point { x, y: 0 } => f"on x-axis at {x}",
    Point { x: 0, y } => f"on y-axis at {y}", 
    Point { x, y } => f"point at ({x}, {y})"
}

// Match with guards
match number {
    n if n < 0 => "negative",
    0 => "zero",
    n if n > 100 => "large",
    n => f"small positive: {n}"
}
```

### Loop Expressions ✅ IMPLEMENTED (Parser)

#### While Loops

```neuro
while condition {
    // Loop body
}

// While let pattern
while let Some(item) = iterator.next() {
    process(item);
}
```

#### For Loops

```neuro
// Range-based for loop
for i in 0..10 {
    print(i);
}

// Iterator-based for loop  
for item in collection {
    process(item);
}

// Reference iteration
for item in &collection {
    print(*item);
}

// Mutable reference iteration
for item in &mut collection {
    *item *= 2;
}

// Enumerate
for (index, item) in collection.iter().enumerate() {
    print(f"{index}: {item}");
}
```

#### Infinite Loops

```neuro
loop {
    if break_condition {
        break;
    }
}

// Loop with break value
let result = loop {
    let input = get_input();
    if input == "quit" {
        break "goodbye";
    }
};
```

### Closure Expressions (📅 Planned - Phase 2)

```neuro
// Simple closure
|x| x * 2

// Closure with type annotations
|x: i32| -> i32 { x * 2 }

// Multi-parameter closure
|a, b| a + b

// Closure with block body
|x| {
    let result = expensive_computation(x);
    result * 2
}

// Capturing environment
let multiplier = 10;
let closure = |x| x * multiplier;

// Move closure
let closure = move |x| x * multiplier;
```

### Async Expressions (📅 Planned - Phase 3)

```neuro
// Async block
async {
    let data = fetch_data().await;
    process(data)
}

// Async closure  
async |x| {
    let result = async_operation(x).await;
    result * 2
}
```

---

## Statements

### Expression Statements ✅ IMPLEMENTED

```neuro
42;                 // Expression followed by semicolon
function_call();    // Function call statement
x + y;              // Arithmetic expression (result discarded)
```

### Let Statements ✅ IMPLEMENTED

```neuro
// Basic let binding
let x = 42;
let name = "NEURO";

// With explicit type
let x: i32 = 42;
let name: String = "NEURO".to_string();

// Mutable binding
let mut counter = 0;
let mut buffer: Vec<u8> = Vec::new();

// Pattern destructuring
let (a, b) = (1, 2);
let Point { x, y } = point;
let [first, second, rest @ ..] = array;

// Multiple patterns
let (mut x, y) = (10, 20);  // x is mutable, y is not

// Let with else (📅 Planned)
let Some(value) = optional else {
    return;  // Early return if pattern doesn't match
};
```

### Assignment Statements ✅ IMPLEMENTED

```neuro
// Simple assignment  
x = 42;
name = "new_name".to_string();

// Compound assignment
counter += 1;
buffer *= 2;
total -= cost;
quotient /= divisor;
remainder %= modulo;

// Multiple assignment (📅 Planned)
(a, b) = (b, a);    // Swap values
```

### Return Statements ✅ IMPLEMENTED

```neuro
return;             // Return unit type ()
return 42;          // Return value
return Some(value); // Return Option

// Early return with error propagation
let value = function_call()?;
```

### Break and Continue Statements ✅ IMPLEMENTED

```neuro
// In loops
while condition {
    if skip_condition {
        continue;   // Skip to next iteration
    }
    if exit_condition {
        break;      // Exit loop
    }
}

// Break with value
let result = loop {
    let input = get_input();
    if input == "quit" {
        break input;  // Break with value
    }
};

// Labeled break/continue
'outer: for i in 0..10 {
    'inner: for j in 0..10 {
        if i * j > 50 {
            break 'outer;    // Break outer loop
        }
        if i == j {
            continue 'inner; // Continue inner loop
        }
    }
}
```

---

## Declarations

### Function Declarations ✅ IMPLEMENTED

```neuro
// Basic function
fn add(a: i32, b: i32) -> i32 {
    a + b
}

// Function with no return value (returns unit type)
fn print_hello() {
    print("Hello!");
}

// Function with explicit unit return
fn explicit_unit() -> () {
    print("Explicit unit");
}

// Generic function
fn identity<T>(value: T) -> T {
    value
}

// Function with lifetime parameters
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Function with where clause
fn complex<T, U>(a: T, b: U) -> bool
where
    T: Display + Clone,
    U: Debug + PartialEq<T>
{
    print(a.clone());
    b == a
}

// Async function (📅 Planned)
async fn fetch_data(url: &str) -> Result<String, Error> {
    let response = http_get(url).await?;
    Ok(response.text().await?)
}
```

### Variable Declarations ✅ IMPLEMENTED

```neuro
// Immutable variables
let x = 42;
let name: String = "NEURO".to_string();

// Mutable variables  
let mut counter = 0;
let mut buffer = Vec::new();

// Constants
const MAX_SIZE: usize = 1000;
const PI: f64 = 3.141592653589793;

// Static variables
static GLOBAL_COUNTER: AtomicUsize = AtomicUsize::new(0);
static mut GLOBAL_STATE: Option<State> = None;
```

### Type Declarations

#### Struct Declarations ✅ IMPLEMENTED (Parser)

```neuro
// Basic struct
struct Point {
    x: f64,
    y: f64
}

// Generic struct
struct Container<T> {
    value: T
}

// Struct with lifetime parameters
struct StringSlice<'a> {
    data: &'a str
}

// Tuple struct
struct Color(u8, u8, u8);
struct Meters(f64);

// Unit struct
struct Marker;

// Struct with visibility modifiers
pub struct PublicStruct {
    pub public_field: i32,
    private_field: String  // private by default
}
```

#### Enum Declarations ✅ IMPLEMENTED (Parser)

```neuro
// Basic enum
enum Direction {
    North,
    South,
    East,
    West
}

// Enum with associated data
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(u8, u8, u8)
}

// Generic enum
enum Option<T> {
    Some(T),
    None
}

enum Result<T, E> {
    Ok(T),
    Err(E)
}

// Enum with discriminants
enum Priority {
    Low = 1,
    Medium = 5,
    High = 10
}
```

#### Trait Declarations (📅 Planned - Phase 2)

```neuro
// Basic trait
trait Display {
    fn fmt(&self) -> String;
}

// Trait with associated types
trait Iterator {
    type Item;
    
    fn next(&mut self) -> Option<Self::Item>;
}

// Trait with generic parameters
trait Add<Rhs = Self> {
    type Output;
    
    fn add(self, rhs: Rhs) -> Self::Output;
}

// Trait with default implementations
trait Greet {
    fn name(&self) -> &str;
    
    fn greet(&self) -> String {
        f"Hello, {self.name()}!"  // Default implementation
    }
}
```

#### Type Aliases ✅ IMPLEMENTED (Parser)

```neuro
// Simple type alias
type Kilometers = f64;
type UserId = u64;

// Generic type alias
type Result<T> = std::result::Result<T, Error>;
type HashMap<K, V> = std::collections::HashMap<K, V>;

// Complex type alias
type EventHandler<T> = fn(&T) -> bool;
type AsyncResult<T> = Pin<Box<dyn Future<Output = Result<T>>>>;
```

### Implementation Declarations ✅ IMPLEMENTED (Parser)

```neuro
// Inherent implementation
impl Point {
    fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    
    fn distance(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    
    fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

// Generic implementation
impl<T> Container<T> {
    fn new(value: T) -> Container<T> {
        Container { value }
    }
    
    fn get(&self) -> &T {
        &self.value
    }
}

// Trait implementation (📅 Planned)
impl Display for Point {
    fn fmt(&self) -> String {
        f"Point({self.x}, {self.y})"
    }
}

// Conditional trait implementation
impl<T: Display> Display for Container<T> {
    fn fmt(&self) -> String {
        f"Container({})", self.value.fmt()
    }
}
```

### Module Declarations ✅ IMPLEMENTED

```neuro
// Inline module
mod utils {
    pub fn helper_function() -> i32 {
        42
    }
    
    // Nested module
    mod internal {
        fn private_function() {
            // Only accessible within this module
        }
    }
}

// External module (loads from file)
mod math;           // Loads math.nr or math/mod.nr
mod neural_networks; // Loads neural_networks.nr or neural_networks/mod.nr

// Conditional modules
#[cfg(target_os = "linux")]
mod linux_specific;

#[cfg(feature = "gpu")]
mod gpu_kernels;
```

---

## Types

### Primitive Types ✅ IMPLEMENTED

```neuro
// Integer types
i8, i16, i32, i64, i128, isize    // Signed integers
u8, u16, u32, u64, u128, usize    // Unsigned integers

// Floating-point types
f32, f64                          // IEEE 754 floating point

// Boolean type
bool                              // true or false

// Character type
char                              // Unicode scalar value

// Unit type
()                                // Empty tuple, represents "no value"
```

### String Types ✅ IMPLEMENTED

```neuro
str                   // String slice (unsized)
&str                  // Reference to string slice
String                // Owned, growable string
```

### Compound Types

#### Array Types ✅ IMPLEMENTED

```neuro
[T; N]                // Fixed-size array of type T with N elements
[i32; 5]              // Array of 5 integers
[f64; 100]            // Array of 100 floats
[bool; 8]             // Array of 8 booleans
```

#### Slice Types ✅ IMPLEMENTED

```neuro
&[T]                  // Shared slice of type T
&mut [T]              // Mutable slice of type T
&[i32]                // Slice of integers
&mut [f64]            // Mutable slice of floats
```

#### Tuple Types ✅ IMPLEMENTED

```neuro
()                    // Unit tuple
(T,)                  // Single-element tuple
(T, U)                // Two-element tuple
(i32, f64, bool)      // Three-element tuple with mixed types
(String, Vec<i32>)    // Complex tuple types
```

#### Reference Types ✅ IMPLEMENTED

```neuro
&T                    // Shared/immutable reference
&mut T                // Mutable reference
&i32                  // Reference to integer
&mut String           // Mutable reference to string
&[u8]                 // Reference to byte slice
```

### Collection Types ✅ IMPLEMENTED

```neuro
Vec<T>                // Growable array/vector
HashMap<K, V>         // Hash map/dictionary
HashSet<T>            // Hash set
BTreeMap<K, V>        // Ordered map
BTreeSet<T>           // Ordered set
LinkedList<T>         // Doubly-linked list
VecDeque<T>           // Double-ended queue
```

### Function Types ✅ IMPLEMENTED (Parser)

```neuro
fn()                  // Function with no parameters, returns unit
fn() -> i32           // Function returning i32
fn(i32) -> i32        // Function taking i32, returning i32  
fn(i32, f64) -> bool  // Function with multiple parameters

// Function pointer types
fn(T) -> U            // Generic function pointer
fn(&str) -> String    // Function taking string slice, returning String
```

### Generic Types ✅ IMPLEMENTED (Parser)

```neuro
Option<T>             // Generic option type
Result<T, E>          // Generic result type  
Vec<T>                // Generic vector
HashMap<K, V>         // Generic hash map

// Generic with bounds
T: Clone              // T must implement Clone
T: Display + Debug    // T must implement both Display and Debug
T: 'static            // T must have 'static lifetime
```

### Tensor Types ✅ IMPLEMENTED (Parser)

```neuro
// Static tensor types (compile-time shape)
Tensor<f32, [784]>          // 1D tensor (vector) of 784 f32 elements
Tensor<f64, [28, 28]>       // 2D tensor (matrix) of 28×28 f64 elements  
Tensor<i32, [32, 3, 224, 224]> // 4D tensor (batch of images)

// Dynamic tensor types (runtime shape)
DynamicTensor<f32>          // Tensor with runtime-determined shape
SparseTensor<f64>           // Sparse tensor representation

// Tensor with specific memory layout
Tensor<f32, [M, N], RowMajor>    // Row-major layout
Tensor<f32, [M, N], ColumnMajor> // Column-major layout
```

### Advanced Types (📅 Planned - Phase 2)

#### Lifetime Types

```neuro
&'a T                 // Reference with lifetime 'a
&'static str          // Reference with static lifetime
fn<'a>(&'a str) -> &'a str // Function with lifetime parameters
```

#### Associated Types  

```neuro
<T as Iterator>::Item      // Associated type from trait
Self::Output               // Associated type in trait definition
```

#### Higher-Ranked Types

```neuro
for<'a> fn(&'a str) -> &'a str  // Higher-ranked trait bound
fn(for<'a> fn(&'a str))         // Higher-ranked function parameter
```

#### Existential Types

```neuro
impl Trait                 // Existential type (opaque return type)
Box<dyn Trait>            // Dynamic trait object
&dyn Trait                // Reference to trait object
```

---

## Patterns

### Literal Patterns ✅ IMPLEMENTED

```neuro
match x {
    42 => "answer",           // Integer literal
    3.14 => "pi",             // Float literal  
    true => "yes",            // Boolean literal
    'a' => "letter a",        // Character literal
    "hello" => "greeting",    // String literal
    _ => "other"              // Wildcard pattern
}
```

### Variable Patterns ✅ IMPLEMENTED

```neuro
match value {
    x => x * 2,               // Bind to variable x
    _ => 0                    // Wildcard (ignore value)
}

// Mutable pattern
match mut_value {
    mut x => {                // Bind to mutable variable
        x += 1;
        x
    }
}
```

### Reference Patterns ✅ IMPLEMENTED

```neuro
match &value {
    &42 => "reference to 42",
    ref x => f"reference to {x}", // Create reference
    _ => "other"
}

match &mut value {
    &mut 42 => "mutable reference to 42",
    ref mut x => {            // Create mutable reference
        *x += 1;
        "modified"
    }
}
```

### Struct Patterns ✅ IMPLEMENTED (Parser)

```neuro
match point {
    Point { x: 0, y: 0 } => "origin",
    Point { x: 0, y } => f"on y-axis at {y}",
    Point { x, y: 0 } => f"on x-axis at {x}", 
    Point { x, y } => f"point at ({x}, {y})",
    Point { x, .. } => f"x coordinate is {x}"  // Ignore other fields
}
```

### Tuple Patterns ✅ IMPLEMENTED (Parser)

```neuro
match tuple {
    () => "unit tuple",
    (x,) => f"single element: {x}",
    (x, y) => f"pair: ({x}, {y})",
    (x, y, z) => f"triple: ({x}, {y}, {z})",
    (first, .., last) => f"first: {first}, last: {last}"
}
```

### Array/Slice Patterns ✅ IMPLEMENTED (Parser)

```neuro
match slice {
    [] => "empty",
    [x] => f"single: {x}",
    [x, y] => f"pair: {x}, {y}",
    [first, rest @ ..] => f"first: {first}, rest: {rest:?}",
    [.., last] => f"last: {last}",
    [first, .., last] => f"first: {first}, last: {last}"
}
```

### Enum Patterns ✅ IMPLEMENTED (Parser)

```neuro
match message {
    Message::Quit => "quitting",
    Message::Move { x, y } => f"moving to ({x}, {y})",
    Message::Write(text) => f"writing: {text}",
    Message::ChangeColor(r, g, b) => f"color: rgb({r}, {g}, {b})"
}

match option {
    Some(x) => f"value: {x}",
    None => "no value"
}
```

### Range Patterns ✅ IMPLEMENTED (Parser)

```neuro
match age {
    0..=12 => "child",        // Inclusive range
    13..18 => "teenager",     // Exclusive range
    18..=65 => "adult",
    66.. => "senior",         // Open-ended range
    _ => "invalid age"
}
```

### Pattern Guards ✅ IMPLEMENTED (Parser)

```neuro
match x {
    n if n < 0 => "negative",
    n if n == 0 => "zero", 
    n if n > 100 => "large",
    n => f"small positive: {n}"
}

match point {
    Point { x, y } if x == y => "diagonal",
    Point { x, y } if x + y == 0 => "anti-diagonal",
    Point { x, y } => f"general point: ({x}, {y})"
}
```

### Or Patterns ✅ IMPLEMENTED (Parser)

```neuro
match character {
    'a' | 'e' | 'i' | 'o' | 'u' => "vowel",
    'y' => "sometimes vowel",
    _ => "consonant"
}

match number {
    1 | 2 | 3 | 5 | 7 | 11 | 13 => "small prime",
    17 | 19 | 23 => "larger prime",
    _ => "not a small prime"
}
```

### Tensor Pattern Matching ✅ IMPLEMENTED (Parser) - ML-Optimized

```neuro
// Pattern matching on tensor shapes
match tensor.shape() {
    [] => "scalar",                           // 0D tensor
    [n] => f"vector of length {n}",          // 1D tensor
    [m, n] => f"matrix {m}×{n}",             // 2D tensor
    [batch, features] => f"batch data",      // Batch of feature vectors
    [batch, h, w, c] => f"image batch",      // Image data
    [batch, seq, features] => f"sequences",  // Sequence data
    _ => "high-dimensional tensor"
}

// Pattern matching on tensor values (future)
match tensor {
    tensor![0, 0, 0] => "zero vector",
    tensor![1, 0, 0] => "unit x",
    tensor![0, 1, 0] => "unit y", 
    tensor![0, 0, 1] => "unit z",
    _ => "general vector"
}
```

---

## Attributes

Attributes provide metadata and compiler directives in NEURO.

### Syntax ✅ IMPLEMENTED (Parser)

```neuro
#[attribute_name]                    // Simple attribute
#[attribute_name(arg)]               // Attribute with single argument
#[attribute_name(arg1, arg2)]        // Attribute with multiple arguments
#[attribute_name = "value"]          // Attribute with assignment
#[path::to::attribute]               // Namespaced attribute

// Multiple attributes
#[attr1]
#[attr2]
#[attr3(param)]
fn function() {}

// Inline multiple attributes  
#[attr1, attr2, attr3]
fn function() {}
```

### Core Attributes ✅ IMPLEMENTED

#### Conditional Compilation

```neuro
#[cfg(target_os = "linux")]
fn linux_specific() {}

#[cfg(target_os = "windows")]
fn windows_specific() {}

#[cfg(feature = "gpu")]
mod gpu_kernels;

#[cfg(debug_assertions)]
fn debug_only() {}

#[cfg(not(test))]
fn non_test_code() {}

#[cfg(all(unix, target_pointer_width = "64"))]
fn unix_64bit() {}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn x86_code() {}
```

#### Documentation

```neuro
#[doc = "This function adds two numbers"]
fn add(a: i32, b: i32) -> i32 { a + b }

#[doc(hidden)]              // Hide from documentation
fn internal_function() {}

#[doc(inline)]              // Inline documentation from re-export
pub use other_mod::function;
```

#### Deprecation

```neuro
#[deprecated]
fn old_function() {}

#[deprecated = "Use new_function instead"]
fn old_function_with_message() {}

#[deprecated(since = "1.2.0", note = "Use better_function instead")]
fn specific_deprecation() {}
```

#### Compiler Hints

```neuro
#[inline]                   // Suggest inlining
fn small_function() -> i32 { 42 }

#[inline(always)]           // Force inlining
fn must_inline() -> i32 { 1 }

#[inline(never)]            // Prevent inlining
fn no_inline() {}

#[cold]                     // Mark as rarely executed
fn error_path() {}

#[hot]                      // Mark as frequently executed  
fn main_loop() {}
```

### ML/AI Attributes ✅ IMPLEMENTED (Parser) / 🏗️ IN DEVELOPMENT

#### Automatic Differentiation

```neuro
#[grad]                     // Enable automatic differentiation
fn neural_layer(x: Tensor<f32>) -> Tensor<f32> {
    // Gradients automatically computed
    activate(x @ weights + bias)
}

#[grad(retain_graph)]       // Retain computation graph
fn complex_loss(pred: Tensor<f32>, target: Tensor<f32>) -> Tensor<f32> {
    mse_loss(pred, target) + l2_regularization(pred)
}

#[no_grad]                  // Disable gradient computation
fn inference(x: Tensor<f32>) -> Tensor<f32> {
    // No gradients computed - faster inference
    model.forward(x)
}
```

#### GPU Kernels

```neuro
#[kernel(gpu = "cuda")]     // CUDA kernel
fn cuda_multiply(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
    a * b
}

#[kernel(gpu = "vulkan")]   // Vulkan compute shader
fn vulkan_convolution(input: &Tensor<f32>, kernel: &Tensor<f32>) -> Tensor<f32> {
    conv2d(input, kernel)
}

#[kernel(gpu = "cuda,vulkan")] // Multi-backend kernel
fn universal_kernel(data: &Tensor<f32>) -> Tensor<f32> {
    data.relu()
}

// Advanced GPU kernel configuration
#[kernel(gpu = "cuda")]
#[launch_config(blocks = [32, 32], threads = [16, 16])]
#[shared_memory(size = 4096)]
fn advanced_cuda_kernel(input: &Tensor<f32>) -> Tensor<f32> {
    // CUDA-specific optimizations
    optimized_computation(input)
}
```

#### Model Definition

```neuro
#[model]                    // Mark as neural network model
struct NeuralNet {
    layer1: Dense<784, 256>,
    layer2: Dense<256, 128>,
    output: Dense<128, 10>
}

#[layer]                    // Mark as neural network layer
struct CustomLayer {
    weights: Tensor<f32, [INPUT_SIZE, OUTPUT_SIZE]>,
    bias: Tensor<f32, [OUTPUT_SIZE]>
}

#[optimizer]                // Mark as optimizer
struct CustomOptimizer {
    learning_rate: f32,
    momentum: f32
}
```

#### Memory Management

```neuro
#[memory(pool = "tensor_pool")] // Use specific memory pool
fn tensor_operation() -> Tensor<f32> {
    Tensor::zeros([1024, 1024])
}

#[memory(alignment = 32)]   // Specify memory alignment
fn simd_optimized() -> Vec<f32> {
    vec![0.0; 1000]
}

#[no_gc]                    // Disable garbage collection
fn performance_critical() {
    // GC-free execution
}
```

#### Performance Optimization

```neuro
#[optimize(level = 3)]      // Optimization level
fn performance_critical() {}

#[optimize(target = "avx2")] // Target-specific optimization
fn simd_code() {}

#[vectorize]                // Enable auto-vectorization
fn vectorizable_loop(data: &[f32]) -> Vec<f32> {
    data.iter().map(|x| x * x).collect()
}

#[parallel]                 // Enable automatic parallelization
fn parallel_computation(data: &[Tensor<f32>]) -> Vec<Tensor<f32>> {
    data.iter().map(|t| t.relu()).collect()
}
```

### Test and Benchmark Attributes

```neuro
#[test]                     // Unit test
fn test_addition() {
    assert_eq!(add(2, 3), 5);
}

#[bench]                    // Benchmark
fn bench_sorting(b: &mut Bencher) {
    let mut data = vec![3, 1, 4, 1, 5, 9, 2, 6];
    b.iter(|| data.sort());
}

#[ignore]                   // Ignore test by default
#[test]
fn expensive_test() {
    // Only run with --ignored flag
}

#[should_panic]             // Test should panic
#[test]
fn test_panic() {
    panic!("This should panic");
}

#[should_panic(expected = "divide by zero")]
#[test]
fn test_specific_panic() {
    divide_by_zero();
}
```

### Custom Attributes (📅 Planned - Phase 2)

```neuro
// Define custom attribute
#[attribute]
struct benchmark {
    iterations: usize = 1000,
    warmup: usize = 100
}

// Use custom attribute  
#[benchmark(iterations = 10000)]
fn sort_algorithm() {
    // Implementation
}

// Procedural macro attribute
#[derive(Clone, Debug, Serialize)]
struct DataPoint {
    x: f64,
    y: f64
}

// Custom derive for ML types
#[derive(Tensor, Trainable)]
struct ModelWeights {
    dense1: Tensor<f32, [784, 256]>,
    dense2: Tensor<f32, [256, 10]>
}
```

---

## Macros

NEURO supports both declarative macros (macro_rules!) and procedural macros.

### Macro Invocation ✅ IMPLEMENTED (Parser)

```neuro
// Function-like macros
println!("Hello, world!");
vec![1, 2, 3, 4]
assert_eq!(a, b);
format!("Value: {}", x)

// Attribute-like macros
#[derive(Debug, Clone)]
struct Point { x: i32, y: i32 }

// Derive macros
#[derive(PartialEq, Eq, Hash)]
enum Color { Red, Green, Blue }
```

### Built-in Macros ✅ IMPLEMENTED

```neuro
// Assertion macros
assert!(condition);
assert_eq!(left, right);
assert_ne!(left, right);
debug_assert!(expensive_check());

// Printing macros  
print!("Hello");
println!("Hello, world!");
eprint!("Error: ");
eprintln!("Error message");

// Formatting
format!("Hello, {}", name);
write!(buffer, "Formatted: {}", value);
writeln!(buffer, "Line: {}", line);

// Compiler intrinsics
unreachable!();             // Mark unreachable code
unimplemented!();           // Mark unimplemented code  
todo!();                    // Mark TODO items
panic!("Error message");    // Panic with message

// Type information
size_of::<T>();             // Size of type T
align_of::<T>();            // Alignment of type T
type_name::<T>();           // Name of type T

// Environment
env!("CARGO_PKG_VERSION");  // Environment variable at compile time
option_env!("HOME");        // Optional environment variable
include_str!("file.txt");   // Include file as string literal
include_bytes!("data.bin"); // Include file as byte array
```

### Tensor Macros ✅ IMPLEMENTED (Parser)

```neuro
// Tensor creation macros
tensor![1, 2, 3, 4];        // 1D tensor
tensor![                    // 2D tensor
    [1, 2, 3],
    [4, 5, 6]
];

// Specialized tensor macros
zeros!(shape);              // Zero tensor
ones!(shape);               // Ones tensor  
eye!(n);                    // Identity matrix
random!(shape);             // Random tensor
linspace!(start, end, steps); // Linear space

// Tensor operation macros
broadcast!(tensor, shape);   // Broadcasting
reshape!(tensor, new_shape); // Reshaping
```

### Declarative Macros (📅 Planned - Phase 2)

```neuro
// Basic macro_rules!
macro_rules! say_hello {
    () => {
        println!("Hello!");
    };
}

// Macro with parameters
macro_rules! create_function {
    ($func_name:ident) => {
        fn $func_name() {
            println!("You called {:?}()", stringify!($func_name));
        }
    };
}

// Complex macro with repetition
macro_rules! hashmap {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            let mut map = HashMap::new();
            $(
                map.insert($key, $value);
            )*
            map
        }
    };
}

// Usage
create_function!(foo);
let map = hashmap! {
    "key1" => "value1",
    "key2" => "value2"
};
```

### Procedural Macros (📅 Planned - Phase 2)

```neuro
// Function-like procedural macro
sql! {
    SELECT id, name FROM users WHERE age > 18
}

// Attribute procedural macro
#[route(GET, "/users/{id}")]
fn get_user(id: u32) -> User {
    // Implementation
}

// Derive procedural macro
#[derive(Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String
}
```

### ML-Specific Macros (🏗️ IN DEVELOPMENT)

```neuro
// Neural network architecture macro
model! {
    Sequential [
        Dense(784, 256),
        ReLU,
        Dense(256, 128),
        ReLU, 
        Dense(128, 10),
        Softmax
    ]
}

// Training loop macro
train! {
    model: my_model,
    optimizer: Adam(lr = 0.001),
    loss: CrossEntropyLoss,
    epochs: 100,
    batch_size: 32
}

// GPU kernel macro
cuda_kernel! {
    fn matrix_add(a: &[f32], b: &[f32], c: &mut [f32], n: usize) {
        let idx = blockIdx.x * blockDim.x + threadIdx.x;
        if idx < n {
            c[idx] = a[idx] + b[idx];
        }
    }
}

// Automatic differentiation macro
autodiff! {
    fn loss_function(pred: Tensor<f32>, target: Tensor<f32>) -> Tensor<f32> {
        (pred - target).pow(2).mean()
    }
}
```

---

This completes the comprehensive syntax reference for NEURO, covering all implemented and planned language features with clear status indicators and practical examples.