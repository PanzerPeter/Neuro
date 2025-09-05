//! Attribute system for NEURO (#[grad], #[kernel], etc.)

use serde::{Deserialize, Serialize};
use std::fmt;

/// NEURO attributes for AI/ML features
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Attribute {
    /// Enable automatic differentiation
    Grad,
    /// Mark as GPU kernel
    Kernel {
        backend: GpuBackend,
    },
    /// GPU execution hint
    Gpu,
    /// Memory pool allocation
    Pool(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GpuBackend {
    Cuda,
    Vulkan,
    Auto,
}

impl fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Attribute::Grad => write!(f, "grad"),
            Attribute::Kernel { backend } => write!(f, "kernel({})", backend),
            Attribute::Gpu => write!(f, "gpu"),
            Attribute::Pool(name) => write!(f, "pool({})", name),
        }
    }
}

impl fmt::Display for GpuBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpuBackend::Cuda => write!(f, "cuda"),
            GpuBackend::Vulkan => write!(f, "vulkan"),
            GpuBackend::Auto => write!(f, "auto"),
        }
    }
}