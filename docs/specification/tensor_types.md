# NEURO Tensor Types Specification v1.0

## Overview

Tensors are first-class citizens in NEURO, designed from the ground up for high-performance machine learning workloads with compile-time shape verification and zero-cost abstractions.

## Core Tensor Type

### Basic Syntax

```neuro
Tensor<T, [D1, D2, ..., DN]>
```

Where:
- `T` is the element type (f32, f64, i32, etc.)
- `[D1, D2, ..., DN]` is the compile-time shape specification
- Each `Di` can be a compile-time constant or `_` for runtime dimensions

### Type Examples

```neuro
// 2D matrix: 3 rows, 4 columns
let matrix: Tensor<f32, [3, 4]>;

// 3D tensor: batch size 32, height 28, width 28
let images: Tensor<u8, [32, 28, 28]>;

// 4D tensor: batch, channels, height, width (NCHW format)
let feature_maps: Tensor<f32, [16, 64, 56, 56]>;

// Vector (1D tensor)
let weights: Tensor<f64, [1000]>;

// Scalar (0D tensor)
let loss: Tensor<f32, []>;
```

## Shape Specification

### Static Shapes

Compile-time known dimensions enable optimizations:

```neuro
// All dimensions known at compile time
let conv_kernel: Tensor<f32, [64, 32, 3, 3]>; // out_ch, in_ch, h, w

// Enables aggressive optimizations
#[kernel]
fn optimized_conv(input: Tensor<f32, [1, 32, 224, 224]>) -> Tensor<f32, [1, 64, 224, 224]> {
    // Compiler can unroll loops and optimize memory access
}
```

### Dynamic Shapes

Runtime-determined dimensions using `_`:

```neuro
// Batch size determined at runtime
let dynamic_batch: Tensor<f32, [_, 784]>;

// Fully dynamic tensor
let unknown_shape: Tensor<f32, [_, _, _]>;

// Mixed static/dynamic
let sequence: Tensor<i32, [_, 512]>; // Variable sequence length, fixed embedding size
```

### Shape Constraints

```neuro
// Generic functions with shape constraints
fn matrix_multiply<const M: usize, const N: usize, const P: usize>(
    a: Tensor<f32, [M, N]>,
    b: Tensor<f32, [N, P]>
) -> Tensor<f32, [M, P]> {
    a @ b // Shape compatibility verified at compile time
}

// Constraint ensuring square matrices
fn matrix_inverse<const N: usize>(input: Tensor<f64, [N, N]>) -> Tensor<f64, [N, N]> {
    // Implementation for square matrix inversion
}
```

## Tensor Creation

### Literal Syntax

```neuro
// 1D tensor
let vector = tensor![1.0, 2.0, 3.0, 4.0];

// 2D tensor 
let matrix = tensor![
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0],
    [7.0, 8.0, 9.0]
];

// 3D tensor
let cube = tensor![
    [[1, 2], [3, 4]],
    [[5, 6], [7, 8]]
];
```

### Constructor Functions

```neuro
// Zero-filled tensors
let zeros: Tensor<f32, [10, 10]> = Tensor::zeros();

// One-filled tensors
let ones: Tensor<f64, [5, 5, 5]> = Tensor::ones();

// Random tensors
let random: Tensor<f32, [100, 100]> = Tensor::random();
let normal: Tensor<f32, [50, 50]> = Tensor::normal(0.0, 1.0); // mean, std

// From existing data
let from_vec: Tensor<i32, [2, 3]> = Tensor::from_vec(vec![1, 2, 3, 4, 5, 6], [2, 3]);

// Identity matrices
let identity: Tensor<f64, [5, 5]> = Tensor::eye();

// Range tensors
let range: Tensor<i32, [10]> = Tensor::arange(0, 10, 1); // start, end, step
```

### Type Inference

```neuro
// Type inference from literals
let inferred = tensor![1.0, 2.0, 3.0]; // Tensor<f64, [3]>
let explicit: Tensor<f32, [3]> = tensor![1.0, 2.0, 3.0]; // Explicit type

// Shape inference from operations
let a: Tensor<f32, [3, 4]> = Tensor::random();
let b: Tensor<f32, [4, 5]> = Tensor::random();
let c = a @ b; // Shape inferred as [3, 5]
```

## Tensor Operations

### Element-wise Operations

```neuro
let a: Tensor<f32, [3, 4]> = Tensor::random();
let b: Tensor<f32, [3, 4]> = Tensor::random();

// Arithmetic operations
let sum = a + b;
let diff = a - b;
let product = a * b; // Element-wise multiplication
let quotient = a / b;

// Scalar operations
let scaled = a * 2.0;
let shifted = a + 1.0;

// Mathematical functions
let exp_a = a.exp();
let log_a = a.log();
let sqrt_a = a.sqrt();
let abs_a = a.abs();

// Trigonometric functions
let sin_a = a.sin();
let cos_a = a.cos();
let tan_a = a.tan();
```

### Linear Algebra Operations

```neuro
// Matrix multiplication
let a: Tensor<f32, [3, 4]> = Tensor::random();
let b: Tensor<f32, [4, 5]> = Tensor::random();
let c = a @ b; // Result: Tensor<f32, [3, 5]>

// Tensor contraction (Einstein summation)
let result = tensor_contract("ij,jk->ik", a, b);

// Dot product (1D tensors)
let v1: Tensor<f32, [100]> = Tensor::random();
let v2: Tensor<f32, [100]> = Tensor::random();
let dot = v1.dot(v2); // Result: Tensor<f32, []> (scalar)

// Cross product (3D vectors)
let x: Tensor<f32, [3]> = tensor![1.0, 0.0, 0.0];
let y: Tensor<f32, [3]> = tensor![0.0, 1.0, 0.0];
let cross = x.cross(y); // Result: Tensor<f32, [3]>

// Transpose
let matrix: Tensor<f32, [3, 4]> = Tensor::random();
let transposed = matrix.transpose(); // Result: Tensor<f32, [4, 3]>

// More complex transpose
let tensor_4d: Tensor<f32, [2, 3, 4, 5]> = Tensor::random();
let reordered = tensor_4d.permute([3, 1, 0, 2]); // Result: Tensor<f32, [5, 3, 2, 4]>
```

### Broadcasting

Automatic shape compatibility for element-wise operations:

```neuro
// Broadcasting rules (similar to NumPy)
let a: Tensor<f32, [3, 4]> = Tensor::ones();
let b: Tensor<f32, [4]> = Tensor::ones();    // Broadcasting: [4] -> [1, 4]
let c: Tensor<f32, [3, 1]> = Tensor::ones(); // Broadcasting: [3, 1] -> [3, 4]

let result1 = a + b; // Shape: [3, 4]
let result2 = a + c; // Shape: [3, 4]
let result3 = b + c; // Shape: [3, 4]

// Complex broadcasting
let x: Tensor<f32, [8, 1, 6, 1]> = Tensor::ones();
let y: Tensor<f32, [7, 1, 5]> = Tensor::ones();
let z = x + y; // Result shape: [8, 7, 6, 5]
```

### Reduction Operations

```neuro
let tensor: Tensor<f32, [3, 4, 5]> = Tensor::random();

// Sum reductions
let total_sum = tensor.sum(); // Result: Tensor<f32, []>
let sum_axis0 = tensor.sum_axis(0); // Result: Tensor<f32, [4, 5]>
let sum_axis12 = tensor.sum_axes([1, 2]); // Result: Tensor<f32, [3]>

// Other reductions
let mean = tensor.mean();
let max = tensor.max();
let min = tensor.min();
let std = tensor.std();
let var = tensor.var();

// Along specific axes
let row_means = tensor.mean_axis(1); // Mean across axis 1
let col_maxes = tensor.max_axis(2);  // Max across axis 2

// Argmax/Argmin
let max_indices = tensor.argmax(); // Global argmax
let max_per_row = tensor.argmax_axis(1); // Argmax per row
```

### Indexing and Slicing

```neuro
let tensor: Tensor<f32, [4, 6, 8]> = Tensor::random();

// Basic indexing
let element = tensor[1, 2, 3]; // Single element: Tensor<f32, []>
let slice = tensor[1]; // First dimension slice: Tensor<f32, [6, 8]>

// Range slicing
let submatrix = tensor[1..3, 2..5, ..]; // Tensor<f32, [2, 3, 8]>
let strided = tensor[.., ..6, ..2]; // Every 2nd element in last dim

// Advanced indexing
let indices: Tensor<i32, [2]> = tensor![0, 2];
let selected = tensor.index_select(0, indices); // Select specific indices

// Boolean indexing
let mask: Tensor<bool, [4, 6, 8]> = tensor > 0.5;
let filtered = tensor.masked_select(mask); // 1D tensor with selected elements

// Fancy indexing
let row_indices: Tensor<i32, [3]> = tensor![0, 2, 1];
let col_indices: Tensor<i32, [3]> = tensor![1, 3, 5];
let gathered = tensor.gather_2d(row_indices, col_indices);
```

### Shape Manipulation

```neuro
let tensor: Tensor<f32, [2, 3, 4]> = Tensor::random();

// Reshape (total elements must match)
let reshaped = tensor.reshape([6, 4]); // Tensor<f32, [6, 4]>
let flattened = tensor.flatten(); // Tensor<f32, [24]>

// View operations (no data copy)
let viewed = tensor.view([4, 6]); // Same data, different shape
let squeezed = tensor.unsqueeze(1); // Add dimension: Tensor<f32, [2, 1, 3, 4]>
let expanded = tensor.squeeze(); // Remove size-1 dimensions

// Concatenation
let a: Tensor<f32, [2, 3]> = Tensor::ones();
let b: Tensor<f32, [2, 3]> = Tensor::zeros();
let concat_rows = Tensor::concat([a, b], axis: 0); // Tensor<f32, [4, 3]>
let concat_cols = Tensor::concat([a, b], axis: 1); // Tensor<f32, [2, 6]>

// Stack (adds new dimension)
let stacked = Tensor::stack([a, b], axis: 0); // Tensor<f32, [2, 2, 3]>

// Split
let chunks = tensor.chunk(2, axis: 1); // Split into 2 chunks along axis 1
let splits = tensor.split([1, 2], axis: 1); // Split into sizes [1, 2]
```

## Memory Layout and Performance

### Memory Layouts

```neuro
// Row-major (C-style, default)
let row_major: Tensor<f32, [100, 100]> = Tensor::zeros();

// Column-major (Fortran-style)
let col_major = row_major.to_layout(Layout::ColumnMajor);

// Custom strides
let strided = Tensor::zeros_with_strides([10, 10], [1, 10]); // Column-major strides
```

### SIMD Optimization

```neuro
// Automatic vectorization for aligned operations
let a: Tensor<f32, [1000]> = Tensor::random();
let b: Tensor<f32, [1000]> = Tensor::random();
let c = a + b; // Automatically vectorized with AVX2/AVX-512

// Explicit SIMD hints
#[simd]
fn vectorized_operation(input: Tensor<f32, [N]>) -> Tensor<f32, [N]> {
    input.map(|x| x.exp()) // Compiler vectorizes the exponential
}
```

### GPU Integration

```neuro
use memory::Device;

// GPU tensor creation
let gpu_tensor = Tensor::<f32, [1000, 1000]>::zeros_on(Device::CUDA(0));

// CPU-GPU transfers
let cpu_tensor: Tensor<f32, [100, 100]> = Tensor::ones();
let gpu_copy = cpu_tensor.to_device(Device::CUDA(0));
let back_to_cpu = gpu_copy.to_device(Device::CPU);

// GPU operations
#[kernel]
fn gpu_matrix_add(a: Tensor<f32, [M, N]>, b: Tensor<f32, [M, N]>) -> Tensor<f32, [M, N]> {
    a + b // Executes on GPU
}
```

## Type Safety and Compile-Time Checks

### Shape Compatibility

```neuro
// Compile-time shape verification
fn safe_matmul<const M: usize, const N: usize, const P: usize>(
    a: Tensor<f32, [M, N]>,
    b: Tensor<f32, [N, P]>  // N must match - enforced at compile time
) -> Tensor<f32, [M, P]> {
    a @ b
}

// This would fail to compile:
// let a: Tensor<f32, [3, 4]> = Tensor::zeros();
// let b: Tensor<f32, [5, 6]> = Tensor::zeros();
// let c = safe_matmul(a, b); // Error: 4 != 5
```

### Runtime Bounds Checking

```neuro
// Debug builds include bounds checking
let tensor: Tensor<f32, [10, 10]> = Tensor::zeros();

// Safe access
match tensor.try_get([5, 15]) {
    Some(value) => println!("Value: {}", value),
    None => println!("Index out of bounds"),
}

// Panic in debug, undefined behavior in release
let value = tensor[5, 15]; // Panics if bounds checking enabled
```

## Advanced Features

### Custom Element Types

```neuro
// Complex numbers
#[derive(Clone, Copy)]
struct Complex {
    real: f32,
    imag: f32,
}

impl Add for Complex { /* ... */ }
impl Mul for Complex { /* ... */ }

let complex_tensor: Tensor<Complex, [100, 100]> = Tensor::zeros();
```

### Tensor Traits

```neuro
trait TensorElement: Clone + Copy + Send + Sync {
    const ZERO: Self;
    const ONE: Self;
    
    fn add(self, other: Self) -> Self;
    fn mul(self, other: Self) -> Self;
}

impl TensorElement for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    
    fn add(self, other: Self) -> Self { self + other }
    fn mul(self, other: Self) -> Self { self * other }
}
```

### Lazy Evaluation

```neuro
// Lazy tensor expressions (future feature)
let a: Tensor<f32, [1000, 1000]> = Tensor::random();
let b: Tensor<f32, [1000, 1000]> = Tensor::random();
let c: Tensor<f32, [1000, 1000]> = Tensor::random();

// Creates computation graph, doesn't execute immediately
let expr = (a + b) * c - a.transpose() @ b;

// Optimize and execute
let result = expr.eval(); // Fused operations, optimal memory usage
```

## Implementation Status

| Feature | Status | Notes |
|---------|--------|--------|
| Basic tensor types | ✅ COMPLETE | All core types implemented and working |
| Shape verification | ✅ COMPLETE | Compile-time shape checking working |
| Element-wise ops | ✅ COMPLETE | Full arithmetic support implemented |
| Linear algebra | ✅ COMPLETE | Matrix multiplication, transpose working |
| Broadcasting | ✅ COMPLETE | Basic broadcasting implemented |
| Reductions | ✅ COMPLETE | Sum, mean, max, min working |
| Indexing/slicing | ✅ COMPLETE | Basic indexing implemented |
| Shape manipulation | ✅ COMPLETE | Reshape operations working |
| Memory layouts | ✅ COMPLETE | Row-major layout implemented |
| SIMD optimization | 🏗️ IN PROGRESS | Automatic vectorization in development |
| GPU integration | 📅 PLANNED | GPU device abstraction planned for Phase 2 |
| Advanced operations | 📅 PLANNED | Complex tensor operations planned for Phase 2 |
| Lazy evaluation | 📅 PLANNED | Future optimization feature for Phase 3 |

## Performance Characteristics

- **Zero-cost abstractions**: High-level operations compile to optimal code
- **SIMD vectorization**: Automatic use of AVX2/AVX-512 instructions
- **Memory alignment**: 32-byte alignment for optimal SIMD performance
- **Cache efficiency**: Optimized memory access patterns
- **GPU offloading**: Seamless CPU/GPU computation with unified API

NEURO's tensor system provides the foundation for high-performance ML computing while maintaining type safety and ease of use.