// NEURO Memory Management Example
// Demonstrates advanced memory management features with ARC and memory pools

import std::memory;

// Function demonstrating ARC usage
fn arc_example() -> void {
    // Create ARC-managed data
    let data = new_arc([1, 2, 3, 4, 5]);
    let data_clone = clone_arc(data);  // Reference counted clone
    
    // Weak reference to break cycles  
    let weak_ref = downgrade_arc(data);
    
    // Check if data is still alive
    if let Some(strong_ref) = upgrade_weak(weak_ref) {
        println("Data is still available");
    }
}

// Function demonstrating memory pools for ML workloads
#[pool("training")]
fn training_loop() -> void {
    // All allocations in this function use the "training" memory pool
    // Optimized for ML workload patterns with SIMD alignment
    
    let batch_size = 32;
    let hidden_dim = 512;
    
    // Allocate tensors in the training pool
    let inputs = allocate_tensor([batch_size, hidden_dim]);
    let weights = allocate_tensor([hidden_dim, 256]);
    let gradients = allocate_tensor([hidden_dim, 256]);
    
    // Memory pool automatically manages alignment and reuse
    for epoch in 0..100 {
        // Pool memory is reused efficiently across iterations
        let hidden = matmul(inputs, weights);
        let loss = compute_loss(hidden);
        
        // Gradient computation uses pool memory
        compute_gradients(loss, gradients);
    }
    
    // Pool memory is automatically freed at function exit
}

// Explicit memory pool management
fn explicit_pool_example() -> void {
    // Create a custom memory pool for inference
    let pool = MemoryPool::new("inference", 1024 * 1024 * 64); // 64MB pool
    
    // Use pool for specific allocations
    let model_weights = pool.allocate_aligned(1024 * 512, 64); // SIMD aligned
    
    // Pool statistics
    let stats = pool.get_statistics();
    println("Pool usage: {} / {} bytes", stats.used_bytes, stats.total_bytes);
    
    // Explicit cleanup
    pool.reset(); // Frees all allocations in pool
}

// Memory leak detection (debug mode)
#[debug]
fn leak_detection_example() -> void {
    // In debug mode, all allocations are tracked
    let tracked_data = allocate_tracked(1024);
    
    // Deliberately "forget" to free - will be caught by leak detector
    // forget(tracked_data); // Uncomment to trigger leak detection
    
    // Leak detector runs at program exit in debug builds
}

fn main() -> int {
    // Initialize memory management system
    memory::initialize();
    
    // Configure memory pools
    memory::configure_pool("training", 
        PoolConfig {
            initial_size: 128 * 1024 * 1024,  // 128MB
            alignment: 64,                     // SIMD alignment
            growth_strategy: Exponential,
        }
    );
    
    // Run examples
    arc_example();
    training_loop();
    explicit_pool_example();
    leak_detection_example();
    
    // Memory system automatically cleans up at exit
    return 0;
}