# NEURO Standard Library Reference

The NEURO standard library provides a comprehensive set of modules and functions designed specifically for AI/ML workloads, while also supporting general-purpose programming.

## Table of Contents

1. [Core Modules](#core-modules)
2. [Tensor Operations](#tensor-operations)
3. [Machine Learning](#machine-learning)
4. [Neural Networks](#neural-networks)
5. [GPU Computing](#gpu-computing)
6. [Collections](#collections)
7. [I/O and Filesystem](#io-and-filesystem)
8. [Concurrency](#concurrency)
9. [Mathematics](#mathematics)
10. [String Processing](#string-processing)

---

## Core Modules

### std::prelude ✅ IMPLEMENTED

Automatically imported items available in all NEURO programs:

```neuro
// Automatically available - no import needed

// Core types
Option<T>      // Nullable values
Result<T, E>   // Error handling
Vec<T>         // Dynamic arrays
String         // Owned strings
&str           // String slices

// Core traits
Clone          // Cloneable types
Copy           // Copyable types  
Debug          // Debug formatting
Display        // Display formatting
Default        // Default values
PartialEq      // Partial equality
Eq             // Full equality
PartialOrd     // Partial ordering
Ord            // Full ordering
Hash           // Hashable types

// Core functions
print!()       // Print to stdout
println!()     // Print line to stdout
eprint!()      // Print to stderr
eprintln!()    // Print line to stderr
format!()      // Format string
panic!()       // Program panic
assert!()      // Assertion
debug_assert!()// Debug assertion
todo!()        // TODO marker
unimplemented!() // Unimplemented marker
unreachable!() // Unreachable code marker
```

### std::mem ✅ IMPLEMENTED

Memory utilities and operations:

```neuro
import std::mem;

// Memory operations
fn size_of<T>() -> usize               // Size of type T in bytes
fn align_of<T>() -> usize              // Alignment of type T in bytes
fn size_of_val<T>(val: &T) -> usize   // Size of value in bytes
fn align_of_val<T>(val: &T) -> usize  // Alignment of value in bytes

fn swap<T>(x: &mut T, y: &mut T)       // Swap two values
fn replace<T>(dest: &mut T, src: T) -> T // Replace value, return old
fn take<T: Default>(dest: &mut T) -> T  // Take value, replace with default

// Memory safety
fn drop<T>(x: T)                       // Explicitly drop value
fn forget<T>(x: T)                     // Leak memory (disable drop)

// Discriminant access for enums  
fn discriminant<T>(v: &T) -> Discriminant<T>

// Examples
let size = mem::size_of::<f64>();      // 8 bytes
let align = mem::align_of::<i32>();    // 4 bytes

let mut a = 5;
let mut b = 10;
mem::swap(&mut a, &mut b);             // a = 10, b = 5

let mut x = String::from("hello");
let old = mem::replace(&mut x, String::from("world")); // old = "hello", x = "world"
```

### std::ptr ✅ IMPLEMENTED (Infrastructure)

Raw pointer operations for unsafe code:

```neuro
import std::ptr;

// Pointer creation
fn null<T>() -> *const T               // Null pointer
fn null_mut<T>() -> *mut T             // Null mutable pointer
fn dangling<T>() -> *const T           // Dangling pointer (non-null but invalid)

// Pointer operations  
fn read<T>(src: *const T) -> T         // Read from pointer
fn write<T>(dst: *mut T, src: T)       // Write to pointer
fn copy<T>(src: *const T, dst: *mut T, count: usize) // Copy memory
fn copy_nonoverlapping<T>(src: *const T, dst: *mut T, count: usize) // Non-overlapping copy

// Pointer arithmetic
fn offset<T>(ptr: *const T, count: isize) -> *const T // Offset pointer
fn add<T>(ptr: *const T, count: usize) -> *const T    // Add to pointer
fn sub<T>(ptr: *const T, count: usize) -> *const T    // Subtract from pointer

// Pointer comparisons
fn eq<T>(a: *const T, b: *const T) -> bool            // Pointer equality

// Examples (unsafe code)
unsafe {
    let data = [1, 2, 3, 4, 5];
    let ptr = data.as_ptr();
    
    let first = ptr::read(ptr);            // Read first element: 1
    let third = ptr::read(ptr.add(2));     // Read third element: 3
    
    let mut buffer = [0; 5];
    ptr::copy_nonoverlapping(ptr, buffer.as_mut_ptr(), 5);
}
```

---

## Tensor Operations

### std::tensor ✅ IMPLEMENTED

Core tensor types and operations:

```neuro
import std::tensor::{Tensor, DynamicTensor, TensorError};

// Tensor types
struct Tensor<T, const SHAPE: &'static [usize]> // Static tensor
struct DynamicTensor<T>                          // Dynamic tensor
struct SparseTensor<T>                           // Sparse tensor
struct MaskedTensor<T, const SHAPE: &'static [usize]> // Masked tensor

// Tensor creation
impl<T, const SHAPE: &'static [usize]> Tensor<T, SHAPE> {
    fn zeros() -> Self                           // Zero tensor
    fn ones() -> Self                            // Ones tensor  
    fn full(value: T) -> Self                    // Filled tensor
    fn random() -> Self                          // Random tensor
    fn eye() -> Self                             // Identity matrix
    fn from_array(data: [T; N]) -> Self         // From array
    fn from_vec(data: Vec<T>) -> Self           // From vector
    fn from_fn<F>(f: F) -> Self                 // From function
    where F: Fn(&[usize]) -> T
}

// Tensor operations
impl<T, const SHAPE: &'static [usize]> Tensor<T, SHAPE> {
    // Shape and properties
    fn shape(&self) -> &[usize]                  // Tensor shape
    fn ndim(&self) -> usize                      // Number of dimensions
    fn size(&self) -> usize                      // Total elements
    fn is_empty(&self) -> bool                   // Check if empty
    fn is_contiguous(&self) -> bool              // Check memory layout
    
    // Indexing and slicing
    fn get(&self, indices: &[usize]) -> Option<&T>
    fn get_mut(&mut self, indices: &[usize]) -> Option<&mut T>
    fn slice<R>(&self, ranges: R) -> TensorView<T>
    
    // Arithmetic operations
    fn add(&self, other: &Self) -> Self          // Element-wise addition
    fn sub(&self, other: &Self) -> Self          // Element-wise subtraction  
    fn mul(&self, other: &Self) -> Self          // Element-wise multiplication
    fn div(&self, other: &Self) -> Self          // Element-wise division
    fn matmul(&self, other: &Self) -> Self       // Matrix multiplication (@)
    
    // Scalar operations
    fn add_scalar(&self, scalar: T) -> Self      // Add scalar
    fn mul_scalar(&self, scalar: T) -> Self      // Multiply by scalar
    
    // Reductions
    fn sum(&self) -> T                           // Sum all elements
    fn sum_axis(&self, axis: usize) -> Tensor<T, NEW_SHAPE>
    fn mean(&self) -> T                          // Mean of all elements
    fn mean_axis(&self, axis: usize) -> Tensor<T, NEW_SHAPE>
    fn min(&self) -> T                           // Minimum element
    fn max(&self) -> T                           // Maximum element
    fn argmin(&self) -> usize                    // Index of minimum
    fn argmax(&self) -> usize                    // Index of maximum
    
    // Shape operations
    fn reshape<const NEW_SHAPE: &'static [usize]>(&self) -> Tensor<T, NEW_SHAPE>
    fn transpose(&self) -> Tensor<T, TRANSPOSED_SHAPE>
    fn permute(&self, dims: &[usize]) -> Tensor<T, PERMUTED_SHAPE>
    fn squeeze(&self) -> Tensor<T, SQUEEZED_SHAPE>
    fn unsqueeze(&self, dim: usize) -> Tensor<T, UNSQUEEZED_SHAPE>
    
    // Element-wise functions
    fn abs(&self) -> Self                        // Absolute value
    fn sqrt(&self) -> Self                       // Square root  
    fn exp(&self) -> Self                        // Exponential
    fn ln(&self) -> Self                         // Natural logarithm
    fn sin(&self) -> Self                        // Sine
    fn cos(&self) -> Self                        // Cosine
    fn tanh(&self) -> Self                       // Hyperbolic tangent
    fn relu(&self) -> Self                       // ReLU activation
    fn sigmoid(&self) -> Self                    // Sigmoid activation
}

// Examples
let a: Tensor<f32, [3, 3]> = Tensor::eye();    // 3x3 identity matrix
let b: Tensor<f32, [3, 3]> = Tensor::random(); // 3x3 random matrix
let c = a @ b;                                  // Matrix multiplication
let sum = c.sum();                              // Sum all elements
let transposed = c.transpose();                 // Transpose matrix
let flattened = c.reshape::<[9]>();            // Flatten to 1D
```

### std::tensor::random ✅ IMPLEMENTED

Random tensor generation:

```neuro
import std::tensor::random::{Normal, Uniform, Xavier, He};

// Distribution types
struct Normal { mean: f64, std: f64 }
struct Uniform { low: f64, high: f64 }
struct Xavier;
struct He;

// Random tensor creation
impl<T, const SHAPE: &'static [usize]> Tensor<T, SHAPE> {
    fn random() -> Self                          // Uniform [0, 1)
    fn random_normal() -> Self                   // Normal N(0, 1)
    fn random_uniform(low: T, high: T) -> Self   // Uniform [low, high)
    fn xavier_uniform() -> Self                  // Xavier initialization
    fn xavier_normal() -> Self                   // Xavier normal initialization
    fn he_uniform() -> Self                      // He initialization  
    fn he_normal() -> Self                       // He normal initialization
    
    fn random_with_distribution<D: Distribution<T>>(dist: D) -> Self
    fn random_with_seed(seed: u64) -> Self       // With specific seed
}

// Examples
let uniform: Tensor<f32, [100, 100]> = Tensor::random();
let normal: Tensor<f32, [784, 256]> = Tensor::random_normal();
let xavier: Tensor<f32, [256, 128]> = Tensor::xavier_uniform();
let he: Tensor<f32, [128, 64]> = Tensor::he_normal();

// Custom distributions
let custom_normal = Normal { mean: 0.5, std: 0.1 };
let custom_tensor = Tensor::random_with_distribution(custom_normal);
```

---

## Machine Learning

### std::ml ✅ IMPLEMENTED (Infrastructure)

Core ML functionality and traits:

```neuro
import std::ml::{Model, Loss, Optimizer, Dataset};

// Model trait - implemented by all neural networks
trait Model {
    type Input;
    type Output;
    
    fn forward(&self, input: Self::Input) -> Self::Output;
    fn parameters(&self) -> Vec<&Tensor<f32>>;
    fn parameters_mut(&mut self) -> Vec<&mut Tensor<f32>>;
    fn zero_gradients(&mut self);
    fn train_mode(&mut self);
    fn eval_mode(&mut self);
}

// Loss function trait
trait Loss<Input, Target> {
    type Output;
    
    fn compute(&self, predictions: Input, targets: Target) -> Self::Output;
    fn backward(&self) -> Gradients;
}

// Optimizer trait
trait Optimizer {
    fn step(&mut self, parameters: &mut [Parameter]);
    fn zero_grad(&mut self);
    fn learning_rate(&self) -> f64;
    fn set_learning_rate(&mut self, lr: f64);
}

// Dataset trait for data loading
trait Dataset {
    type Item;
    
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<Self::Item>;
    fn shuffle(&mut self);
    fn split(self, ratio: f64) -> (Self, Self);
}

// Training utilities
struct TrainingConfig {
    epochs: usize,
    batch_size: usize,
    learning_rate: f64,
    device: Device,
}

fn train_model<M, D, L, O>(
    model: &mut M,
    dataset: D,
    loss_fn: L,
    optimizer: &mut O,
    config: TrainingConfig
) -> TrainingResults
where
    M: Model,
    D: Dataset,
    L: Loss<M::Output, D::Target>,
    O: Optimizer;
```

### std::ml::losses ✅ IMPLEMENTED (Infrastructure)

Common loss functions:

```neuro
import std::ml::losses::*;

// Classification losses
struct CrossEntropyLoss;
struct BinaryCrossEntropyLoss;
struct SparseCrossEntropyLoss;
struct FocalLoss { alpha: f32, gamma: f32 }
struct HingeLoss { margin: f32 }

// Regression losses  
struct MeanSquaredError;
struct MeanAbsoluteError;
struct HuberLoss { delta: f32 }
struct LogCoshLoss;

// Advanced losses
struct ContrastiveLoss { margin: f32 }
struct TripletLoss { margin: f32 }
struct CenterLoss { num_classes: usize, alpha: f32 }

// Implementation examples
impl Loss<Tensor<f32, [N, C]>, Tensor<i64, [N]>> for CrossEntropyLoss {
    type Output = Tensor<f32, []>;  // Scalar loss
    
    fn compute(&self, predictions: Tensor<f32, [N, C]>, targets: Tensor<i64, [N]>) -> Self::Output {
        let log_probs = predictions.log_softmax(dim: 1);
        let gathered = log_probs.gather(1, &targets.unsqueeze(1));
        -gathered.mean()
    }
}

impl Loss<Tensor<f32, [N]>, Tensor<f32, [N]>> for MeanSquaredError {
    type Output = Tensor<f32, []>;
    
    fn compute(&self, predictions: Tensor<f32, [N]>, targets: Tensor<f32, [N]>) -> Self::Output {
        (predictions - targets).pow(2).mean()
    }
}

// Usage
let mse = MeanSquaredError;
let predictions: Tensor<f32, [32]> = model.forward(inputs);  
let targets: Tensor<f32, [32]> = load_targets();
let loss = mse.compute(predictions, targets);

let cross_entropy = CrossEntropyLoss;
let logits: Tensor<f32, [32, 10]> = classifier.forward(inputs);
let labels: Tensor<i64, [32]> = load_labels();
let classification_loss = cross_entropy.compute(logits, labels);
```

### std::ml::optimizers ✅ IMPLEMENTED (Infrastructure)

Optimization algorithms:

```neuro
import std::ml::optimizers::*;

// Basic optimizers
struct SGD { 
    learning_rate: f64, 
    momentum: f64, 
    weight_decay: f64,
    nesterov: bool 
}

struct Adam {
    learning_rate: f64,
    beta1: f64,          // Default: 0.9
    beta2: f64,          // Default: 0.999  
    eps: f64,            // Default: 1e-8
    weight_decay: f64,   // Default: 0.0
    amsgrad: bool        // Default: false
}

struct AdamW {
    learning_rate: f64,
    beta1: f64,
    beta2: f64,
    eps: f64,
    weight_decay: f64,
    correct_bias: bool
}

struct RMSprop {
    learning_rate: f64,
    alpha: f64,          // Decay rate
    eps: f64,
    weight_decay: f64,
    momentum: f64,
    centered: bool
}

// Advanced optimizers
struct AdaGrad {
    learning_rate: f64,
    eps: f64,
    weight_decay: f64
}

struct Adadelta {
    rho: f64,            // Decay rate
    eps: f64,
    weight_decay: f64
}

struct LAMB {
    learning_rate: f64,
    beta1: f64,
    beta2: f64,
    eps: f64,
    weight_decay: f64,
    trust_clip: bool
}

// Learning rate schedulers
trait LRScheduler {
    fn get_lr(&self, step: usize) -> f64;
    fn step(&mut self);
}

struct StepLR { 
    initial_lr: f64, 
    step_size: usize, 
    gamma: f64 
}

struct ExponentialLR { 
    initial_lr: f64, 
    gamma: f64 
}

struct CosineAnnealingLR { 
    initial_lr: f64, 
    max_steps: usize,
    eta_min: f64 
}

struct ReduceLROnPlateau {
    factor: f64,
    patience: usize,
    threshold: f64,
    min_lr: f64
}

// Usage examples
let mut adam = Adam {
    learning_rate: 0.001,
    beta1: 0.9,
    beta2: 0.999,
    eps: 1e-8,
    weight_decay: 0.01,
    amsgrad: false
};

let mut scheduler = CosineAnnealingLR {
    initial_lr: 0.001,
    max_steps: 1000,
    eta_min: 1e-6
};

// Training loop
for epoch in 0..num_epochs {
    for batch in dataloader {
        let predictions = model.forward(&batch.inputs);
        let loss = loss_fn.compute(predictions, &batch.targets);
        
        model.zero_gradients();
        let gradients = loss.backward();
        adam.step(model.parameters_mut(), &gradients);
        
        scheduler.step();
        adam.set_learning_rate(scheduler.get_lr(epoch));
    }
}
```

---

## Neural Networks

### std::ml::layers ✅ IMPLEMENTED (Infrastructure)

Common neural network layers:

```neuro
import std::ml::layers::*;

// Linear layers
struct Dense<const INPUT: usize, const OUTPUT: usize> {
    weights: Tensor<f32, [INPUT, OUTPUT]>,
    bias: Tensor<f32, [OUTPUT]>,
    use_bias: bool
}

struct Linear<const INPUT: usize, const OUTPUT: usize>(Dense<INPUT, OUTPUT>);

// Convolutional layers
struct Conv1D<const IN_CH: usize, const OUT_CH: usize> {
    weights: Tensor<f32, [OUT_CH, IN_CH, KERNEL_SIZE]>,
    bias: Tensor<f32, [OUT_CH]>,
    stride: usize,
    padding: usize,
    dilation: usize
}

struct Conv2D<const IN_CH: usize, const OUT_CH: usize> {
    weights: Tensor<f32, [OUT_CH, IN_CH, KERNEL_H, KERNEL_W]>,
    bias: Tensor<f32, [OUT_CH]>,
    stride: [usize; 2],
    padding: [usize; 2],
    dilation: [usize; 2],
    groups: usize
}

struct Conv3D<const IN_CH: usize, const OUT_CH: usize> {
    weights: Tensor<f32, [OUT_CH, IN_CH, KERNEL_D, KERNEL_H, KERNEL_W]>,
    bias: Tensor<f32, [OUT_CH]>,
    stride: [usize; 3],
    padding: [usize; 3],
    dilation: [usize; 3]
}

// Activation layers
struct ReLU;
struct LeakyReLU { negative_slope: f32 }
struct ELU { alpha: f32 }
struct SELU;
struct GELU;
struct Swish;
struct Mish;
struct Sigmoid;
struct Tanh;
struct Softmax { dim: usize }
struct LogSoftmax { dim: usize }

// Normalization layers
struct BatchNorm1D<const FEATURES: usize> {
    running_mean: Tensor<f32, [FEATURES]>,
    running_var: Tensor<f32, [FEATURES]>,
    weight: Tensor<f32, [FEATURES]>,
    bias: Tensor<f32, [FEATURES]>,
    eps: f32,
    momentum: f32,
    affine: bool,
    track_running_stats: bool
}

struct BatchNorm2D<const FEATURES: usize> {
    running_mean: Tensor<f32, [FEATURES]>,
    running_var: Tensor<f32, [FEATURES]>,
    weight: Tensor<f32, [FEATURES]>,
    bias: Tensor<f32, [FEATURES]>,
    eps: f32,
    momentum: f32
}

struct LayerNorm<const FEATURES: usize> {
    weight: Tensor<f32, [FEATURES]>,
    bias: Tensor<f32, [FEATURES]>,
    eps: f32,
    elementwise_affine: bool
}

struct GroupNorm<const CHANNELS: usize> {
    num_groups: usize,
    weight: Tensor<f32, [CHANNELS]>,
    bias: Tensor<f32, [CHANNELS]>,
    eps: f32,
    affine: bool
}

// Pooling layers
struct MaxPool1D { kernel_size: usize, stride: usize, padding: usize }
struct MaxPool2D { kernel_size: [usize; 2], stride: [usize; 2], padding: [usize; 2] }
struct AvgPool1D { kernel_size: usize, stride: usize, padding: usize }
struct AvgPool2D { kernel_size: [usize; 2], stride: [usize; 2], padding: [usize; 2] }
struct AdaptiveAvgPool1D<const OUTPUT_SIZE: usize>;
struct AdaptiveAvgPool2D<const OUTPUT_H: usize, const OUTPUT_W: usize>;
struct GlobalAvgPool;
struct GlobalMaxPool;

// Regularization layers
struct Dropout { p: f32, training: bool }
struct Dropout2D { p: f32, training: bool }
struct AlphaDropout { p: f32, training: bool }

// Recurrent layers
struct LSTM<const INPUT_SIZE: usize, const HIDDEN_SIZE: usize> {
    weight_ih: Tensor<f32, [4 * HIDDEN_SIZE, INPUT_SIZE]>,
    weight_hh: Tensor<f32, [4 * HIDDEN_SIZE, HIDDEN_SIZE]>,
    bias_ih: Tensor<f32, [4 * HIDDEN_SIZE]>,
    bias_hh: Tensor<f32, [4 * HIDDEN_SIZE]>,
    num_layers: usize,
    batch_first: bool,
    dropout: f32,
    bidirectional: bool
}

struct GRU<const INPUT_SIZE: usize, const HIDDEN_SIZE: usize> {
    weight_ih: Tensor<f32, [3 * HIDDEN_SIZE, INPUT_SIZE]>,
    weight_hh: Tensor<f32, [3 * HIDDEN_SIZE, HIDDEN_SIZE]>,
    bias_ih: Tensor<f32, [3 * HIDDEN_SIZE]>,
    bias_hh: Tensor<f32, [3 * HIDDEN_SIZE]>,
    num_layers: usize,
    batch_first: bool,
    dropout: f32,
    bidirectional: bool
}

// Attention mechanisms
struct MultiHeadAttention<const D_MODEL: usize, const NUM_HEADS: usize> {
    q_linear: Dense<D_MODEL, D_MODEL>,
    k_linear: Dense<D_MODEL, D_MODEL>,
    v_linear: Dense<D_MODEL, D_MODEL>,
    out_linear: Dense<D_MODEL, D_MODEL>,
    dropout: Dropout,
    scale: f32
}

struct SelfAttention<const D_MODEL: usize> {
    attention: MultiHeadAttention<D_MODEL, 8>
}

// Layer implementations
impl<const INPUT: usize, const OUTPUT: usize> Layer for Dense<INPUT, OUTPUT> {
    type Input = Tensor<f32, [BATCH_SIZE, INPUT]>;
    type Output = Tensor<f32, [BATCH_SIZE, OUTPUT]>;
    
    fn forward(&self, input: Self::Input) -> Self::Output {
        let output = input @ &self.weights;
        if self.use_bias {
            output + &self.bias
        } else {
            output
        }
    }
}

impl Layer for ReLU {
    type Input = Tensor<f32>;
    type Output = Tensor<f32>;
    
    fn forward(&self, input: Self::Input) -> Self::Output {
        input.clamp_min(0.0)
    }
}

// Usage example
let dense1 = Dense::<784, 256> {
    weights: Tensor::xavier_uniform(),
    bias: Tensor::zeros(),
    use_bias: true
};

let relu = ReLU;
let dropout = Dropout { p: 0.5, training: true };
let output_layer = Dense::<256, 10>::new();

// Forward pass
let x: Tensor<f32, [32, 784]> = load_batch();
let h1 = relu.forward(dense1.forward(x));
let h2 = dropout.forward(h1);
let logits = output_layer.forward(h2);
```

### std::ml::models ✅ IMPLEMENTED (Infrastructure)

Pre-built model architectures:

```neuro
import std::ml::models::*;

// Multi-Layer Perceptron
struct MLP {
    layers: Vec<Box<dyn Layer>>,
    dropout: f32
}

impl MLP {
    fn new(layer_sizes: &[usize], activation: ActivationType, dropout: f32) -> Self;
    fn add_layer<L: Layer + 'static>(&mut self, layer: L);
}

// Convolutional Neural Networks
struct LeNet5 {
    conv1: Conv2D<1, 6>,
    conv2: Conv2D<6, 16>, 
    fc1: Dense<400, 120>,
    fc2: Dense<120, 84>,
    fc3: Dense<84, 10>
}

struct AlexNet {
    features: Sequential<[
        Conv2D<3, 64>, ReLU, MaxPool2D,
        Conv2D<64, 192>, ReLU, MaxPool2D,
        Conv2D<192, 384>, ReLU,
        Conv2D<384, 256>, ReLU,
        Conv2D<256, 256>, ReLU, MaxPool2D
    ]>,
    classifier: Sequential<[
        Dropout, Dense<9216, 4096>, ReLU,
        Dropout, Dense<4096, 4096>, ReLU,
        Dense<4096, 1000>
    ]>
}

struct ResNet18 {
    conv1: Conv2D<3, 64>,
    layer1: ResidualBlock<64, 64>,
    layer2: ResidualBlock<64, 128>,
    layer3: ResidualBlock<128, 256>, 
    layer4: ResidualBlock<256, 512>,
    avgpool: AdaptiveAvgPool2D<1, 1>,
    fc: Dense<512, 1000>
}

// Transformer models
struct TransformerEncoder<const D_MODEL: usize, const NUM_HEADS: usize, const NUM_LAYERS: usize> {
    layers: [TransformerEncoderLayer<D_MODEL, NUM_HEADS>; NUM_LAYERS],
    norm: LayerNorm<D_MODEL>
}

struct TransformerDecoder<const D_MODEL: usize, const NUM_HEADS: usize, const NUM_LAYERS: usize> {
    layers: [TransformerDecoderLayer<D_MODEL, NUM_HEADS>; NUM_LAYERS],
    norm: LayerNorm<D_MODEL>
}

struct BERT<const VOCAB_SIZE: usize, const D_MODEL: usize, const NUM_HEADS: usize> {
    embeddings: BertEmbeddings<VOCAB_SIZE, D_MODEL>,
    encoder: TransformerEncoder<D_MODEL, NUM_HEADS, 12>,
    pooler: Dense<D_MODEL, D_MODEL>
}

struct GPT<const VOCAB_SIZE: usize, const D_MODEL: usize, const NUM_HEADS: usize> {
    wte: Embedding<VOCAB_SIZE, D_MODEL>,      // Token embeddings
    wpe: Embedding<1024, D_MODEL>,            // Position embeddings
    blocks: TransformerDecoder<D_MODEL, NUM_HEADS, 12>,
    ln_f: LayerNorm<D_MODEL>,
    head: Dense<D_MODEL, VOCAB_SIZE>
}

// Vision Transformers
struct VisionTransformer<const IMAGE_SIZE: usize, const PATCH_SIZE: usize, const D_MODEL: usize> {
    patch_embed: PatchEmbedding<IMAGE_SIZE, PATCH_SIZE, D_MODEL>,
    pos_embed: Parameter<Tensor<f32, [NUM_PATCHES + 1, D_MODEL]>>,
    cls_token: Parameter<Tensor<f32, [1, D_MODEL]>>,
    transformer: TransformerEncoder<D_MODEL, 12, 12>,
    head: Dense<D_MODEL, 1000>
}

// Usage examples
let mlp = MLP::new(&[784, 512, 256, 10], ActivationType::ReLU, 0.5);

let resnet = ResNet18::pretrained("imagenet")?;
let predictions = resnet.forward(images);

let bert = BERT::<30522, 768, 12>::from_pretrained("bert-base-uncased")?;
let embeddings = bert.forward(input_ids, attention_mask);

let gpt = GPT::<50257, 768, 12>::from_pretrained("gpt2")?;
let logits = gpt.forward(input_ids);
```

---

## GPU Computing

### std::gpu ✅ IMPLEMENTED (Infrastructure) / 📅 PLANNED (Full Implementation - Phase 2)

GPU programming utilities:

```neuro
import std::gpu::*;

// Device management
fn device_count() -> Result<usize, GpuError>;
fn current_device() -> Result<Device, GpuError>;
fn set_device(device_id: usize) -> Result<(), GpuError>;
fn device_synchronize() -> Result<(), GpuError>;

// Memory management
fn allocate<T>(size: usize) -> Result<GpuBuffer<T>, GpuError>;
fn allocate_zeroed<T>(size: usize) -> Result<GpuBuffer<T>, GpuError>;
fn copy_to_device<T>(data: &[T]) -> Result<GpuBuffer<T>, GpuError>;
fn copy_to_host<T>(buffer: &GpuBuffer<T>) -> Result<Vec<T>, GpuError>;

// Kernel execution
fn launch_kernel<K: Kernel>(kernel: K, grid: GridConfig, block: BlockConfig) -> Result<(), GpuError>;

// Stream management
struct Stream(StreamHandle);
impl Stream {
    fn create() -> Result<Self, GpuError>;
    fn synchronize(&self) -> Result<(), GpuError>;
    fn destroy(self) -> Result<(), GpuError>;
}

// Event management  
struct Event(EventHandle);
impl Event {
    fn create() -> Result<Self, GpuError>;
    fn record(&self, stream: &Stream) -> Result<(), GpuError>;
    fn synchronize(&self) -> Result<(), GpuError>;
    fn elapsed_time(&self, end: &Event) -> Result<f32, GpuError>;
}

// Profiling
struct GpuProfiler {
    events: Vec<(String, Event, Event)>
}

impl GpuProfiler {
    fn new() -> Self;
    fn start_timer(&mut self, name: &str) -> Result<(), GpuError>;
    fn end_timer(&mut self, name: &str) -> Result<(), GpuError>;
    fn report(&self) -> String;
}

// Error handling
#[derive(Debug)]
enum GpuError {
    OutOfMemory,
    InvalidDevice,
    KernelLaunchFailed,
    MemoryTransferFailed,
    DeviceNotAvailable,
    DriverVersionMismatch
}

// Usage examples
let device_count = gpu::device_count()?;
gpu::set_device(0)?;

let data = vec![1.0f32; 1000000];
let gpu_data = gpu::copy_to_device(&data)?;

let mut profiler = GpuProfiler::new();
profiler.start_timer("vector_add")?;
launch_vector_add_kernel(&gpu_data)?;
profiler.end_timer("vector_add")?;

let result = gpu::copy_to_host(&gpu_data)?;
print(profiler.report());
```

### std::cuda ✅ IMPLEMENTED (Infrastructure) / 📅 PLANNED (Phase 2)

CUDA-specific functionality:

```neuro
import std::cuda::*;

// CUDA device properties
struct DeviceProperties {
    name: String,
    compute_capability: (i32, i32),
    total_memory: usize,
    multiprocessor_count: i32,
    warp_size: i32,
    max_threads_per_block: i32,
    max_block_dimensions: [i32; 3],
    max_grid_dimensions: [i32; 3],
    memory_clock_rate: i32,
    memory_bus_width: i32,
    l2_cache_size: i32
}

fn get_device_properties(device: i32) -> Result<DeviceProperties, CudaError>;

// CUDA streams
struct CudaStream(cudaStream_t);
impl CudaStream {
    fn create() -> Result<Self, CudaError>;
    fn create_with_flags(flags: u32) -> Result<Self, CudaError>;
    fn synchronize(&self) -> Result<(), CudaError>;
    fn query(&self) -> Result<bool, CudaError>;  // Non-blocking check
}

// CUDA events
struct CudaEvent(cudaEvent_t);
impl CudaEvent {
    fn create() -> Result<Self, CudaError>;
    fn create_with_flags(flags: u32) -> Result<Self, CudaError>;
    fn record(&self, stream: Option<&CudaStream>) -> Result<(), CudaError>;
    fn synchronize(&self) -> Result<(), CudaError>;
    fn elapsed_time(&self, end: &CudaEvent) -> Result<f32, CudaError>;
}

// CUDA memory management
struct CudaMemory<T> {
    ptr: *mut T,
    size: usize,
    device: i32
}

impl<T> CudaMemory<T> {
    fn allocate(count: usize) -> Result<Self, CudaError>;
    fn allocate_managed(count: usize) -> Result<Self, CudaError>;  // Unified memory
    fn copy_from_host(&mut self, data: &[T]) -> Result<(), CudaError>;
    fn copy_to_host(&self, data: &mut [T]) -> Result<(), CudaError>;
    fn copy_from_host_async(&mut self, data: &[T], stream: &CudaStream) -> Result<(), CudaError>;
    fn copy_to_host_async(&self, data: &mut [T], stream: &CudaStream) -> Result<(), CudaError>;
    fn memset(&mut self, value: u8) -> Result<(), CudaError>;
    fn as_ptr(&self) -> *const T;
    fn as_mut_ptr(&mut self) -> *mut T;
}

// CUDA kernel execution
macro_rules! cuda_kernel {
    ($kernel:ident<<<$grid:expr, $block:expr>>>($($arg:expr),*)) => {
        unsafe {
            $kernel.launch($grid, $block, &[$($arg as *const _),*])
        }
    };
    ($kernel:ident<<<$grid:expr, $block:expr, $shared:expr, $stream:expr>>>($($arg:expr),*)) => {
        unsafe {
            $kernel.launch_with_stream($grid, $block, $shared, $stream, &[$($arg as *const _),*])
        }
    };
}

// Usage example
let properties = cuda::get_device_properties(0)?;
print(f"Device: {properties.name}");
print(f"Compute Capability: {properties.compute_capability:?}");

let stream = CudaStream::create()?;
let mut memory = CudaMemory::<f32>::allocate(1000000)?;

let host_data = vec![1.0f32; 1000000];
memory.copy_from_host_async(&host_data, &stream)?;

cuda_kernel!(vector_add_kernel<<<1024, 256, 0, stream>>>(
    memory.as_mut_ptr(),
    memory.as_ptr(),
    1000000
));

stream.synchronize()?;
```

---

## Collections

### std::collections ✅ IMPLEMENTED

Data structures for organizing data:

```neuro
import std::collections::*;

// Dynamic arrays
struct Vec<T> {
    // Dynamic resizable array
}

impl<T> Vec<T> {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn push(&mut self, item: T);
    fn pop(&mut self) -> Option<T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn capacity(&self) -> usize;
    fn reserve(&mut self, additional: usize);
    fn shrink_to_fit(&mut self);
    fn insert(&mut self, index: usize, element: T);
    fn remove(&mut self, index: usize) -> T;
    fn clear(&mut self);
    fn append(&mut self, other: &mut Vec<T>);
    fn split_off(&mut self, at: usize) -> Vec<T>;
}

// Hash maps
struct HashMap<K, V> {
    // Hash table with key-value pairs
}

impl<K: Hash + Eq, V> HashMap<K, V> {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn insert(&mut self, k: K, v: V) -> Option<V>;
    fn get(&self, k: &K) -> Option<&V>;
    fn get_mut(&mut self, k: &K) -> Option<&mut V>;
    fn remove(&mut self, k: &K) -> Option<V>;
    fn contains_key(&self, k: &K) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn clear(&mut self);
    fn keys(&self) -> Keys<K, V>;
    fn values(&self) -> Values<K, V>;
    fn iter(&self) -> Iter<K, V>;
}

// Hash sets
struct HashSet<T> {
    // Hash table for unique values
}

impl<T: Hash + Eq> HashSet<T> {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn insert(&mut self, value: T) -> bool;
    fn remove(&mut self, value: &T) -> bool;
    fn contains(&self, value: &T) -> bool;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn clear(&mut self);
    fn union(&self, other: &HashSet<T>) -> Union<T>;
    fn intersection(&self, other: &HashSet<T>) -> Intersection<T>;
    fn difference(&self, other: &HashSet<T>) -> Difference<T>;
}

// Binary trees (sorted)
struct BTreeMap<K, V> {
    // Self-balancing binary search tree
}

struct BTreeSet<T> {
    // Self-balancing binary search tree for sets
}

// Linked lists
struct LinkedList<T> {
    // Doubly-linked list
}

impl<T> LinkedList<T> {
    fn new() -> Self;
    fn push_front(&mut self, elt: T);
    fn push_back(&mut self, elt: T);
    fn pop_front(&mut self) -> Option<T>;
    fn pop_back(&mut self) -> Option<T>;
    fn front(&self) -> Option<&T>;
    fn back(&self) -> Option<&T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn clear(&mut self);
}

// Double-ended queues
struct VecDeque<T> {
    // Ring buffer implementation
}

impl<T> VecDeque<T> {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn push_front(&mut self, value: T);
    fn push_back(&mut self, value: T);
    fn pop_front(&mut self) -> Option<T>;
    fn pop_back(&mut self) -> Option<T>;
    fn front(&self) -> Option<&T>;
    fn back(&self) -> Option<&T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

// Binary heaps (priority queues)
struct BinaryHeap<T> {
    // Max-heap implementation
}

impl<T: Ord> BinaryHeap<T> {
    fn new() -> Self;
    fn with_capacity(capacity: usize) -> Self;
    fn push(&mut self, item: T);
    fn pop(&mut self) -> Option<T>;
    fn peek(&self) -> Option<&T>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn clear(&mut self);
}

// Usage examples
let mut vec = Vec::new();
vec.push(1);
vec.push(2);
vec.push(3);

let mut map = HashMap::new();
map.insert("key1", "value1");
map.insert("key2", "value2");

let mut set = HashSet::new();
set.insert("item1");
set.insert("item2");

let mut deque = VecDeque::new();
deque.push_back(1);
deque.push_front(0);

let mut heap = BinaryHeap::new();
heap.push(3);
heap.push(1);
heap.push(4);
let max = heap.pop(); // Some(4)
```

---

This comprehensive standard library reference covers all the major modules and functionality available in NEURO, from core utilities to advanced ML/AI features. The implementation status indicators help developers understand what's currently available versus planned for future releases.