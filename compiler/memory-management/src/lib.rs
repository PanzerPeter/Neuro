//! Memory management system for NEURO
//! 
//! Provides both Automatic Reference Counting (ARC) for general use
//! and explicit memory pools for high-performance ML workloads.

pub mod arc_runtime;
pub mod memory_pool;
pub mod leak_detection;
pub mod allocation_tracking;

use std::sync::Arc;
use thiserror::Error;

/// Memory management errors
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Memory pool exhausted: {0}")]
    PoolExhausted(String),
    #[error("Invalid allocation size: {size}")]
    InvalidSize { size: usize },
    #[error("Memory leak detected: {count} objects")]
    LeakDetected { count: usize },
    #[error("Double free detected")]
    DoubleFree,
}

/// Memory management configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Enable leak detection (development only)
    pub leak_detection: bool,
    /// Enable allocation tracking
    pub allocation_tracking: bool,
    /// Default pool size in bytes
    pub default_pool_size: usize,
    /// GC threshold
    pub gc_threshold: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            leak_detection: cfg!(debug_assertions),
            allocation_tracking: cfg!(debug_assertions),
            default_pool_size: 64 * 1024 * 1024, // 64MB
            gc_threshold: 1000,
        }
    }
}

/// Memory manager for NEURO runtime
#[derive(Debug)]
pub struct MemoryManager {
    config: MemoryConfig,
    pools: parking_lot::RwLock<std::collections::HashMap<String, memory_pool::MemoryPool>>,
    tracker: Option<allocation_tracking::AllocationTracker>,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new(config: MemoryConfig) -> Self {
        let tracker = if config.allocation_tracking {
            Some(allocation_tracking::AllocationTracker::new())
        } else {
            None
        };

        Self {
            config,
            pools: parking_lot::RwLock::new(std::collections::HashMap::new()),
            tracker,
        }
    }

    /// Create or get a named memory pool
    pub fn get_pool(&self, name: &str) -> Result<Arc<memory_pool::MemoryPool>, MemoryError> {
        let pools = self.pools.read();
        if let Some(_pool) = pools.get(name) {
            // Create a new pool with the same configuration instead of cloning
            return Ok(Arc::new(memory_pool::MemoryPool::new(name, self.config.default_pool_size)?));
        }
        drop(pools);

        let mut pools = self.pools.write();
        if let Some(_pool) = pools.get(name) {
            return Ok(Arc::new(memory_pool::MemoryPool::new(name, self.config.default_pool_size)?));
        }

        let pool = memory_pool::MemoryPool::new(name, self.config.default_pool_size)?;
        pools.insert(name.to_string(), pool);
        Ok(Arc::new(memory_pool::MemoryPool::new(name, self.config.default_pool_size)?))
    }

    /// Get allocation statistics
    pub fn get_stats(&self) -> Option<allocation_tracking::AllocationStats> {
        self.tracker.as_ref().map(|t| t.get_stats())
    }

    /// Check for memory leaks (development only)
    pub fn check_leaks(&self) -> Result<(), MemoryError> {
        if let Some(tracker) = &self.tracker {
            tracker.check_leaks()
        } else {
            Ok(())
        }
    }
}

/// Global memory manager instance
static MEMORY_MANAGER: std::sync::OnceLock<MemoryManager> = std::sync::OnceLock::new();

/// Initialize the global memory manager
pub fn init_memory_manager(config: MemoryConfig) {
    MEMORY_MANAGER.set(MemoryManager::new(config))
        .expect("Memory manager already initialized");
}

/// Get the global memory manager
pub fn get_memory_manager() -> &'static MemoryManager {
    MEMORY_MANAGER.get().expect("Memory manager not initialized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager_creation() {
        let config = MemoryConfig::default();
        let manager = MemoryManager::new(config);
        assert!(manager.get_pool("test").is_ok());
    }

    #[test]
    fn test_memory_pool_reuse() {
        let config = MemoryConfig::default();
        let manager = MemoryManager::new(config);
        
        let pool1 = manager.get_pool("shared").unwrap();
        let pool2 = manager.get_pool("shared").unwrap();
        
        // Should be the same pool instance
        assert_eq!(pool1.name(), pool2.name());
    }
}