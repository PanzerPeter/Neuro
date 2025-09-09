//! GPU compilation and kernel generation for NEURO
//! 
//! Handles #[kernel] and #[gpu] attributes to generate GPU code for CUDA and Vulkan.

pub mod kernel_attribute;
pub mod cuda_backend;
pub mod vulkan_backend;
pub mod kernel_fusion;
pub mod thread_block_analysis;
pub mod memory_coalescing;

use thiserror::Error;

/// GPU compilation errors
#[derive(Error, Debug, Clone)]
pub enum GpuError {
    #[error("Kernel attribute parsing error: {message}")]
    AttributeError { message: String },
    #[error("Unsupported GPU operation: {op}")]
    UnsupportedOperation { op: String },
    #[error("CUDA compilation error: {error}")]
    CudaError { error: String },
    #[error("Vulkan compilation error: {error}")]
    VulkanError { error: String },
    #[error("Thread block size invalid: {size}")]
    InvalidThreadBlockSize { size: usize },
    #[error("Memory coalescing failed: {reason}")]
    MemoryCoalescingError { reason: String },
}

/// Result type for GPU operations
pub type GpuResult<T> = Result<T, GpuError>;

/// GPU backend types
#[derive(Debug, Clone, PartialEq)]
pub enum GpuBackend {
    Cuda,
    Vulkan,
    Auto, // Choose best available
}

/// Kernel launch configuration
#[derive(Debug, Clone, PartialEq)]
pub struct KernelConfig {
    pub threads_per_block: (u32, u32, u32),
    pub blocks_per_grid: (u32, u32, u32),
    pub shared_memory_bytes: u32,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            threads_per_block: (256, 1, 1),
            blocks_per_grid: (1, 1, 1),
            shared_memory_bytes: 0,
        }
    }
}

/// GPU compilation context
pub struct GpuCompiler {
    backend: GpuBackend,
    config: KernelConfig,
}

impl GpuCompiler {
    /// Create a new GPU compiler
    pub fn new(backend: GpuBackend) -> Self {
        Self {
            backend,
            config: KernelConfig::default(),
        }
    }

    /// Set kernel configuration
    pub fn with_config(mut self, config: KernelConfig) -> Self {
        self.config = config;
        self
    }

    /// Check if GPU backend is available
    pub fn is_backend_available(&self) -> bool {
        match self.backend {
            GpuBackend::Cuda => cuda_backend::is_cuda_available(),
            GpuBackend::Vulkan => vulkan_backend::is_vulkan_available(),
            GpuBackend::Auto => {
                cuda_backend::is_cuda_available() || vulkan_backend::is_vulkan_available()
            },
        }
    }

    /// Compile a kernel function
    pub fn compile_kernel(&self, kernel_source: &str) -> GpuResult<String> {
        match self.backend {
            GpuBackend::Cuda => cuda_backend::compile_cuda_kernel(kernel_source),
            GpuBackend::Vulkan => vulkan_backend::compile_vulkan_kernel(kernel_source),
            GpuBackend::Auto => {
                if cuda_backend::is_cuda_available() {
                    cuda_backend::compile_cuda_kernel(kernel_source)
                } else if vulkan_backend::is_vulkan_available() {
                    vulkan_backend::compile_vulkan_kernel(kernel_source)
                } else {
                    Err(GpuError::UnsupportedOperation {
                        op: "No GPU backend available".to_string(),
                    })
                }
            },
        }
    }
}