# NEURO Attributes Specification v0.1

## Overview

NEURO attributes are compiler directives that enable AI/ML-specific features. They provide a clean, declarative way to enable automatic differentiation, GPU acceleration, and memory management optimizations.

**Current Status (Phase 1 Complete)**: The attribute parsing framework and infrastructure are fully implemented. Full attribute functionality will be completed in Phase 2.

## Core Attributes

### `#[grad]` - Automatic Differentiation

Enables automatic differentiation for functions and data structures.

```neuro
#[grad]
fn neural_network(input: Tensor<f32, [784]>) -> Tensor<f32, [10]> {
    let hidden = relu(linear(input, weights1));
    linear(hidden, weights2)
}
```

**Behavior:**
- Automatically tracks gradients during forward pass
- Enables backward pass computation
- Type system enforces gradient compatibility

### `#[kernel]` - GPU Kernel

Marks functions for GPU compilation.

```neuro
#[kernel(cuda)]
fn matrix_multiply(a: Tensor<f32, [M, K]>, b: Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    // GPU-optimized implementation
}

#[kernel(vulkan)]  
fn convolution(input: Tensor<f32, [B, C, H, W]>, filter: Tensor<f32, [F, C, KH, KW]>) -> Tensor<f32, [B, F, OH, OW]> {
    // Cross-platform GPU implementation
}
```

**Supported Backends:**
- `cuda` - NVIDIA CUDA
- `vulkan` - Cross-platform Vulkan compute  
- `auto` - Automatically select best available

### `#[gpu]` - GPU Execution Hint

Suggests GPU execution without requiring kernel compilation.

```neuro
#[gpu]
fn train_model(data: Dataset, model: &mut Model) {
    // Prefer GPU execution when available
}
```

### `#[pool]` - Memory Pool Allocation

Specifies memory pool for allocation.

```neuro
#[pool("high_frequency")]  
fn process_batch(batch: Tensor<f32, [BATCH, FEATURES]>) -> Tensor<f32, [BATCH, CLASSES]> {
    // Use high-frequency memory pool for optimal performance
}
```

## Attribute Composition

Attributes can be combined for complex scenarios:

```neuro
#[grad]
#[kernel(cuda)]
#[pool("training")]
fn transformer_layer(
    input: Tensor<f32, [SEQ, HIDDEN]>,
    weights: &TransformerWeights
) -> Tensor<f32, [SEQ, HIDDEN]> {
    // GPU-accelerated, differentiable transformer layer
    // with custom memory pool
}
```

## Type System Integration

Attributes interact with the type system to enforce correctness:

- `#[grad]` functions must use gradient-compatible types
- `#[kernel]` functions have restrictions on control flow
- Memory pool attributes affect allocation behavior

## Implementation Status

| Attribute | Phase 1 | Phase 2 | Status |
|-----------|---------|---------|--------|
| `#[grad]` | ✅ Framework | 📅 Full Implementation | Framework ready, full AD in Phase 2 |
| `#[kernel]` | ✅ Parsing | 📅 GPU Compilation | Attribute parsed, GPU kernels in Phase 2 |  
| `#[gpu]` | ✅ Framework | 📅 Full Implementation | Framework ready, full GPU in Phase 2 |
| `#[pool]` | ✅ Framework | 📅 Full Implementation | Framework ready, advanced pools in Phase 2 |

## Future Extensions

- `#[quantized]` - Quantization hints
- `#[distributed]` - Multi-device execution
- `#[cache]` - Result caching strategies