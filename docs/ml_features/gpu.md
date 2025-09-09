# GPU Programming in NEURO

NEURO provides first-class support for GPU programming through its unified kernel programming model, supporting both CUDA and Vulkan compute shaders from the same source code.

## Table of Contents

1. [Overview](#overview)
2. [GPU Kernel Basics](#gpu-kernel-basics)
3. [CUDA Programming](#cuda-programming)
4. [Vulkan Computing](#vulkan-computing)
5. [Cross-Platform Kernels](#cross-platform-kernels)
6. [Memory Management](#memory-management)
7. [Performance Optimization](#performance-optimization)
8. [Debugging and Profiling](#debugging-and-profiling)

---

## Overview

### Unified GPU Programming Model 📅 PLANNED (Phase 2)

NEURO abstracts GPU programming through a unified kernel model that compiles to both CUDA and Vulkan:

- **Single Source**: Write once, run on NVIDIA (CUDA) and AMD/Intel/Mobile (Vulkan)
- **Type Safety**: Compile-time checking of GPU memory access and tensor operations
- **Performance**: Zero-overhead abstractions with aggressive optimization
- **Integration**: Seamless integration with NEURO's tensor system and automatic differentiation

### GPU Architecture Support

```neuro
// Supported GPU architectures
#[cfg(gpu = "cuda")]
const SUPPORTED_CUDA: &[&str] = &[
    "sm_50", "sm_52",           // Maxwell
    "sm_60", "sm_61", "sm_62",  // Pascal  
    "sm_70", "sm_72", "sm_75",  // Volta, Turing
    "sm_80", "sm_86", "sm_87",  // Ampere
    "sm_89", "sm_90"            // Ada Lovelace, Hopper
];

#[cfg(gpu = "vulkan")]
const SUPPORTED_VULKAN: &[&str] = &[
    "spirv-1.0", "spirv-1.1", "spirv-1.2",
    "spirv-1.3", "spirv-1.4", "spirv-1.5", "spirv-1.6"
];
```

---

## GPU Kernel Basics

### Basic Kernel Syntax 📅 PLANNED (Phase 2)

```neuro
import std::gpu::{BlockIdx, ThreadIdx, BlockDim, GridDim};

// Simple GPU kernel
#[kernel(gpu = "cuda")]
fn vector_add(
    a: &Tensor<f32, [N]>,
    b: &Tensor<f32, [N]>, 
    c: &mut Tensor<f32, [N]>
) {
    let idx = BlockIdx.x * BlockDim.x + ThreadIdx.x;
    
    if idx < N {
        c[idx] = a[idx] + b[idx];
    }
}

// Launch kernel from host code
fn main() {
    let a: Tensor<f32, [1000000]> = Tensor::random();
    let b: Tensor<f32, [1000000]> = Tensor::random();
    let mut c: Tensor<f32, [1000000]> = Tensor::zeros();
    
    // Launch configuration
    let threads_per_block = 256;
    let blocks_per_grid = (1000000 + threads_per_block - 1) / threads_per_block;
    
    // Launch kernel
    vector_add<<<blocks_per_grid, threads_per_block>>>(&a, &b, &mut c);
    
    // Synchronize and transfer result to host
    gpu::device_synchronize();
    let result = c.to_cpu();
}
```

### Kernel Attributes

```neuro
// Basic kernel attribute
#[kernel(gpu = "cuda")]
fn simple_kernel() { }

// Multi-backend kernel
#[kernel(gpu = "cuda,vulkan")]
fn cross_platform_kernel() { }

// Kernel with launch configuration
#[kernel(gpu = "cuda")]
#[launch_config(blocks = [32, 32], threads = [16, 16])]
fn matrix_kernel(data: &mut Tensor<f32, [1024, 1024]>) { }

// Kernel with shared memory
#[kernel(gpu = "cuda")]
#[shared_memory(size = 4096)]
fn shared_mem_kernel(input: &Tensor<f32>) { }

// Kernel with compile-time optimizations
#[kernel(gpu = "cuda")]
#[optimize(unroll_loops = true, fast_math = true)]
fn optimized_kernel(data: &Tensor<f32>) { }
```

### GPU Memory Hierarchy

```neuro
// Global memory (device memory)
#[kernel(gpu = "cuda")]
fn global_memory_access(global_data: &mut Tensor<f32, [N]>) {
    let idx = get_global_id();
    global_data[idx] *= 2.0;  // Global memory access
}

// Shared memory (fast on-chip memory)
#[kernel(gpu = "cuda")]
#[shared_memory(tile: [16, 16])]
fn shared_memory_kernel(data: &Tensor<f32, [M, N]>) {
    let local_id = get_local_id();
    let group_id = get_group_id();
    
    // Load data into shared memory
    tile[local_id.y][local_id.x] = data[group_id.y * 16 + local_id.y][group_id.x * 16 + local_id.x];
    
    // Synchronize threads in workgroup
    barrier();
    
    // Use shared memory data
    let result = tile[local_id.y][local_id.x] * 2.0;
    data[group_id.y * 16 + local_id.y][group_id.x * 16 + local_id.x] = result;
}

// Local variables (registers)
#[kernel(gpu = "cuda")]
fn register_usage(input: &Tensor<f32>) -> f32 {
    let local_var = input[get_global_id()];  // Stored in registers
    let computed = local_var * local_var + 1.0;
    computed
}
```

---

## CUDA Programming

### CUDA-Specific Features 📅 PLANNED (Phase 2)

```neuro
import std::cuda::{warp_size, lane_id, warp_id};

// CUDA warp-level primitives
#[kernel(gpu = "cuda")]
fn warp_reduce_sum(data: &Tensor<f32, [N]>, result: &mut Tensor<f32, [N/32]>) {
    let tid = ThreadIdx.x + BlockIdx.x * BlockDim.x;
    let warp_id = tid / warp_size();
    let lane = tid % warp_size();
    
    let val = if tid < N { data[tid] } else { 0.0 };
    
    // Warp shuffle reduction
    let mut sum = val;
    sum += __shfl_down_sync(0xFFFFFFFF, sum, 16);
    sum += __shfl_down_sync(0xFFFFFFFF, sum, 8);
    sum += __shfl_down_sync(0xFFFFFFFF, sum, 4);
    sum += __shfl_down_sync(0xFFFFFFFF, sum, 2);
    sum += __shfl_down_sync(0xFFFFFFFF, sum, 1);
    
    if lane == 0 {
        result[warp_id] = sum;
    }
}

// CUDA cooperative groups
#[kernel(gpu = "cuda")]
#[requires_cooperative_groups]
fn cooperative_kernel(data: &mut Tensor<f32, [N]>) {
    let grid = this_grid();
    let block = this_thread_block();
    let warp = tiled_partition<32>(block);
    
    let tid = grid.thread_rank();
    
    // Grid-level synchronization
    if tid < N {
        data[tid] *= 2.0;
    }
    
    grid.sync();  // Synchronize entire grid
    
    // Further processing...
}

// CUDA tensor cores for mixed-precision
#[kernel(gpu = "cuda")]
#[requires_tensor_cores]
fn tensor_core_matmul(
    a: &Tensor<f16, [M, K]>,
    b: &Tensor<f16, [K, N]>,
    c: &mut Tensor<f32, [M, N]>
) {
    // Automatically uses Tensor Cores when available
    let fragment_a = load_matrix_fragment_a(a);
    let fragment_b = load_matrix_fragment_b(b);
    let fragment_c = load_matrix_fragment_c(c);
    
    mma_sync(fragment_c, fragment_a, fragment_b, fragment_c);
    
    store_matrix_fragment(c, fragment_c);
}
```

### CUDA Memory Management

```neuro
import std::cuda::{CudaMemory, CudaStream, CudaEvent};

// CUDA-specific memory allocation
fn cuda_memory_example() {
    // Allocate CUDA memory
    let device_mem = CudaMemory::allocate::<f32>(1000000)?;
    let host_pinned = CudaMemory::allocate_host_pinned::<f32>(1000000)?;
    
    // Create CUDA streams for async operations
    let stream1 = CudaStream::create()?;
    let stream2 = CudaStream::create()?;
    
    // Async memory transfers
    let host_data = vec![1.0f32; 1000000];
    device_mem.copy_from_host_async(&host_data, &stream1)?;
    
    // Launch kernel on stream
    vector_kernel<<<1024, 256, 0, stream1>>>(device_mem.as_ptr());
    
    // Copy result back asynchronously
    let mut result = vec![0.0f32; 1000000];
    device_mem.copy_to_host_async(&mut result, &stream1)?;
    
    // Synchronize stream
    stream1.synchronize()?;
}

// CUDA unified memory
#[cuda_managed]
static MANAGED_DATA: Tensor<f32, [1000000]> = Tensor::zeros();

fn unified_memory_example() {
    // Accessible from both CPU and GPU
    for i in 0..1000 {
        MANAGED_DATA[i] = i as f32;
    }
    
    // Launch kernel that uses managed memory
    process_managed_data<<<blocks, threads>>>();
    
    // CPU can access immediately (with implicit synchronization)
    print(f"First element: {MANAGED_DATA[0]}");
}
```

### CUDA Optimization Techniques

```neuro
// Coalesced memory access pattern
#[kernel(gpu = "cuda")]
fn coalesced_access(input: &Tensor<f32, [HEIGHT, WIDTH]>, output: &mut Tensor<f32, [HEIGHT, WIDTH]>) {
    let col = BlockIdx.x * BlockDim.x + ThreadIdx.x;
    let row = BlockIdx.y * BlockDim.y + ThreadIdx.y;
    
    if row < HEIGHT && col < WIDTH {
        // Coalesced access - adjacent threads access adjacent memory
        output[row][col] = input[row][col] * 2.0;
    }
}

// Bank conflict avoidance in shared memory
#[kernel(gpu = "cuda")]
#[shared_memory(tile: [33, 32])]  // 33 to avoid bank conflicts
fn transpose_kernel(
    input: &Tensor<f32, [M, N]>,
    output: &mut Tensor<f32, [N, M]>
) {
    let bx = BlockIdx.x;
    let by = BlockIdx.y;
    let tx = ThreadIdx.x;
    let ty = ThreadIdx.y;
    
    // Load input tile to shared memory
    let x_index = bx * 32 + tx;
    let y_index = by * 32 + ty;
    
    if x_index < N && y_index < M {
        tile[ty][tx] = input[y_index][x_index];
    }
    
    __syncthreads();
    
    // Write transposed tile to output
    let x_index_out = by * 32 + tx;
    let y_index_out = bx * 32 + ty;
    
    if x_index_out < M && y_index_out < N {
        output[y_index_out][x_index_out] = tile[tx][ty];
    }
}

// Occupancy optimization
#[kernel(gpu = "cuda")]
#[optimize(max_registers = 32, threads_per_block = 256)]
fn occupancy_optimized_kernel(data: &mut Tensor<f32>) {
    // Compiler optimizes for maximum occupancy
    let tid = get_global_id();
    
    // Use fewer registers to increase occupancy
    let val = data[tid];
    data[tid] = val.sqrt();
}
```

---

## Vulkan Computing

### Vulkan Compute Shaders 📅 PLANNED (Phase 2)

```neuro
import std::vulkan::{LocalInvocationID, GlobalInvocationID, WorkGroupID};

// Basic Vulkan compute shader
#[kernel(gpu = "vulkan")]
#[workgroup_size(256, 1, 1)]
fn vulkan_vector_add(
    #[binding(0)] a: &Tensor<f32, [N]>,
    #[binding(1)] b: &Tensor<f32, [N]>,
    #[binding(2)] c: &mut Tensor<f32, [N]>
) {
    let index = GlobalInvocationID.x;
    
    if index < N {
        c[index] = a[index] + b[index];
    }
}

// Vulkan with local workgroup memory
#[kernel(gpu = "vulkan")]
#[workgroup_size(16, 16, 1)]
#[workgroup_memory(shared_data: [16, 16])]
fn vulkan_matrix_process(
    #[binding(0)] input: &Tensor<f32, [M, N]>,
    #[binding(1)] output: &mut Tensor<f32, [M, N]>
) {
    let local_id = LocalInvocationID;
    let group_id = WorkGroupID;
    
    // Load data into workgroup local memory
    let global_x = group_id.x * 16 + local_id.x;
    let global_y = group_id.y * 16 + local_id.y;
    
    if global_x < N && global_y < M {
        shared_data[local_id.y][local_id.x] = input[global_y][global_x];
    }
    
    // Workgroup barrier
    workgroup_barrier();
    
    // Process using local memory
    if global_x < N && global_y < M {
        let processed = shared_data[local_id.y][local_id.x] * 2.0;
        output[global_y][global_x] = processed;
    }
}
```

### Vulkan Resource Management

```neuro
import std::vulkan::{Device, Buffer, CommandPool, DescriptorSet};

// Vulkan resource setup
fn vulkan_setup() -> Result<VulkanContext, VulkanError> {
    let instance = vulkan::create_instance(&["VK_LAYER_KHRONOS_validation"])?;
    let physical_device = vulkan::select_physical_device(&instance)?;
    let device = vulkan::create_logical_device(&physical_device)?;
    
    let queue_family = device.find_compute_queue_family()?;
    let compute_queue = device.get_queue(queue_family, 0);
    
    Ok(VulkanContext {
        instance,
        device,
        compute_queue,
        command_pool: CommandPool::new(&device, queue_family)?,
    })
}

// Vulkan buffer management
fn vulkan_compute_example(context: &VulkanContext) -> Result<(), VulkanError> {
    let data_size = 1000000 * std::mem::size_of::<f32>();
    
    // Create buffers
    let input_buffer = Buffer::new(
        &context.device,
        data_size,
        BufferUsage::STORAGE_BUFFER,
        MemoryProperty::HOST_VISIBLE
    )?;
    
    let output_buffer = Buffer::new(
        &context.device, 
        data_size,
        BufferUsage::STORAGE_BUFFER,
        MemoryProperty::HOST_VISIBLE
    )?;
    
    // Upload data
    input_buffer.upload(&input_data)?;
    
    // Create descriptor set
    let descriptor_set = DescriptorSet::new(&context.device)?;
    descriptor_set.bind_buffer(0, &input_buffer)?;
    descriptor_set.bind_buffer(1, &output_buffer)?;
    
    // Dispatch compute shader
    let command_buffer = context.command_pool.allocate_command_buffer()?;
    command_buffer.begin()?;
    command_buffer.bind_compute_pipeline(&compute_pipeline)?;
    command_buffer.bind_descriptor_sets(&[descriptor_set])?;
    command_buffer.dispatch(1000000 / 256, 1, 1)?;
    command_buffer.end()?;
    
    // Submit and wait
    context.compute_queue.submit(&command_buffer)?;
    context.device.wait_idle()?;
    
    // Download result
    let result = output_buffer.download::<f32>()?;
    
    Ok(())
}
```

### Vulkan Optimization

```neuro
// Vulkan subgroup operations (similar to CUDA warps)
#[kernel(gpu = "vulkan")]
#[workgroup_size(64, 1, 1)]
#[require_subgroup_ops]
fn vulkan_subgroup_reduce(
    #[binding(0)] input: &Tensor<f32, [N]>,
    #[binding(1)] output: &mut Tensor<f32, [N/64]>
) {
    let tid = GlobalInvocationID.x;
    let subgroup_id = tid / SubgroupSize;
    
    let val = if tid < N { input[tid] } else { 0.0 };
    
    // Subgroup reduction
    let sum = subgroup_add(val);
    
    if subgroup_invocation_id() == 0 {
        output[subgroup_id] = sum;
    }
}

// Vulkan specialization constants
#[kernel(gpu = "vulkan")]
#[specialization_constant(BLOCK_SIZE = 256)]
#[specialization_constant(USE_FAST_MATH = true)]
fn specialized_kernel(data: &mut Tensor<f32>) {
    // BLOCK_SIZE and USE_FAST_MATH are compile-time constants
    let local_array: [f32; BLOCK_SIZE] = [0.0; BLOCK_SIZE];
    
    if USE_FAST_MATH {
        // Use fast math approximations
        data[get_global_id()] = fast_sin(data[get_global_id()]);
    } else {
        // Use precise math
        data[get_global_id()] = precise_sin(data[get_global_id()]);
    }
}
```

---

## Cross-Platform Kernels

### Unified Kernel Programming 📅 PLANNED (Phase 2)

```neuro
// Single kernel for both CUDA and Vulkan
#[kernel(gpu = "cuda,vulkan")]
fn universal_matrix_multiply<const M: usize, const N: usize, const K: usize>(
    a: &Tensor<f32, [M, K]>,
    b: &Tensor<f32, [K, N]>,
    c: &mut Tensor<f32, [M, N]>
) {
    // Platform-agnostic thread indexing
    let row = get_global_id_y();
    let col = get_global_id_x();
    
    if row < M && col < N {
        let mut sum = 0.0;
        for k in 0..K {
            sum += a[row][k] * b[k][col];
        }
        c[row][col] = sum;
    }
}

// Platform-specific optimizations
#[kernel(gpu = "cuda,vulkan")]
fn optimized_convolution(
    input: &Tensor<f32, [N, H, W, C]>,
    kernel: &Tensor<f32, [KH, KW, C, F]>,
    output: &mut Tensor<f32, [N, H_OUT, W_OUT, F]>
) {
    let batch = get_batch_id();
    let out_h = get_output_height_id();
    let out_w = get_output_width_id();
    let filter = get_filter_id();
    
    #[cfg(gpu = "cuda")]
    {
        // CUDA-specific optimization using shared memory
        __shared__ let tile: [[f32; 16]; 16];
        // ... CUDA-specific implementation
    }
    
    #[cfg(gpu = "vulkan")]
    {
        // Vulkan-specific optimization using workgroup local memory
        let tile = workgroup_memory::<[16, 16]>();
        // ... Vulkan-specific implementation
    }
    
    // Common computation logic
    let mut sum = 0.0;
    for kh in 0..KH {
        for kw in 0..KW {
            for c in 0..C {
                let in_h = out_h + kh - KH / 2;
                let in_w = out_w + kw - KW / 2;
                
                if in_h >= 0 && in_h < H && in_w >= 0 && in_w < W {
                    sum += input[batch][in_h][in_w][c] * kernel[kh][kw][c][filter];
                }
            }
        }
    }
    
    output[batch][out_h][out_w][filter] = sum;
}
```

### Platform Abstraction Layer

```neuro
// Abstract GPU operations
trait GpuDevice {
    fn allocate_buffer<T>(&self, size: usize) -> Result<GpuBuffer<T>, GpuError>;
    fn launch_kernel<K: Kernel>(&self, kernel: K, grid: GridConfig) -> Result<(), GpuError>;
    fn synchronize(&self) -> Result<(), GpuError>;
}

// CUDA implementation
impl GpuDevice for CudaDevice {
    fn allocate_buffer<T>(&self, size: usize) -> Result<GpuBuffer<T>, GpuError> {
        CudaBuffer::allocate(size).map(GpuBuffer::Cuda)
    }
    
    fn launch_kernel<K: Kernel>(&self, kernel: K, grid: GridConfig) -> Result<(), GpuError> {
        cuda_launch(kernel, grid)
    }
}

// Vulkan implementation
impl GpuDevice for VulkanDevice {
    fn allocate_buffer<T>(&self, size: usize) -> Result<GpuBuffer<T>, GpuError> {
        VulkanBuffer::allocate(size).map(GpuBuffer::Vulkan)
    }
    
    fn launch_kernel<K: Kernel>(&self, kernel: K, grid: GridConfig) -> Result<(), GpuError> {
        vulkan_dispatch(kernel, grid)
    }
}

// Unified high-level API
fn run_computation<D: GpuDevice>(device: &D) -> Result<(), GpuError> {
    let input_buffer = device.allocate_buffer(1000000)?;
    let output_buffer = device.allocate_buffer(1000000)?;
    
    let kernel = universal_matrix_multiply;
    let grid = GridConfig::new([1024, 1024], [16, 16]);
    
    device.launch_kernel(kernel, grid)?;
    device.synchronize()?;
    
    Ok(())
}
```

---

## Memory Management

### GPU Memory Types 📅 PLANNED (Phase 2)

```neuro
import std::gpu::{GpuMemory, MemoryType, MemoryPool};

// Different GPU memory types
enum GpuMemoryType {
    Global,        // Device global memory (large, slower)
    Shared,        // On-chip shared memory (small, fast)
    Constant,      // Read-only cached memory
    Texture,       // Texture memory (cached, 2D locality)
    Unified,       // Unified memory (CUDA) / Host-coherent (Vulkan)
}

// GPU memory allocation
fn gpu_memory_example() {
    // Global memory allocation
    let global_mem = GpuMemory::allocate::<f32>(
        1000000, 
        MemoryType::Global
    )?;
    
    // Constant memory (read-only, cached)
    let constants = GpuMemory::allocate_constant(&[1.0f32, 2.0, 3.14159])?;
    
    // Unified/managed memory
    let unified = GpuMemory::allocate::<f32>(
        1000000,
        MemoryType::Unified
    )?;
    
    // Memory pools for frequent allocations
    let tensor_pool = MemoryPool::new(
        device: &gpu_device,
        initial_size: 1024 * 1024 * 1024,  // 1GB
        memory_type: MemoryType::Global
    );
}

// Memory transfer operations
fn memory_transfers() {
    let host_data = vec![1.0f32; 1000000];
    let mut gpu_buffer = GpuMemory::allocate::<f32>(1000000, MemoryType::Global)?;
    
    // Synchronous transfers
    gpu_buffer.copy_from_host(&host_data)?;
    let result = gpu_buffer.copy_to_host()?;
    
    // Asynchronous transfers (non-blocking)
    let stream = GpuStream::create()?;
    gpu_buffer.copy_from_host_async(&host_data, &stream)?;
    gpu_buffer.copy_to_host_async(&mut result, &stream)?;
    stream.synchronize()?;
    
    // Peer-to-peer transfers (between GPUs)
    let gpu2_buffer = GpuMemory::allocate::<f32>(1000000, MemoryType::Global)?;
    gpu_buffer.copy_to_peer(&gpu2_buffer, target_device: 1)?;
}
```

### Memory Access Patterns

```neuro
// Coalesced memory access (efficient)
#[kernel(gpu = "cuda,vulkan")]
fn coalesced_access(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    // Adjacent threads access adjacent memory locations
    if tid < N {
        data[tid] = data[tid] * 2.0;  // Coalesced access
    }
}

// Strided memory access (inefficient)
#[kernel(gpu = "cuda,vulkan")]
fn strided_access(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    // Non-coalesced access pattern
    if tid < N / 32 {
        data[tid * 32] = data[tid * 32] * 2.0;  // Large strides
    }
}

// Shared memory usage
#[kernel(gpu = "cuda,vulkan")]  
#[shared_memory(tile: [16, 16])]
fn shared_memory_optimization(
    input: &Tensor<f32, [M, N]>,
    output: &mut Tensor<f32, [M, N]>
) {
    let local_x = get_local_id_x();
    let local_y = get_local_id_y();
    let group_x = get_group_id_x();
    let group_y = get_group_id_y();
    
    // Load data into shared memory
    let global_x = group_x * 16 + local_x;
    let global_y = group_y * 16 + local_y;
    
    if global_x < N && global_y < M {
        tile[local_y][local_x] = input[global_y][global_x];
    }
    
    // Synchronize threads in workgroup
    workgroup_barrier();
    
    // Process using fast shared memory
    if global_x < N && global_y < M {
        let neighbors = tile[local_y - 1][local_x] + tile[local_y + 1][local_x] +
                       tile[local_y][local_x - 1] + tile[local_y][local_x + 1];
        output[global_y][global_x] = neighbors * 0.25;
    }
}
```

---

## Performance Optimization

### Occupancy Optimization 📅 PLANNED (Phase 2)

```neuro
// Optimize for maximum occupancy
#[kernel(gpu = "cuda")]
#[optimize(occupancy = "max")]
#[registers(max = 32)]         // Limit register usage
#[shared_memory(max = 48KB)]   // Limit shared memory usage
fn occupancy_optimized(data: &mut Tensor<f32>) {
    let tid = get_global_id();
    
    // Use minimal registers and shared memory
    let val = data[tid];
    data[tid] = val.sqrt() + val.sin();
}

// Memory bandwidth optimization
#[kernel(gpu = "cuda,vulkan")]
#[memory_coalescing(enable = true)]
fn bandwidth_optimized(
    input: &Tensor<f32, [N]>,
    output: &mut Tensor<f32, [N]>
) {
    let tid = get_global_id();
    let stride = get_grid_stride();
    
    // Grid-stride loop for better memory utilization
    let mut i = tid;
    while i < N {
        output[i] = input[i] * 2.0 + 1.0;
        i += stride;
    }
}
```

### Kernel Fusion

```neuro
// Multiple separate kernels (inefficient)
fn separate_kernels(data: &mut Tensor<f32, [N]>) {
    add_constant<<<blocks, threads>>>(data, 1.0);
    gpu_synchronize();
    
    multiply_scalar<<<blocks, threads>>>(data, 2.0);
    gpu_synchronize();
    
    apply_activation<<<blocks, threads>>>(data);
}

// Fused kernel (efficient)
#[kernel(gpu = "cuda,vulkan")]
fn fused_operations(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    if tid < N {
        // All operations in single kernel
        let val = data[tid] + 1.0;    // Add constant
        let val = val * 2.0;          // Multiply scalar  
        let val = val.max(0.0);       // ReLU activation
        data[tid] = val;
    }
}

// Automatic kernel fusion (compiler optimization)
#[fuse_kernels]
fn auto_fused_pipeline(data: &mut Tensor<f32, [N]>) {
    // Compiler automatically fuses these operations
    data += 1.0;
    data *= 2.0;
    data = data.relu();
}
```

### Advanced Optimization Techniques

```neuro
// Loop unrolling for better instruction-level parallelism
#[kernel(gpu = "cuda,vulkan")]
#[unroll_loops(factor = 4)]
fn unrolled_computation(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id() * 4;
    
    // Compiler unrolls this loop
    for i in 0..4 {
        if tid + i < N {
            data[tid + i] = compute_complex(data[tid + i]);
        }
    }
}

// Prefetching for memory latency hiding
#[kernel(gpu = "cuda")]
#[prefetch_distance(4)]
fn prefetched_access(input: &Tensor<f32, [N]>, output: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    // Compiler generates prefetch instructions
    if tid + 4 < N {
        prefetch(&input[tid + 4]);  // Prefetch future data
    }
    
    output[tid] = expensive_computation(input[tid]);
}

// Vectorized memory operations
#[kernel(gpu = "cuda,vulkan")]  
#[vectorize(width = 4)]
fn vectorized_operations(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    let vec_id = tid / 4;
    
    if vec_id * 4 + 3 < N {
        // Load 4 floats at once
        let vec_data = load_float4(&data[vec_id * 4]);
        
        // Vectorized computation
        let result = vec_data * 2.0 + 1.0;
        
        // Store 4 floats at once
        store_float4(&mut data[vec_id * 4], result);
    }
}
```

---

## Debugging and Profiling

### GPU Debugging 📅 PLANNED (Phase 3)

```neuro
// Debug assertions in GPU code
#[kernel(gpu = "cuda,vulkan")]
fn debug_kernel(data: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    // Debug assertions (compiled out in release mode)
    debug_assert!(tid < N, "Thread index out of bounds");
    debug_assert!(data[tid].is_finite(), "Input data contains NaN/Inf");
    
    let result = data[tid] * 2.0;
    
    debug_assert!(result.is_finite(), "Output contains NaN/Inf");
    data[tid] = result;
}

// GPU printf debugging (CUDA-specific)
#[kernel(gpu = "cuda")]
fn printf_debug(data: &Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    if tid == 0 {
        printf("Kernel launched with %d threads\n", get_grid_size());
    }
    
    if tid < 10 {  // Only first 10 threads
        printf("Thread %d: data[%d] = %f\n", tid, tid, data[tid]);
    }
}

// Conditional breakpoints
#[kernel(gpu = "cuda")]
fn conditional_breakpoint(data: &Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    if tid == 42 && data[tid] > 1000.0 {
        trap();  // Trigger debugger breakpoint
    }
    
    data[tid] = process_data(data[tid]);
}
```

### GPU Profiling

```neuro
import std::gpu::profiling::{GpuProfiler, ProfileMarker};

// Profiling GPU kernels
fn profile_gpu_computation() {
    let profiler = GpuProfiler::new()?;
    
    {
        let _marker = ProfileMarker::new("vector_addition");
        vector_add<<<blocks, threads>>>(a, b, c);
    }
    
    {
        let _marker = ProfileMarker::new("matrix_multiply");
        matmul<<<blocks, threads>>>(x, y, z);
    }
    
    // Generate profiling report
    let report = profiler.generate_report();
    print(report);
}

// Memory bandwidth profiling
#[kernel(gpu = "cuda,vulkan")]
#[profile_memory_bandwidth]
fn bandwidth_test(input: &Tensor<f32, [N]>, output: &mut Tensor<f32, [N]>) {
    let tid = get_global_id();
    
    if tid < N {
        output[tid] = input[tid];  // Simple copy to measure bandwidth
    }
}

// Kernel timing
fn time_kernel_execution() {
    let start_time = GpuTimer::start();
    
    my_kernel<<<blocks, threads>>>(data);
    gpu_synchronize();
    
    let elapsed = start_time.elapsed();
    print(f"Kernel execution time: {elapsed:.3} ms");
}
```

### Error Handling

```neuro
import std::gpu::{GpuError, GpuResult};

// GPU error handling
fn safe_gpu_computation() -> GpuResult<Tensor<f32, [N]>> {
    // Check for CUDA/Vulkan errors after each operation
    let input = Tensor::random().to_gpu()?;
    
    let result = gpu_kernel(input)
        .map_err(|e| GpuError::KernelLaunchFailed(e))?;
    
    let output = result.to_cpu()
        .map_err(|e| GpuError::MemoryTransferFailed(e))?;
    
    Ok(output)
}

// Automatic error checking
#[gpu_error_checking(enabled = true)]
fn checked_gpu_operations() {
    // All GPU operations automatically checked for errors
    let data = Tensor::random().to_gpu();  // Checked
    let result = process_kernel(data);     // Checked
    let output = result.to_cpu();          // Checked
}

// Recovery from GPU errors
fn error_recovery() {
    match gpu_computation() {
        Ok(result) => process_result(result),
        Err(GpuError::OutOfMemory) => {
            // Fall back to CPU computation
            print("GPU out of memory, falling back to CPU");
            cpu_computation()
        },
        Err(GpuError::DeviceNotAvailable) => {
            // Use different GPU or CPU
            print("Primary GPU not available, trying alternative");
            fallback_computation()
        },
        Err(e) => {
            print(f"Unrecoverable GPU error: {e}");
            std::process::exit(1);
        }
    }
}
```

This comprehensive GPU programming documentation covers all aspects of GPU development in NEURO, from basic kernel programming to advanced optimization techniques and debugging strategies. The unified programming model allows developers to write GPU code once and run it on both CUDA and Vulkan platforms.