//! Allocation tracking for memory profiling and debugging

use crate::MemoryError;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Allocation tracker for debugging memory usage
#[derive(Debug)]
pub struct AllocationTracker {
    allocations: RwLock<HashMap<usize, AllocationInfo>>,
    next_id: AtomicUsize,
    total_allocated: AtomicUsize,
    total_deallocated: AtomicUsize,
    peak_usage: AtomicUsize,
}

#[derive(Debug, Clone)]
struct AllocationInfo {
    size: usize,
    type_name: String,
    allocated_at: std::time::Instant,
}

impl AllocationTracker {
    /// Create a new allocation tracker
    pub fn new() -> Self {
        Self {
            allocations: RwLock::new(HashMap::new()),
            next_id: AtomicUsize::new(1),
            total_allocated: AtomicUsize::new(0),
            total_deallocated: AtomicUsize::new(0),
            peak_usage: AtomicUsize::new(0),
        }
    }

    /// Track a new allocation
    pub fn track_allocation<T>(&self, size: usize) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let info = AllocationInfo {
            size,
            type_name: std::any::type_name::<T>().to_string(),
            allocated_at: std::time::Instant::now(),
        };

        {
            let mut allocations = self.allocations.write();
            allocations.insert(id, info);
        }

        let total_allocated = self.total_allocated.fetch_add(size, Ordering::Relaxed) + size;
        let current_usage = total_allocated - self.total_deallocated.load(Ordering::Relaxed);
        
        // Update peak usage
        let mut peak = self.peak_usage.load(Ordering::Relaxed);
        while current_usage > peak {
            match self.peak_usage.compare_exchange_weak(peak, current_usage, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }

        id
    }

    /// Track a deallocation
    pub fn track_deallocation(&self, id: usize) -> Result<(), MemoryError> {
        let info = {
            let mut allocations = self.allocations.write();
            allocations.remove(&id).ok_or(MemoryError::DoubleFree)?
        };

        self.total_deallocated.fetch_add(info.size, Ordering::Relaxed);
        Ok(())
    }

    /// Get allocation statistics
    pub fn get_stats(&self) -> AllocationStats {
        let allocations = self.allocations.read();
        let mut by_type = HashMap::new();
        let mut live_size = 0;

        for info in allocations.values() {
            *by_type.entry(info.type_name.clone()).or_insert(0) += 1;
            live_size += info.size;
        }

        AllocationStats {
            live_allocations: allocations.len(),
            live_size,
            total_allocated: self.total_allocated.load(Ordering::Relaxed),
            total_deallocated: self.total_deallocated.load(Ordering::Relaxed),
            peak_usage: self.peak_usage.load(Ordering::Relaxed),
            allocations_by_type: by_type,
        }
    }

    /// Check for memory leaks
    pub fn check_leaks(&self) -> Result<(), MemoryError> {
        let allocations = self.allocations.read();
        if !allocations.is_empty() {
            // Look for long-lived allocations that might be leaks
            let potential_leaks = allocations.values()
                .filter(|info| info.allocated_at.elapsed().as_secs() > 300) // 5 minutes
                .count();
            
            if potential_leaks > 0 {
                return Err(MemoryError::LeakDetected { count: potential_leaks });
            }
        }
        Ok(())
    }

    /// Get memory usage report
    pub fn get_report(&self) -> String {
        let stats = self.get_stats();
        
        format!(
            "Memory Usage Report:\n\
             Live allocations: {} ({} bytes)\n\
             Total allocated: {} bytes\n\
             Total deallocated: {} bytes\n\
             Peak usage: {} bytes\n\
             \n\
             Allocations by type:\n{}",
            stats.live_allocations,
            stats.live_size,
            stats.total_allocated,
            stats.total_deallocated,
            stats.peak_usage,
            stats.allocations_by_type.iter()
                .map(|(ty, count)| format!("  {}: {}", ty, count))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl Default for AllocationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about memory allocations
#[derive(Debug, Clone)]
pub struct AllocationStats {
    pub live_allocations: usize,
    pub live_size: usize,
    pub total_allocated: usize,
    pub total_deallocated: usize,
    pub peak_usage: usize,
    pub allocations_by_type: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_tracking() {
        let tracker = AllocationTracker::new();
        let id = tracker.track_allocation::<i32>(4);
        
        let stats = tracker.get_stats();
        assert_eq!(stats.live_allocations, 1);
        assert_eq!(stats.live_size, 4);
        assert_eq!(stats.total_allocated, 4);
        
        tracker.track_deallocation(id).unwrap();
        
        let stats = tracker.get_stats();
        assert_eq!(stats.live_allocations, 0);
        assert_eq!(stats.live_size, 0);
        assert_eq!(stats.total_deallocated, 4);
    }

    #[test]
    fn test_double_free_detection() {
        let tracker = AllocationTracker::new();
        let id = tracker.track_allocation::<i32>(4);
        
        tracker.track_deallocation(id).unwrap();
        let result = tracker.track_deallocation(id);
        assert!(matches!(result, Err(MemoryError::DoubleFree)));
    }

    #[test]
    fn test_peak_usage_tracking() {
        let tracker = AllocationTracker::new();
        
        let id1 = tracker.track_allocation::<i32>(100);
        let id2 = tracker.track_allocation::<i32>(200);
        
        let stats = tracker.get_stats();
        assert_eq!(stats.peak_usage, 300);
        
        tracker.track_deallocation(id1).unwrap();
        
        let stats = tracker.get_stats();
        assert_eq!(stats.peak_usage, 300); // Peak should remain at maximum
        assert_eq!(stats.live_size, 200);
        
        tracker.track_deallocation(id2).unwrap();
    }
}