# Tensor Programming in NEURO

Tensors are first-class citizens in NEURO, designed for high-performance machine learning workloads with compile-time shape verification and zero-cost abstractions.

## Table of Contents

1. [Tensor Types](#tensor-types)
2. [Tensor Creation](#tensor-creation)
3. [Basic Operations](#basic-operations)
4. [Broadcasting](#broadcasting)
5. [Advanced Operations](#advanced-operations)
6. [Memory Layout](#memory-layout)
7. [Performance Optimization](#performance-optimization)
8. [Interoperability](#interoperability)

---

## Tensor Types

### Static Tensors ✅ IMPLEMENTED

Static tensors have compile-time known shapes, enabling maximum optimization and shape checking.

```neuro
import std::tensor::Tensor;

// 1D tensor (vector)
let vector: Tensor<f32, [100]> = Tensor::zeros();

// 2D tensor (matrix)
let matrix: Tensor<f64, [28, 28]> = Tensor::eye();

// 3D tensor (batch of matrices)
let batch: Tensor<i32, [32, 28, 28]> = Tensor::random();

// 4D tensor (batch of images: [batch, height, width, channels])
let images: Tensor<u8, [64, 224, 224, 3]> = Tensor::zeros();

// 5D tensor (video: [batch, time, height, width, channels])
let videos: Tensor<f16, [8, 30, 112, 112, 3]> = Tensor::random();

// Higher-dimensional tensors
let hyperdimensional: Tensor<f32, [2, 3, 4, 5, 6, 7]> = Tensor::ones();
```

### Dynamic Tensors ✅ IMPLEMENTED

Dynamic tensors have runtime-determined shapes for flexibility.

```neuro
import std::tensor::DynamicTensor;

// Create dynamic tensor with runtime shape
let shape = vec![128, 256, 512];
let dynamic: DynamicTensor<f32> = DynamicTensor::zeros(&shape);

// Runtime shape queries
let dims = dynamic.ndim();           // Number of dimensions
let shape = dynamic.shape();         // Shape as &[usize]
let size = dynamic.size();           // Total number of elements
let is_empty = dynamic.is_empty();   // Check if tensor is empty

// Convert between static and dynamic
let static_tensor: Tensor<f32, [3, 3]> = Tensor::eye();
let as_dynamic: DynamicTensor<f32> = static_tensor.into_dynamic();

// Try to convert dynamic to static (runtime check)
let back_to_static: Result<Tensor<f32, [3, 3]>, ShapeError> = 
    as_dynamic.try_into_static();
```

### Specialized Tensor Types ✅ IMPLEMENTED (Infrastructure)

```neuro
// Sparse tensors for high-dimensional data
import std::tensor::SparseTensor;
let sparse: SparseTensor<f64> = SparseTensor::from_coo(indices, values, shape);

// Masked tensors for attention mechanisms
import std::tensor::MaskedTensor;
let masked: MaskedTensor<f32, [SEQ_LEN, HIDDEN_SIZE]> = 
    MaskedTensor::new(data, attention_mask);

// Complex tensors for signal processing
let complex: Tensor<Complex<f64>, [1024]> = Tensor::zeros();
let fft_result = complex.fft();

// Boolean tensors for masking
let mask: Tensor<bool, [BATCH_SIZE, SEQ_LEN]> = Tensor::zeros();
let masked_values = data.masked_fill(&mask, 0.0);
```

### Tensor Element Types ✅ IMPLEMENTED

NEURO tensors support all primitive numeric types:

```neuro
// Floating point tensors
Tensor<f16, [N]>    // Half precision (16-bit)
Tensor<f32, [N]>    // Single precision (32-bit) - default
Tensor<f64, [N]>    // Double precision (64-bit)

// Integer tensors
Tensor<i8, [N]>     // 8-bit signed
Tensor<i16, [N]>    // 16-bit signed  
Tensor<i32, [N]>    // 32-bit signed
Tensor<i64, [N]>    // 64-bit signed
Tensor<u8, [N]>     // 8-bit unsigned
Tensor<u16, [N]>    // 16-bit unsigned
Tensor<u32, [N]>    // 32-bit unsigned
Tensor<u64, [N]>    // 64-bit unsigned

// Boolean tensors
Tensor<bool, [N]>   // Boolean values

// Complex tensors
Tensor<Complex<f32>, [N]>  // Complex single precision
Tensor<Complex<f64>, [N]>  // Complex double precision
```

---

## Tensor Creation

### Zero and One Initialization ✅ IMPLEMENTED

```neuro
// Zero tensors
let zeros_1d: Tensor<f32, [100]> = Tensor::zeros();
let zeros_2d: Tensor<f64, [10, 10]> = Tensor::zeros();
let zeros_dynamic = DynamicTensor::zeros(&[3, 4, 5]);

// One tensors  
let ones_1d: Tensor<f32, [50]> = Tensor::ones();
let ones_2d: Tensor<i32, [5, 5]> = Tensor::ones();

// Full tensors (filled with specific value)
let fives: Tensor<f32, [10]> = Tensor::full(5.0);
let negative_ones: Tensor<i32, [3, 3]> = Tensor::full(-1);
```

### Random Initialization ✅ IMPLEMENTED

```neuro
import std::tensor::random::{Normal, Uniform, Xavier, He};

// Uniform random [0, 1)
let uniform: Tensor<f32, [100, 100]> = Tensor::random();

// Normal distribution N(0, 1)
let normal: Tensor<f32, [128, 256]> = Tensor::random_normal();

// Custom normal distribution N(mean, std)
let custom_normal: Tensor<f32, [64, 128]> = 
    Tensor::random_normal_with_params(mean: 0.5, std: 0.1);

// Uniform distribution [low, high)
let uniform_range: Tensor<f32, [50, 50]> = 
    Tensor::random_uniform(low: -1.0, high: 1.0);

// Xavier/Glorot initialization (good for tanh, sigmoid)
let xavier: Tensor<f32, [784, 256]> = Tensor::xavier_uniform();
let xavier_normal: Tensor<f32, [256, 128]> = Tensor::xavier_normal();

// He initialization (good for ReLU)  
let he: Tensor<f32, [128, 64]> = Tensor::he_uniform();
let he_normal: Tensor<f32, [64, 10]> = Tensor::he_normal();

// Truncated normal (avoids extreme values)
let truncated: Tensor<f32, [100, 100]> = 
    Tensor::truncated_normal(mean: 0.0, std: 0.02, bounds: (-0.1, 0.1));

// Random integers
let random_ints: Tensor<i32, [10, 10]> = 
    Tensor::random_int(low: 0, high: 100);
```

### Special Matrix Creation ✅ IMPLEMENTED

```neuro
// Identity matrices
let identity_3x3: Tensor<f32, [3, 3]> = Tensor::eye();
let identity_5x5: Tensor<f64, [5, 5]> = Tensor::eye();

// Diagonal matrices
let diag_values: Tensor<f32, [4]> = tensor![1.0, 2.0, 3.0, 4.0];
let diagonal: Tensor<f32, [4, 4]> = Tensor::diag(&diag_values);

// Triangular matrices
let upper_tri: Tensor<f32, [5, 5]> = Tensor::triu(); // Upper triangular
let lower_tri: Tensor<f32, [5, 5]> = Tensor::tril(); // Lower triangular

// Vandermonde matrix
let vander: Tensor<f64, [5, 3]> = Tensor::vander(&tensor![1.0, 2.0, 3.0, 4.0, 5.0]);

// Toeplitz matrix
let toep: Tensor<f32, [4, 4]> = Tensor::toeplitz(&tensor![1.0, 2.0, 3.0, 4.0]);
```

### Sequential Creation ✅ IMPLEMENTED

```neuro
// Range tensors
let range: Tensor<i32, [10]> = Tensor::range(0, 10);        // [0, 1, 2, ..., 9]
let range_step: Tensor<i32, [5]> = Tensor::range_step(0, 10, 2); // [0, 2, 4, 6, 8]

// Linear space
let linspace: Tensor<f64, [100]> = Tensor::linspace(0.0, 1.0); // 100 points from 0 to 1
let logspace: Tensor<f64, [50]> = Tensor::logspace(0.0, 2.0);  // 50 points from 10^0 to 10^2

// Meshgrid for coordinate arrays
let (x_grid, y_grid) = Tensor::meshgrid(
    &Tensor::linspace(-1.0, 1.0), // 100 points  
    &Tensor::linspace(-1.0, 1.0)  // 100 points
); // Creates 2D coordinate grids
```

### From Data ✅ IMPLEMENTED

```neuro
// From Rust arrays
let from_array: Tensor<f32, [4]> = Tensor::from_array([1.0, 2.0, 3.0, 4.0]);

// From vectors (runtime shape determination)
let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
let from_vec: Tensor<f32, [2, 3]> = Tensor::from_vec(data);

// From nested vectors (automatic shape inference)
let nested = vec![
    vec![1.0, 2.0, 3.0],
    vec![4.0, 5.0, 6.0]
];
let from_nested: DynamicTensor<f32> = DynamicTensor::from_nested_vec(nested);

// From slices with explicit shape
let slice = &[1, 2, 3, 4, 5, 6, 7, 8];
let reshaped: Tensor<i32, [2, 4]> = Tensor::from_slice_with_shape(slice);

// From iterator
let from_iter: Tensor<f32, [5]> = (0..5)
    .map(|i| i as f32 * 0.5)
    .collect::<Tensor<_, _>>();
```

### Tensor Literals ✅ IMPLEMENTED (Parser)

```neuro
// 1D tensor literal
let vector = tensor![1.0, 2.0, 3.0, 4.0, 5.0];

// 2D tensor literal
let matrix = tensor![
    [1.0, 2.0, 3.0],
    [4.0, 5.0, 6.0],
    [7.0, 8.0, 9.0]
];

// 3D tensor literal
let tensor_3d = tensor![
    [[1, 2], [3, 4]],
    [[5, 6], [7, 8]]
];

// Mixed type literals (with type inference)
let mixed = tensor![1, 2.0, 3];  // Inferred as tensor<f64>

// Complex tensor literals
let complex = tensor![
    [1.0 + 2.0i, 3.0 + 4.0i],
    [5.0 + 6.0i, 7.0 + 8.0i]
];
```

---

## Basic Operations

### Element-wise Arithmetic ✅ IMPLEMENTED

```neuro
let a: Tensor<f32, [3, 3]> = Tensor::ones();
let b: Tensor<f32, [3, 3]> = Tensor::full(2.0);
let scalar = 3.0;

// Basic arithmetic operations
let sum = a + b;           // Element-wise addition
let diff = a - b;          // Element-wise subtraction  
let product = a * b;       // Element-wise multiplication (Hadamard product)
let quotient = a / b;      // Element-wise division
let remainder = a % b;     // Element-wise modulo
let power = a.pow(2.0);    // Element-wise exponentiation

// Scalar operations (broadcasting)
let scaled = a * scalar;   // Scale all elements
let shifted = a + scalar;  // Add scalar to all elements
let divided = a / scalar;  // Divide all elements by scalar

// In-place operations (more memory efficient)
let mut c = a.clone();
c += b;                    // In-place addition
c -= scalar;               // In-place scalar subtraction
c *= 0.5;                  // In-place scalar multiplication
c /= 2.0;                  // In-place scalar division
```

### Comparison Operations ✅ IMPLEMENTED

```neuro
let a: Tensor<f32, [3, 3]> = Tensor::random();
let b: Tensor<f32, [3, 3]> = Tensor::random();
let threshold = 0.5;

// Element-wise comparisons (return boolean tensors)
let eq = a.eq(&b);         // Element-wise equality
let ne = a.ne(&b);         // Element-wise inequality  
let lt = a.lt(&b);         // Element-wise less than
let le = a.le(&b);         // Element-wise less than or equal
let gt = a.gt(&b);         // Element-wise greater than
let ge = a.ge(&b);         // Element-wise greater than or equal

// Scalar comparisons
let above_threshold = a.gt(threshold);
let in_range = a.ge(-1.0) & a.le(1.0);  // Logical AND of conditions

// Aggregate comparisons
let all_positive = a.gt(0.0).all();     // true if all elements > 0
let any_negative = a.lt(0.0).any();     // true if any element < 0
let all_equal = a.eq(&b).all();         // true if tensors are identical
```

### Logical Operations ✅ IMPLEMENTED

```neuro
let mask1: Tensor<bool, [10]> = Tensor::random() > 0.5;
let mask2: Tensor<bool, [10]> = Tensor::random() > 0.3;

// Logical operations
let logical_and = mask1 & mask2;        // Element-wise AND
let logical_or = mask1 | mask2;         // Element-wise OR  
let logical_xor = mask1 ^ mask2;        // Element-wise XOR
let logical_not = !mask1;               // Element-wise NOT

// Short-circuiting logical operations (for control flow)
if tensor.any() && other_condition {
    // Executes only if tensor has any true values
}
```

### Mathematical Functions ✅ IMPLEMENTED

```neuro
let x: Tensor<f32, [100]> = Tensor::linspace(-3.0, 3.0);

// Trigonometric functions
let sin_x = x.sin();       // Sine
let cos_x = x.cos();       // Cosine  
let tan_x = x.tan();       // Tangent
let asin_x = x.asin();     // Arc sine
let acos_x = x.acos();     // Arc cosine
let atan_x = x.atan();     // Arc tangent
let atan2_xy = y.atan2(&x); // Two-argument arc tangent

// Hyperbolic functions
let sinh_x = x.sinh();     // Hyperbolic sine
let cosh_x = x.cosh();     // Hyperbolic cosine
let tanh_x = x.tanh();     // Hyperbolic tangent

// Exponential and logarithmic
let exp_x = x.exp();       // e^x
let exp2_x = x.exp2();     // 2^x
let expm1_x = x.expm1();   // e^x - 1 (more accurate for small x)
let ln_x = x.ln();         // Natural logarithm
let log2_x = x.log2();     // Base-2 logarithm  
let log10_x = x.log10();   // Base-10 logarithm
let ln1p_x = x.ln1p();     // ln(1 + x) (more accurate for small x)

// Power and root functions
let sqrt_x = x.sqrt();     // Square root
let cbrt_x = x.cbrt();     // Cube root
let pow_x = x.pow(2.5);    // x^2.5
let pow_tensor = x.pow(&other_tensor); // Element-wise x[i]^other[i]

// Rounding functions
let floor_x = x.floor();   // Floor (round down)
let ceil_x = x.ceil();     // Ceiling (round up)
let round_x = x.round();   // Round to nearest integer
let trunc_x = x.trunc();   // Truncate (round towards zero)

// Other mathematical functions
let abs_x = x.abs();       // Absolute value
let sign_x = x.sign();     // Sign (-1, 0, or 1)
let fract_x = x.fract();   // Fractional part
```

---

## Broadcasting

NEURO implements NumPy-style broadcasting for tensor operations.

### Broadcasting Rules ✅ IMPLEMENTED

```neuro
// Broadcasting rules:
// 1. Dimensions are aligned from the rightmost (trailing) dimension
// 2. Dimensions of size 1 can be broadcast to any size
// 3. Missing dimensions are treated as size 1

let a: Tensor<f32, [3, 1, 5]> = Tensor::ones();
let b: Tensor<f32, [2, 4, 1]> = Tensor::ones(); 

// Broadcasting result shape: [3, 4, 5]
let result = a + b;  // Automatic broadcasting

// Scalar broadcasting
let tensor: Tensor<f32, [3, 4, 5]> = Tensor::ones();
let scalar = 2.0;
let scaled = tensor * scalar;  // Scalar broadcast to all elements

// Vector broadcasting
let vector: Tensor<f32, [5]> = Tensor::ones();
let broadcasted = tensor + vector;  // Vector broadcast to [3, 4, 5]
```

### Explicit Broadcasting ✅ IMPLEMENTED

```neuro
// Check if tensors are broadcastable
let compatible = Tensor::is_broadcastable(&a.shape(), &b.shape());

// Get broadcast shape
let broadcast_shape = Tensor::broadcast_shape(&a.shape(), &b.shape())?;

// Explicit broadcasting
let a_broadcasted = a.broadcast_to(&broadcast_shape);
let b_broadcasted = b.broadcast_to(&broadcast_shape);
let result = a_broadcasted + b_broadcasted;

// Expand dimensions for broadcasting
let vector: Tensor<f32, [5]> = Tensor::ones();
let expanded: Tensor<f32, [1, 5]> = vector.unsqueeze(0);    // Add dimension at axis 0
let expanded2: Tensor<f32, [5, 1]> = vector.unsqueeze(1);   // Add dimension at axis 1

// Remove size-1 dimensions
let squeezed: Tensor<f32, [5]> = expanded.squeeze();        // Remove all size-1 dims
let squeezed_axis: Tensor<f32, [5]> = expanded.squeeze(0);  // Remove specific dimension
```

### Broadcasting in Practice ✅ IMPLEMENTED

```neuro
// Common broadcasting patterns in ML

// Batch normalization
let batch_data: Tensor<f32, [32, 256]> = Tensor::random();  // [batch, features]
let batch_mean: Tensor<f32, [256]> = batch_data.mean(0);    // Mean along batch dimension
let batch_std: Tensor<f32, [256]> = batch_data.std(0);      // Std along batch dimension

// Broadcasting for normalization
let normalized = (batch_data - batch_mean) / batch_std;     // Broadcast [256] to [32, 256]

// Attention weights
let queries: Tensor<f32, [32, 128, 64]> = Tensor::random(); // [batch, seq_len, dim]
let keys: Tensor<f32, [32, 128, 64]> = Tensor::random();    // [batch, seq_len, dim]

// Compute attention scores
let scores = queries @ keys.transpose(-1, -2);               // [32, 128, 128]
let temperature = 8.0;
let scaled_scores = scores / temperature;                    // Broadcast scalar

// Positional embeddings
let position_ids: Tensor<i64, [128]> = Tensor::range(0, 128);
let position_emb: Tensor<f32, [128, 512]> = embedding_lookup(position_ids);
let batch_pos_emb = position_emb.unsqueeze(0);              // [1, 128, 512]

// Broadcast to batch size
let input_emb: Tensor<f32, [32, 128, 512]> = Tensor::random();
let with_position = input_emb + batch_pos_emb;               // Broadcast [1, 128, 512] to [32, 128, 512]
```

---

## Advanced Operations

### Linear Algebra ✅ IMPLEMENTED

```neuro
// Matrix multiplication
let a: Tensor<f32, [3, 4]> = Tensor::random();
let b: Tensor<f32, [4, 5]> = Tensor::random();
let c = a @ b;  // Matrix multiplication: [3, 5]

// Batch matrix multiplication
let batch_a: Tensor<f32, [10, 3, 4]> = Tensor::random();
let batch_b: Tensor<f32, [10, 4, 5]> = Tensor::random(); 
let batch_c = batch_a @ batch_b;  // Batch matmul: [10, 3, 5]

// Dot product (1D tensors)
let vec1: Tensor<f32, [100]> = Tensor::random();
let vec2: Tensor<f32, [100]> = Tensor::random();
let dot_product = vec1.dot(&vec2);  // Scalar result

// Vector outer product
let outer = vec1.outer(&vec2);      // [100, 100] matrix

// Matrix decompositions
let matrix: Tensor<f64, [5, 5]> = Tensor::random();

// LU decomposition
let (l, u, p) = matrix.lu_decomposition();

// QR decomposition  
let (q, r) = matrix.qr_decomposition();

// SVD decomposition
let (u, s, vt) = matrix.svd();

// Eigendecomposition
let (eigenvalues, eigenvectors) = matrix.eigen();

// Cholesky decomposition (for positive definite matrices)
let positive_def = matrix @ matrix.transpose() + Tensor::eye::<5>() * 0.1;
let cholesky = positive_def.cholesky();
```

### Reduction Operations ✅ IMPLEMENTED

```neuro
let tensor: Tensor<f32, [3, 4, 5]> = Tensor::random();

// Sum reductions
let sum_all = tensor.sum();          // Sum all elements -> scalar  
let sum_axis0 = tensor.sum(0);       // Sum along axis 0 -> [4, 5]
let sum_axis1 = tensor.sum(1);       // Sum along axis 1 -> [3, 5]
let sum_axes = tensor.sum(&[0, 2]);  // Sum along axes 0 and 2 -> [4]

// Other reduction operations
let mean = tensor.mean(1);           // Mean along axis 1
let std = tensor.std(0);             // Standard deviation along axis 0
let var = tensor.var(2);             // Variance along axis 2
let min = tensor.min(0);             // Minimum along axis 0
let max = tensor.max(1);             // Maximum along axis 1
let argmin = tensor.argmin(0);       // Index of minimum along axis 0
let argmax = tensor.argmax(1);       // Index of maximum along axis 1

// Product operations
let prod = tensor.prod(0);           // Product along axis 0
let cumsum = tensor.cumsum(1);       // Cumulative sum along axis 1
let cumprod = tensor.cumprod(2);     // Cumulative product along axis 2

// Norm operations
let l1_norm = tensor.norm(1);        // L1 norm (Manhattan distance)
let l2_norm = tensor.norm(2);        // L2 norm (Euclidean distance)  
let frobenius = tensor.norm("fro");  // Frobenius norm
let nuclear = tensor.norm("nuc");    // Nuclear norm

// Keep dimensions (don't reduce)
let mean_keepdim = tensor.mean_keepdim(1);  // [3, 1, 5] instead of [3, 5]
```

### Reshaping and Manipulation ✅ IMPLEMENTED

```neuro
let tensor: Tensor<f32, [2, 3, 4]> = Tensor::random();

// Reshaping
let reshaped: Tensor<f32, [6, 4]> = tensor.reshape([6, 4]);
let flattened: Tensor<f32, [24]> = tensor.flatten();
let view: Tensor<f32, [8, 3]> = tensor.view([8, 3]);  // Alias for reshape

// Transposition
let transposed = tensor.transpose();                    // Reverse all dimensions
let swapped = tensor.transpose(0, 2);                  // Swap dimensions 0 and 2
let permuted = tensor.permute([2, 0, 1]);             // Reorder dimensions

// Dimension manipulation
let unsqueezed: Tensor<f32, [2, 1, 3, 4]> = tensor.unsqueeze(1);  // Add dim at index 1
let squeezed: Tensor<f32, [2, 3, 4]> = unsqueezed.squeeze(1);     // Remove dim at index 1

// Splitting and joining
let chunks = tensor.chunk(2, axis: 0);                 // Split into 2 equal chunks along axis 0
let (a, b) = tensor.split([1, 2], axis: 0);           // Split into sizes [1, 2] along axis 0

// Concatenation  
let cat_dim0 = Tensor::cat(&[tensor, tensor], axis: 0); // [4, 3, 4]
let cat_dim1 = Tensor::cat(&[tensor, tensor], axis: 1); // [2, 6, 4]

// Stacking (adds new dimension)
let stacked = Tensor::stack(&[tensor, tensor], axis: 0); // [2, 2, 3, 4]

// Repeating
let repeated = tensor.repeat([2, 1, 3]);               // [4, 3, 12] - repeat elements
let tiled = tensor.tile([2, 1, 3]);                    // [4, 3, 12] - tile tensor
```

### Advanced Indexing ✅ IMPLEMENTED

```neuro
let tensor: Tensor<f32, [5, 4, 3]> = Tensor::random();

// Basic indexing
let element = tensor[2, 1, 0];                         // Single element
let slice = tensor[1..4, :, 0];                        // Slice notation: [3, 4]

// Boolean indexing
let mask: Tensor<bool, [5, 4, 3]> = tensor > 0.5;
let masked_values = tensor.masked_select(&mask);       // 1D tensor of selected values
let masked_tensor = tensor.masked_fill(&mask, 0.0);    // Replace masked values with 0.0

// Advanced indexing with tensors
let row_indices: Tensor<i64, [3]> = tensor![0, 2, 4];
let col_indices: Tensor<i64, [3]> = tensor![1, 3, 2];
let selected = tensor.gather(0, &row_indices);         // Select rows [0, 2, 4]

// Multi-dimensional gathering
let indices: Tensor<i64, [2, 3]> = tensor![[0, 1, 2], [2, 3, 1]];
let gathered = tensor.gather(1, &indices);

// Scatter operations (inverse of gather)
let mut target: Tensor<f32, [5, 4, 3]> = Tensor::zeros();
let values: Tensor<f32, [2, 4, 3]> = Tensor::ones();
let indices: Tensor<i64, [2]> = tensor![1, 3];
target.scatter_(0, &indices, &values);                 // In-place scatter

// Take operations
let indices_1d: Tensor<i64, [6]> = tensor![0, 5, 10, 15, 20, 25];
let taken = tensor.flatten().take(&indices_1d);        // Take elements at flat indices
```

### Signal Processing ✅ IMPLEMENTED (Basic) / 📅 PLANNED (Advanced)

```neuro
// FFT operations (📅 Planned - Phase 2)
let signal: Tensor<f32, [1024]> = Tensor::random();
let fft_result = signal.fft();                         // 1D FFT
let ifft_result = fft_result.ifft();                   // Inverse FFT

// 2D FFT for images
let image: Tensor<f32, [256, 256]> = Tensor::random();
let fft_2d = image.fft2();
let ifft_2d = fft_2d.ifft2();

// Convolution operations ✅ IMPLEMENTED
let input: Tensor<f32, [1, 28, 28]> = Tensor::random();    // [channels, height, width]
let kernel: Tensor<f32, [16, 1, 5, 5]> = Tensor::random(); // [out_ch, in_ch, h, w]

let conv_result = input.conv2d(&kernel, 
    stride: [1, 1], 
    padding: [2, 2], 
    dilation: [1, 1]
);

// Pooling operations
let max_pooled = input.max_pool2d(kernel_size: [2, 2], stride: [2, 2]);
let avg_pooled = input.avg_pool2d(kernel_size: [2, 2], stride: [2, 2]);
let adaptive_pooled = input.adaptive_avg_pool2d([7, 7]); // Adaptive pooling to fixed size
```

---

## Memory Layout

### Memory Layout Control ✅ IMPLEMENTED

```neuro
// Row-major (C-style) layout - default
let row_major: Tensor<f32, [100, 200], RowMajor> = Tensor::zeros();

// Column-major (Fortran-style) layout  
let col_major: Tensor<f32, [100, 200], ColumnMajor> = Tensor::zeros();

// Memory layout queries
let is_contiguous = tensor.is_contiguous();            // Check if memory is contiguous
let is_row_major = tensor.is_row_major();              // Check layout
let is_col_major = tensor.is_column_major();

// Force contiguous memory layout
let contiguous = tensor.contiguous();                  // Ensure contiguous memory
let as_row_major = tensor.as_row_major();              // Convert to row-major
let as_col_major = tensor.as_column_major();           // Convert to column-major

// Stride information
let strides = tensor.strides();                        // Get memory strides
let byte_strides = tensor.byte_strides();              // Strides in bytes
```

### SIMD Alignment ✅ IMPLEMENTED

```neuro
// Tensors are automatically SIMD-aligned for performance
let aligned_tensor: Tensor<f32, [1000]> = Tensor::zeros();

// Check alignment
let is_aligned = tensor.is_simd_aligned();             // Check for SIMD alignment
let alignment = tensor.memory_alignment();             // Get memory alignment in bytes

// Force specific alignment
let aligned_32: Tensor<f32, [1000]> = Tensor::zeros_aligned(32);  // 32-byte alignment
let aligned_64: Tensor<f32, [1000]> = Tensor::zeros_aligned(64);  // 64-byte alignment
```

### Memory Pools ✅ IMPLEMENTED

```neuro
import std::memory::TensorPool;

// Use memory pool for tensor allocation
#[memory(pool = "training_pool")]
fn create_training_tensors() -> Vec<Tensor<f32, [1024, 1024]>> {
    vec![
        Tensor::zeros(),  // Allocated from pool
        Tensor::ones(),   // Allocated from pool
        Tensor::random()  // Allocated from pool
    ]
    // Memory automatically returned to pool when function ends
}

// Manual memory pool management
fn training_loop() {
    let mut pool = TensorPool::with_capacity(1024 * 1024 * 1024); // 1GB pool
    
    for epoch in 0..100 {
        pool.reset();  // Clear pool for next epoch
        
        // All tensor allocations use the pool
        let batch = pool.allocate_tensor([32, 784]);
        let weights = pool.allocate_tensor([784, 256]);
        let activations = pool.allocate_tensor([32, 256]);
        
        // Training step...
        
        // Memory automatically reused in next iteration
    }
}
```

---

## Performance Optimization

### Vectorization ✅ IMPLEMENTED

```neuro
// Automatic vectorization for element-wise operations
#[vectorize]
fn element_wise_computation(a: &Tensor<f32>, b: &Tensor<f32>) -> Tensor<f32> {
    // Automatically vectorized using SIMD instructions
    a * a + b * b + (a * b).sin()
}

// Manual SIMD hints
#[target_feature(enable = "avx2")]
fn avx2_optimized(data: &Tensor<f32>) -> Tensor<f32> {
    // Use AVX2 instructions when available
    data.exp()
}
```

### Parallel Operations ✅ IMPLEMENTED

```neuro
// Automatic parallelization
#[parallel]
fn parallel_tensor_op(tensors: &[Tensor<f32>]) -> Vec<Tensor<f32>> {
    // Automatically parallelized across CPU cores
    tensors.iter()
        .map(|t| t.relu())
        .collect()
}

// Control parallelization
fn custom_parallel() {
    // Set number of threads for tensor operations
    Tensor::set_num_threads(8);
    
    let large_tensor: Tensor<f32, [10000, 10000]> = Tensor::random();
    let result = large_tensor.matmul(&large_tensor.transpose()); // Uses 8 threads
}
```

### Memory-Efficient Operations ✅ IMPLEMENTED

```neuro
// In-place operations to reduce memory usage
let mut tensor: Tensor<f32, [1000, 1000]> = Tensor::random();

// In-place arithmetic (modifies original tensor)
tensor += 1.0;                    // In-place addition
tensor *= 0.5;                    // In-place multiplication
tensor.relu_();                   // In-place ReLU activation
tensor.clamp_(min: 0.0, max: 1.0); // In-place clamping

// Views instead of copies
let view = tensor.view([500, 2000]);     // View, no memory copy
let slice = tensor.slice(100..900, ..);  // Slice view, no copy

// Copy-on-write semantics
let shared = tensor.clone();             // Shallow copy (shares memory)
let modified = shared + 1.0;             // Creates new tensor only when needed
```

### GPU Acceleration (📅 Planned - Phase 2)

```neuro
// Move tensors to GPU
let cpu_tensor: Tensor<f32, [1024, 1024]> = Tensor::random();
let gpu_tensor = cpu_tensor.cuda();      // Move to CUDA GPU
let vulkan_tensor = cpu_tensor.vulkan(); // Move to Vulkan GPU

// GPU operations
let gpu_result = gpu_tensor @ gpu_tensor.transpose(); // GPU matrix multiplication
let cpu_result = gpu_result.cpu();                    // Move back to CPU

// Multi-GPU operations
let distributed = cpu_tensor.distribute_across_gpus(&[0, 1, 2, 3]); // 4 GPUs
let aggregated = distributed.gather();                              // Collect results
```

---

## Interoperability

### NumPy Compatibility (📅 Planned - Phase 3)

```neuro
import std::interop::numpy;

// Convert NEURO tensor to NumPy array
let neuro_tensor: Tensor<f32, [100, 100]> = Tensor::random();
let numpy_array = neuro_tensor.to_numpy();

// Convert NumPy array to NEURO tensor
let from_numpy: Tensor<f32, [100, 100]> = Tensor::from_numpy(numpy_array);

// Zero-copy conversion (when possible)
let zero_copy_view = Tensor::from_numpy_view(numpy_array);
```

### PyTorch Interoperability (📅 Planned - Phase 3)

```neuro
import std::interop::torch;

// Convert to PyTorch tensor
let torch_tensor = neuro_tensor.to_torch();

// Convert from PyTorch tensor  
let from_torch: Tensor<f32, [100, 100]> = Tensor::from_torch(torch_tensor);

// Use PyTorch models in NEURO
let pytorch_model = torch::load_model("model.pt");
let neuro_input: Tensor<f32, [1, 3, 224, 224]> = load_image();
let output = pytorch_model.forward(neuro_input.to_torch()).to_neuro();
```

### TensorFlow Compatibility (📅 Planned - Phase 3)

```neuro
import std::interop::tensorflow as tf;

// Convert to TensorFlow tensor
let tf_tensor = neuro_tensor.to_tensorflow();

// Load TensorFlow SavedModel
let tf_model = tf::load_saved_model("saved_model/");
let predictions = tf_model.predict(neuro_tensor.to_tensorflow()).to_neuro();
```

### ONNX Support (📅 Planned - Phase 3)

```neuro
import std::interop::onnx;

// Export model to ONNX
let neuro_model = create_model();
onnx::export_model(&neuro_model, "model.onnx")?;

// Load ONNX model
let onnx_model = onnx::load_model("external_model.onnx")?;
let output = onnx_model.run(input_tensor);
```

### C/C++ Integration ✅ IMPLEMENTED

```neuro
// FFI with BLAS libraries
extern "C" {
    fn cblas_sgemm(
        order: i32, transa: i32, transb: i32,
        m: i32, n: i32, k: i32,
        alpha: f32, a: *const f32, lda: i32,
        b: *const f32, ldb: i32,  
        beta: f32, c: *mut f32, ldc: i32
    );
}

// Use BLAS for matrix multiplication
impl Tensor<f32, [M, N]> {
    fn matmul_blas(&self, other: &Tensor<f32, [N, K]>) -> Tensor<f32, [M, K]> {
        let mut result = Tensor::zeros();
        
        unsafe {
            cblas_sgemm(
                101, // Row-major order
                111, 111, // No transpose
                M as i32, K as i32, N as i32,
                1.0, self.data_ptr(), N as i32,
                other.data_ptr(), K as i32,
                0.0, result.data_mut_ptr(), K as i32
            );
        }
        
        result
    }
}
```

This comprehensive tensor documentation covers all aspects of tensor programming in NEURO, from basic creation and operations to advanced features and performance optimization. The implementation status indicators help developers understand what features are currently available versus planned for future releases.