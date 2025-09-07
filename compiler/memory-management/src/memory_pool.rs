//! Memory pool implementation for high-performance allocations

use crate::MemoryError;
use parking_lot::Mutex;
use std::alloc::{self, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A memory pool for efficient allocation of ML workloads
#[derive(Debug)]
pub struct MemoryPool {
    name: String,
    chunks: std::sync::Arc<Mutex<Vec<MemoryChunk>>>,
    total_size: AtomicUsize,
    used_size: AtomicUsize,
    max_size: usize,
}

#[derive(Debug)]
struct MemoryChunk {
    ptr: NonNull<u8>,
    size: usize,
    used: usize,
}

// SAFETY: MemoryChunk is only used within properly synchronized contexts (Mutex)
// and the memory pointed to by ptr is only accessed from within the pool
unsafe impl Send for MemoryChunk {}
unsafe impl Sync for MemoryChunk {}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new(name: &str, max_size: usize) -> Result<Self, MemoryError> {
        if max_size == 0 {
            return Err(MemoryError::InvalidSize { size: max_size });
        }

        let initial_chunk = Self::allocate_chunk(max_size / 4)?; // Start with 1/4 of max
        
        Ok(Self {
            name: name.to_string(),
            chunks: std::sync::Arc::new(Mutex::new(vec![initial_chunk])),
            total_size: AtomicUsize::new(max_size / 4),
            used_size: AtomicUsize::new(0),
            max_size,
        })
    }

    /// Get the pool name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Allocate memory from the pool
    pub fn allocate(&self, size: usize, align: usize) -> Result<NonNull<u8>, MemoryError> {
        if size == 0 {
            return Err(MemoryError::InvalidSize { size });
        }

        let layout = Layout::from_size_align(size, align)
            .map_err(|_| MemoryError::InvalidSize { size })?;

        let mut chunks = self.chunks.lock();
        
        // Try to allocate from existing chunks
        for chunk in chunks.iter_mut() {
            if let Some(ptr) = chunk.try_allocate(layout) {
                self.used_size.fetch_add(size, Ordering::Relaxed);
                return Ok(ptr);
            }
        }

        // Need a new chunk
        let chunk_size = (size.max(4096) + 4095) & !4095; // Round up to 4KB
        let new_total = self.total_size.load(Ordering::Relaxed) + chunk_size;
        
        if new_total > self.max_size {
            return Err(MemoryError::PoolExhausted(format!(
                "Pool '{}' would exceed max size {} bytes", 
                self.name, self.max_size
            )));
        }

        let mut new_chunk = Self::allocate_chunk(chunk_size)?;
        let ptr = new_chunk.try_allocate(layout).unwrap(); // Must succeed on empty chunk
        
        chunks.push(new_chunk);
        self.total_size.store(new_total, Ordering::Relaxed);
        self.used_size.fetch_add(size, Ordering::Relaxed);
        
        Ok(ptr)
    }

    /// Reset the pool (for reuse)
    pub fn reset(&self) {
        let mut chunks = self.chunks.lock();
        for chunk in chunks.iter_mut() {
            chunk.reset();
        }
        self.used_size.store(0, Ordering::Relaxed);
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            name: self.name.clone(),
            total_size: self.total_size.load(Ordering::Relaxed),
            used_size: self.used_size.load(Ordering::Relaxed),
            max_size: self.max_size,
            chunks: self.chunks.lock().len(),
        }
    }

    fn allocate_chunk(size: usize) -> Result<MemoryChunk, MemoryError> {
        let layout = Layout::from_size_align(size, 64) // 64-byte alignment for SIMD
            .map_err(|_| MemoryError::InvalidSize { size })?;

        let ptr = unsafe { alloc::alloc(layout) };
        if ptr.is_null() {
            return Err(MemoryError::InvalidSize { size });
        }

        Ok(MemoryChunk {
            ptr: unsafe { NonNull::new_unchecked(ptr) },
            size,
            used: 0,
        })
    }
}

impl Drop for MemoryPool {
    fn drop(&mut self) {
        let chunks = self.chunks.lock();
        for chunk in chunks.iter() {
            unsafe {
                let layout = Layout::from_size_align(chunk.size, 64).unwrap();
                alloc::dealloc(chunk.ptr.as_ptr(), layout);
            }
        }
    }
}

impl MemoryChunk {
    fn try_allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let align_mask = layout.align() - 1;
        let current_ptr = self.ptr.as_ptr() as usize + self.used;
        let aligned_ptr = (current_ptr + align_mask) & !align_mask;
        let end_ptr = aligned_ptr + layout.size();

        if end_ptr <= self.ptr.as_ptr() as usize + self.size {
            self.used = end_ptr - self.ptr.as_ptr() as usize;
            unsafe { Some(NonNull::new_unchecked(aligned_ptr as *mut u8)) }
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.used = 0;
    }
}

/// Memory pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub name: String,
    pub total_size: usize,
    pub used_size: usize,
    pub max_size: usize,
    pub chunks: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pool_creation() {
        let pool = MemoryPool::new("test", 1024 * 1024).unwrap();
        assert_eq!(pool.name(), "test");
    }

    #[test]
    fn test_memory_allocation() {
        let pool = MemoryPool::new("test", 1024 * 1024).unwrap();
        let ptr = pool.allocate(100, 8).unwrap();
        assert!(!ptr.as_ptr().is_null());
    }

    #[test]
    fn test_pool_exhaustion() {
        let pool = MemoryPool::new("test", 1024).unwrap();
        // Try to allocate more than pool size
        let result = pool.allocate(2048, 8);
        assert!(matches!(result, Err(MemoryError::PoolExhausted(_))));
    }

    #[test]
    fn test_pool_reset() {
        let pool = MemoryPool::new("test", 1024 * 1024).unwrap();
        pool.allocate(100, 8).unwrap();
        assert!(pool.stats().used_size > 0);
        
        pool.reset();
        assert_eq!(pool.stats().used_size, 0);
    }
}