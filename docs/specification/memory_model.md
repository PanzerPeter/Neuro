# NEURO Memory Model Specification v1.0

## Overview

NEURO uses a pragmatic memory management approach designed for AI/ML workloads. The default model is Automatic Reference Counting (ARC) with explicit memory pools for high-performance scenarios.

## Memory Management Philosophy

### Design Principles

1. **Performance**: Zero-cost abstractions with predictable performance
2. **Safety**: Memory safety without garbage collection pauses
3. **Pragmatism**: Simple default behavior with power-user options
4. **ML-Optimized**: Designed for tensor operations and GPU memory

### Key Features

- **Default ARC**: Automatic reference counting for ease of use
- **Memory Pools**: High-performance allocators for ML workloads
- **SIMD Alignment**: Automatic alignment for vectorized operations
- **GPU Memory**: Unified CPU/GPU memory model
- **Leak Detection**: Debug-mode leak tracking

## Automatic Reference Counting (ARC)

### Basic ARC Model

NEURO uses reference counting as the default memory management strategy:

```neuro
use memory::Arc;

let data = Arc::new(vec![1, 2, 3, 4, 5]); // Reference count: 1
let reference1 = data.clone();            // Reference count: 2
let reference2 = data.clone();            // Reference count: 3

// When variables go out of scope, reference count decreases
// Data is freed when count reaches 0
```

### Reference Counting Rules

1. **Single Ownership**: Each `Arc<T>` owns a reference to the data
2. **Shared Access**: Multiple `Arc<T>` instances can share the same data
3. **Automatic Cleanup**: Data is freed when the last reference is dropped
4. **Thread Safety**: `Arc<T>` is thread-safe for shared ownership

### ARC Implementation

```neuro
// Internal ARC structure (simplified)
struct NeuroArc<T> {
    ptr: *mut ArcData<T>,
}

struct ArcData<T> {
    ref_count: AtomicUsize,
    weak_count: AtomicUsize,
    data: T,
}

impl<T> NeuroArc<T> {
    fn new(data: T) -> Self {
        let arc_data = Box::into_raw(Box::new(ArcData {
            ref_count: AtomicUsize::new(1),
            weak_count: AtomicUsize::new(1),
            data,
        }));
        
        NeuroArc { ptr: arc_data }
    }
    
    fn clone(&self) -> Self {
        unsafe {
            (*self.ptr).ref_count.fetch_add(1, Ordering::Relaxed);
        }
        NeuroArc { ptr: self.ptr }
    }
}

impl<T> Drop for NeuroArc<T> {
    fn drop(&mut self) {
        unsafe {
            if (*self.ptr).ref_count.fetch_sub(1, Ordering::Release) == 1 {
                // Last reference - free the data
                std::sync::atomic::fence(Ordering::Acquire);
                Box::from_raw(self.ptr);
            }
        }
    }
}
```

### Cycle Detection

NEURO includes optional cycle detection to handle reference cycles:

```neuro
use memory::{Arc, Weak};

struct Node {
    data: i32,
    parent: Option<Weak<Node>>,
    children: Vec<Arc<Node>>,
}

// Weak references break cycles
let parent = Arc::new(Node { data: 1, parent: None, children: vec![] });
let child = Arc::new(Node { 
    data: 2, 
    parent: Some(Arc::downgrade(&parent)), 
    children: vec![] 
});
```

## Memory Pools

### High-Performance Allocation

For ML workloads requiring frequent allocation/deallocation:

```neuro
use memory::MemoryPool;

// Create a 1GB memory pool with SIMD alignment
let pool = MemoryPool::new(1024 * 1024 * 1024)
    .with_alignment(32)  // AVX2 alignment
    .with_gpu_support(true);

// Allocate tensors from the pool
let tensor1: Tensor<f32, [1000, 1000]> = pool.allocate_tensor();
let tensor2: Tensor<f32, [500, 2000]> = pool.allocate_tensor();

// Automatic pool cleanup when variables go out of scope
```

### Pool Implementation

```neuro
struct MemoryPool {
    blocks: Vec<PoolBlock>,
    free_list: Vec<(*mut u8, usize)>,
    alignment: usize,
    total_size: usize,
    allocated: AtomicUsize,
}

impl MemoryPool {
    fn new(size: usize) -> Self {
        MemoryPool {
            blocks: Vec::new(),
            free_list: Vec::new(),
            alignment: 16, // Default SIMD alignment
            total_size: size,
            allocated: AtomicUsize::new(0),
        }
    }
    
    fn allocate<T>(&self, count: usize) -> PoolPtr<T> {
        let size = count * std::mem::size_of::<T>();
        let aligned_size = align_up(size, self.alignment);
        
        // Find suitable free block or allocate new one
        let ptr = self.find_free_block(aligned_size)
            .unwrap_or_else(|| self.allocate_block(aligned_size));
            
        PoolPtr::new(ptr as *mut T, count, self)
    }
}
```

### Pool-Allocated Tensors

```neuro
// Tensors can use different allocators
let stack_tensor: Tensor<f32, [10, 10]> = Tensor::zeros(); // Stack allocated
let heap_tensor = Arc::new(Tensor::<f32, [1000, 1000]>::zeros()); // Heap with ARC
let pool_tensor = pool.allocate_tensor::<f32, [1000, 1000]>(); // Pool allocated
```

## GPU Memory Management

### Unified Memory Model

NEURO provides a unified interface for CPU and GPU memory:

```neuro
use memory::{Device, DeviceMemory};

enum Device {
    CPU,
    CUDA(u32),
    Vulkan(u32),
}

// Allocate on specific device
let gpu_tensor = Tensor::<f32, [1000, 1000]>::zeros_on(Device::CUDA(0));
let cpu_tensor = Tensor::<f32, [1000, 1000]>::zeros_on(Device::CPU);

// Automatic transfers
let result = gpu_tensor.to_device(Device::CPU); // GPU -> CPU transfer
```

### GPU Memory Pools

```neuro
// GPU-specific memory pool
let gpu_pool = MemoryPool::new_gpu(Device::CUDA(0), 2 * 1024 * 1024 * 1024); // 2GB

let gpu_tensor = gpu_pool.allocate_tensor::<f32, [2000, 2000]>();

#[kernel]
fn process_on_gpu(input: Tensor<f32, [N, M]>) -> Tensor<f32, [N, M]> {
    // Kernel uses GPU memory directly
    input.map(|x| x * 2.0)
}
```

## Memory Layout and Alignment

### SIMD Alignment

Tensors are automatically aligned for SIMD operations:

```neuro
// Automatic alignment for vectorization
let tensor: Tensor<f32, [1000, 1000]> = Tensor::zeros();
// Guaranteed to be aligned to 32-byte boundaries for AVX2

// Explicit alignment control
let aligned_tensor = Tensor::<f32, [1000, 1000]>::zeros_aligned(64); // AVX-512
```

### Memory Layout

```neuro
// Row-major layout by default
let matrix: Tensor<f32, [3, 4]> = tensor!([
    [1.0, 2.0, 3.0, 4.0],    // Contiguous in memory
    [5.0, 6.0, 7.0, 8.0],
    [9.0, 10.0, 11.0, 12.0]
]);

// Column-major layout for specific operations
let col_major = matrix.to_layout(Layout::ColumnMajor);
```

## Memory Safety Guarantees

### Compile-Time Checks

1. **No Dangling Pointers**: References are always valid
2. **No Double-Free**: ARC prevents double-freeing
3. **No Memory Leaks**: Automatic cleanup on scope exit
4. **No Buffer Overflows**: Bounds checking on tensor operations

### Runtime Checks

```neuro
// Bounds checking in debug mode
let tensor: Tensor<f32, [10, 10]> = Tensor::zeros();
let value = tensor[15][5]; // Panic in debug mode, undefined in release

// Explicit bounds checking
match tensor.get([15, 5]) {
    Some(value) => println!("Value: {}", value),
    None => println!("Index out of bounds"),
}
```

## Memory Profiling and Debugging

### Leak Detection

```neuro
// Enable leak detection in debug builds
#[cfg(debug_assertions)]
use memory::LeakDetector;

fn main() {
    LeakDetector::initialize();
    
    // Your code here
    let tensor = Arc::new(Tensor::<f32, [100, 100]>::zeros());
    
    LeakDetector::check_leaks(); // Reports any leaked memory
}
```

### Memory Usage Tracking

```neuro
use memory::MemoryTracker;

// Track memory usage
let tracker = MemoryTracker::new();

let tensor1 = Arc::new(Tensor::<f32, [1000, 1000]>::zeros());
println!("Memory usage: {} MB", tracker.current_usage() / 1024 / 1024);

let tensor2 = Arc::new(Tensor::<f32, [2000, 2000]>::zeros());
println!("Memory usage: {} MB", tracker.current_usage() / 1024 / 1024);

// Memory usage statistics
println!("Peak usage: {} MB", tracker.peak_usage() / 1024 / 1024);
println!("Total allocations: {}", tracker.allocation_count());
```

## Best Practices

### When to Use ARC

```neuro
// Good: Shared data structures
let model_weights = Arc::new(load_neural_network());
let thread1_weights = model_weights.clone();
let thread2_weights = model_weights.clone();

// Good: Expensive-to-clone data
let large_dataset = Arc::new(load_dataset());
process_data_parallel(large_dataset);
```

### When to Use Memory Pools

```neuro
// Good: Frequent tensor operations
let pool = MemoryPool::new(1024 * 1024 * 1024); // 1GB pool

for epoch in 0..100 {
    let batch = pool.allocate_tensor::<f32, [32, 784]>();
    let gradients = pool.allocate_tensor::<f32, [784, 10]>();
    
    // Training step - memory is reused efficiently
    train_step(batch, gradients);
    
    // Memory automatically returned to pool
}
```

### GPU Memory Best Practices

```neuro
// Minimize CPU-GPU transfers
#[kernel]
fn full_training_step(
    data: Tensor<f32, [N, D]>,    // Keep on GPU
    weights: Tensor<f32, [D, C]>, // Keep on GPU
    targets: Tensor<i32, [N]>     // Keep on GPU
) -> (Tensor<f32, [D, C]>, f32) { // Return gradients and loss
    let predictions = data @ weights;
    let loss = cross_entropy(predictions, targets);
    let gradients = backward(loss);
    (gradients, loss.item())
}
```

## Implementation Status

| Feature | Status | Notes |
|---------|--------|--------|
| Basic ARC | ✅ COMPLETE | Thread-safe reference counting working |
| Cycle Detection | ✅ COMPLETE | Weak references implemented |
| Memory Pools | ✅ COMPLETE | SIMD-aligned high-performance pools |
| GPU Memory | 📅 PLANNED | GPU device abstraction planned for Phase 2 |
| Leak Detection | ✅ COMPLETE | Debug-mode leak tracking |
| Memory Profiling | ✅ COMPLETE | Usage tracking and statistics |
| SIMD Alignment | ✅ COMPLETE | Automatic tensor alignment |

## Performance Characteristics

- **ARC Overhead**: ~16 bytes per allocation (reference counts + metadata)
- **Pool Overhead**: ~8 bytes per allocation (size tracking)
- **Alignment**: Guaranteed 32-byte alignment for AVX2 operations
- **GPU Transfer**: Async transfers with CPU/GPU synchronization
- **Leak Detection**: Zero runtime overhead in release builds

NEURO's memory model provides the safety of garbage collection with the performance predictability of manual memory management, optimized specifically for AI/ML workloads.