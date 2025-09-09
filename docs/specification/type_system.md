# NEURO Type System Specification v1.0

## Overview

NEURO features a static type system with type inference, designed specifically for AI/ML workloads. The type system ensures memory safety while enabling zero-cost abstractions for tensor operations and GPU computation.

## Core Principles

1. **Static Typing**: All types are resolved at compile time
2. **Type Inference**: Extensive type inference reduces boilerplate 
3. **Memory Safety**: No null pointers or memory leaks through ARC
4. **Zero-Cost Abstractions**: High-level types compile to efficient code
5. **ML-First Design**: Tensors and neural network types are first-class

## Primitive Types

### Integer Types

| Type | Size | Range |
|------|------|-------|
| `i8` | 8-bit | -128 to 127 |
| `i16` | 16-bit | -32,768 to 32,767 |
| `i32` | 32-bit | -2,147,483,648 to 2,147,483,647 |
| `i64` | 64-bit | -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807 |
| `u8` | 8-bit | 0 to 255 |
| `u16` | 16-bit | 0 to 65,535 |
| `u32` | 32-bit | 0 to 4,294,967,295 |
| `u64` | 64-bit | 0 to 18,446,744,073,709,551,615 |

### Floating-Point Types

| Type | Size | Precision |
|------|------|-----------|
| `f32` | 32-bit | IEEE 754 single precision |
| `f64` | 64-bit | IEEE 754 double precision |

### Other Primitive Types

- `bool`: Boolean values (`true` or `false`)
- `char`: Unicode scalar values (32-bit)
- `str`: String slices (UTF-8 encoded)

## Compound Types

### Arrays

Fixed-size sequences of elements:

```neuro
let arr: [i32; 5] = [1, 2, 3, 4, 5];
let dynamic: [f32] = vec![1.0, 2.0, 3.0]; // Dynamic array
```

### Tuples

Fixed-size ordered lists of heterogeneous elements:

```neuro
let point: (f32, f32) = (3.0, 4.0);
let record: (str, i32, bool) = ("John", 30, true);
```

### Structures

Custom types with named fields:

```neuro
struct Point {
    x: f32,
    y: f32,
}

struct Person {
    name: str,
    age: i32,
    active: bool,
}
```

### Enumerations

Sum types with named variants:

```neuro
enum Result<T, E> {
    Ok(T),
    Err(E),
}

enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(str),
    ChangeColor(i32, i32, i32),
}
```

## Tensor Types

NEURO's first-class tensor types with compile-time shape verification:

### Basic Tensor Type

```neuro
Tensor<T, [D1, D2, ..., DN]>
```

Where:
- `T` is the element type (f32, f64, i32, etc.)
- `[D1, D2, ..., DN]` is the compile-time shape specification

### Examples

```neuro
// 2D matrix: 3 rows, 4 columns
let matrix: Tensor<f32, [3, 4]> = tensor!([[1.0, 2.0, 3.0, 4.0],
                                           [5.0, 6.0, 7.0, 8.0], 
                                           [9.0, 10.0, 11.0, 12.0]]);

// 3D tensor: batch size 10, height 28, width 28 
let images: Tensor<u8, [10, 28, 28]> = load_mnist_batch();

// Dynamic dimensions (runtime-determined size)
let dynamic: Tensor<f32, [_, _]> = create_matrix(rows, cols);

// Vector (1D tensor)
let vector: Tensor<f64, [100]> = zeros();
```

### Shape Operations

```neuro
// Shape inference
let a: Tensor<f32, [3, 4]> = matrix_a();
let b: Tensor<f32, [4, 5]> = matrix_b();
let c = a @ b; // Type: Tensor<f32, [3, 5]>

// Broadcasting
let x: Tensor<f32, [3, 1]> = column_vector();
let y: Tensor<f32, [3, 4]> = matrix();
let z = x + y; // Broadcasts to Tensor<f32, [3, 4]>
```

## Generic Types

### Type Parameters

```neuro
struct Container<T> {
    value: T,
}

fn process<T>(input: T) -> T {
    // Process input
    input
}
```

### Const Generics

```neuro
struct FixedArray<T, const N: usize> {
    data: [T; N],
}

fn matrix_multiply<const M: usize, const N: usize, const P: usize>(
    a: Tensor<f32, [M, N]>,
    b: Tensor<f32, [N, P]>
) -> Tensor<f32, [M, P]> {
    a @ b
}
```

### Bounds and Constraints

```neuro
trait Numeric {
    fn add(self, other: Self) -> Self;
    fn zero() -> Self;
}

fn sum<T: Numeric>(values: [T]) -> T {
    values.fold(T::zero(), |acc, x| acc.add(x))
}
```

## Function Types

### Function Signatures

```neuro
fn add(a: i32, b: i32) -> i32 { a + b }

// Function type: fn(i32, i32) -> i32
let operation: fn(i32, i32) -> i32 = add;
```

### Higher-Order Functions

```neuro
fn map<T, U>(values: [T], f: fn(T) -> U) -> [U] {
    // Implementation
}

// Usage
let numbers = [1, 2, 3, 4];
let doubled = map(numbers, |x| x * 2);
```

### Closures

```neuro
let multiplier = 5;
let closure = |x: i32| x * multiplier;
```

## Reference Types

### Immutable References

```neuro
fn length(s: &str) -> usize {
    s.len()
}

let text = "hello";
let len = length(&text);
```

### Mutable References

```neuro
fn increment(x: &mut i32) {
    *x += 1;
}

let mut value = 10;
increment(&mut value);
```

## Memory Management Types

### Automatic Reference Counting (ARC)

```neuro
use memory::Arc;

let data = Arc::new(expensive_computation());
let clone1 = data.clone(); // Reference count: 2
let clone2 = data.clone(); // Reference count: 3
```

### Memory Pools

```neuro
use memory::MemoryPool;

let pool = MemoryPool::new(1024 * 1024); // 1MB pool
let tensor: Tensor<f32, [100, 100]> = pool.allocate_tensor();
```

## Type Inference

NEURO performs extensive type inference:

### Local Variable Inference

```neuro
let x = 42;        // Inferred as i32
let y = 3.14;      // Inferred as f64
let z = x + y as i32; // Explicit cast required
```

### Function Return Type Inference

```neuro
fn create_vector() -> _ {
    vec![1, 2, 3, 4, 5] // Return type inferred as Vec<i32>
}
```

### Tensor Shape Inference

```neuro
let a: Tensor<f32, [3, 4]> = create_matrix();
let b: Tensor<f32, [4, 5]> = create_matrix();
let result = a @ b; // Shape inferred as [3, 5]
```

## Type Coercion

### Numeric Coercion

```neuro
let a: i32 = 42;
let b: i64 = a as i64; // Explicit cast
let c: f32 = a as f32; // Explicit cast
```

### Reference Coercion

```neuro
fn takes_slice(s: &[i32]) {}

let array = [1, 2, 3, 4, 5];
takes_slice(&array); // Array coerces to slice
```

## Automatic Differentiation Types

### Gradient-Enabled Types

```neuro
#[grad]
fn neural_network(input: Tensor<f32, [N, 784]>) -> Tensor<f32, [N, 10]> {
    // Forward pass - gradients computed automatically
    let hidden = relu(input @ weights1 + bias1);
    softmax(hidden @ weights2 + bias2)
}

// Gradient type is automatically generated:
// GradTensor<f32, [N, 784]> for input gradients
```

## GPU Types

### Device Types

```neuro
enum Device {
    CPU,
    CUDA(u32),    // Device ID
    Vulkan(u32),  // Device ID
}

#[kernel]
fn matrix_multiply_gpu<const M: usize, const N: usize, const P: usize>(
    a: Tensor<f32, [M, N]>,
    b: Tensor<f32, [N, P]>
) -> Tensor<f32, [M, P]> on Device::CUDA(0) {
    // GPU kernel implementation
}
```

## Error Types

### Result Type

```neuro
enum Result<T, E> {
    Ok(T),
    Err(E),
}

fn divide(a: f32, b: f32) -> Result<f32, str> {
    if b == 0.0 {
        Err("Division by zero")
    } else {
        Ok(a / b)
    }
}
```

### Option Type

```neuro
enum Option<T> {
    Some(T),
    None,
}

fn find_index(arr: &[i32], target: i32) -> Option<usize> {
    for (i, &value) in arr.iter().enumerate() {
        if value == target {
            return Some(i);
        }
    }
    None
}
```

## Type System Rules

### Ownership and Borrowing

1. Each value has a single owner
2. When the owner goes out of scope, the value is dropped
3. References must always be valid
4. Either one mutable reference or any number of immutable references

### Lifetime Rules

```neuro
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
```

### Type Safety Guarantees

1. No null pointer dereferences
2. No buffer overflows
3. No memory leaks (with ARC)
4. No data races in concurrent code
5. No invalid tensor operations at runtime

## Implementation Status

| Feature | Status | Notes |
|---------|--------|--------|
| Primitive types |  Complete | All basic types implemented |
| Arrays/Tuples |  Complete | Fixed and dynamic arrays |
| Structs/Enums |  Complete | Full support with pattern matching |
| Tensor types |  Complete | With shape verification |
| Generics |  Complete | Type and const generics |
| References |  Complete | Borrow checker implemented |
| ARC |  Complete | Memory management |
| Type inference |  Complete | Local and function inference |
| AD types | =§ In Progress | Basic framework implemented |
| GPU types | =§ In Progress | Device abstraction planned |

The NEURO type system provides the foundation for safe, efficient AI/ML programming with compile-time guarantees and zero-cost abstractions.