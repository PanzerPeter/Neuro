//! Automatic Reference Counting runtime for NEURO

use crate::MemoryError;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::collections::HashMap;

/// Reference counted object for NEURO runtime
pub struct NeuroArc<T> {
    data: Arc<T>,
    id: usize,
}

impl<T> NeuroArc<T> {
    /// Create a new reference counted object
    pub fn new(value: T) -> Self {
        let id = OBJECT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let arc = Arc::new(value);
        
        #[cfg(debug_assertions)]
        LIVE_OBJECTS.write().insert(id, LiveObject {
            type_name: std::any::type_name::<T>().to_string(),
            size: std::mem::size_of::<T>(),
            created_at: std::time::Instant::now(),
        });

        Self {
            data: arc,
            id,
        }
    }

    /// Get a reference to the data
    pub fn as_ref(&self) -> &T {
        self.data.as_ref()
    }

    /// Get the reference count
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.data)
    }

    /// Get the object ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Get the reference count (useful for debugging)
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.data) == 1
    }

    /// Create a weak reference
    pub fn downgrade(&self) -> NeuroWeak<T> {
        NeuroWeak {
            weak: Arc::downgrade(&self.data),
        }
    }
}

impl<T> Clone for NeuroArc<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            id: self.id, // Same object, just more references
        }
    }
}

impl<T> Drop for NeuroArc<T> {
    fn drop(&mut self) {
        // Only remove from live objects when the last reference is dropped
        if Arc::strong_count(&self.data) == 1 {
            #[cfg(debug_assertions)]
            LIVE_OBJECTS.write().remove(&self.id);
        }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for NeuroArc<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuroArc")
            .field("id", &self.id)
            .field("data", self.data.as_ref())
            .field("strong_count", &self.strong_count())
            .finish()
    }
}

/// Weak reference for breaking cycles
pub struct NeuroWeak<T> {
    weak: std::sync::Weak<T>,
}

impl<T> NeuroWeak<T> {
    /// Upgrade to a strong reference if possible
    pub fn upgrade(&self) -> Option<NeuroArc<T>> {
        self.weak.upgrade().map(|arc| {
            // We can't get the original ID here, so we'll use 0 for upgraded references
            // In a real implementation, we might want to track this differently
            NeuroArc {
                data: arc,
                id: 0,
            }
        })
    }
}

/// Statistics about ARC objects (debug builds only)
#[derive(Debug, Clone)]
pub struct ArcStats {
    pub total_objects_created: usize,
    pub live_objects: usize,
    pub objects_by_type: HashMap<String, usize>,
    pub total_memory_estimated: usize,
}

#[cfg(debug_assertions)]
#[derive(Debug, Clone)]
struct LiveObject {
    type_name: String,
    size: usize,
    created_at: std::time::Instant,
}

// Global state for debugging (only in debug builds)
static OBJECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[cfg(debug_assertions)]
use std::sync::LazyLock;

#[cfg(debug_assertions)]
static LIVE_OBJECTS: LazyLock<RwLock<HashMap<usize, LiveObject>>> = LazyLock::new(|| {
    RwLock::new(HashMap::new())
});

/// Get ARC statistics (debug builds only)
pub fn get_arc_stats() -> ArcStats {
    let total_created = OBJECT_COUNTER.load(Ordering::Relaxed);
    
    #[cfg(debug_assertions)]
    {
        let live = LIVE_OBJECTS.read();
        let mut by_type = HashMap::new();
        let mut total_memory = 0;

        for obj in live.values() {
            *by_type.entry(obj.type_name.clone()).or_insert(0) += 1;
            total_memory += obj.size;
        }

        ArcStats {
            total_objects_created: total_created,
            live_objects: live.len(),
            objects_by_type: by_type,
            total_memory_estimated: total_memory,
        }
    }
    
    #[cfg(not(debug_assertions))]
    ArcStats {
        total_objects_created: total_created,
        live_objects: 0, // Not tracked in release builds
        objects_by_type: HashMap::new(),
        total_memory_estimated: 0,
    }
}

/// Check for potential memory leaks (debug builds only)
pub fn check_for_leaks() -> Result<(), MemoryError> {
    #[cfg(debug_assertions)]
    {
        let live = LIVE_OBJECTS.read();
        if !live.is_empty() {
            // In a real application, this might be too strict
            // We could have a more sophisticated leak detection
            let long_lived = live.values()
                .filter(|obj| obj.created_at.elapsed().as_secs() > 60) // Objects alive > 1 minute
                .count();
            
            if long_lived > 0 {
                return Err(MemoryError::LeakDetected { count: long_lived });
            }
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_creation() {
        let arc = NeuroArc::new(42);
        assert_eq!(*arc.as_ref(), 42);
        assert_eq!(arc.strong_count(), 1);
    }

    #[test]
    fn test_arc_cloning() {
        let arc1 = NeuroArc::new(String::from("hello"));
        let arc2 = arc1.clone();
        
        assert_eq!(arc1.id(), arc2.id()); // Same object
        assert_eq!(arc1.strong_count(), 2);
        assert_eq!(arc2.strong_count(), 2);
    }

    #[test]
    fn test_arc_uniqueness() {
        let arc1 = NeuroArc::new(42);
        assert!(arc1.is_unique());
        
        let arc2 = arc1.clone();
        assert!(!arc1.is_unique());
        assert!(!arc2.is_unique());
        
        drop(arc2);
        assert!(arc1.is_unique());
    }

    #[test]
    fn test_weak_references() {
        let arc = NeuroArc::new(42);
        let weak = arc.downgrade();
        
        assert!(weak.upgrade().is_some());
        drop(arc);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn test_arc_stats() {
        let _arc1 = NeuroArc::new(42);
        let _arc2 = NeuroArc::new("hello".to_string());
        
        let stats = get_arc_stats();
        assert!(stats.total_objects_created >= 2);
        
        #[cfg(debug_assertions)]
        assert!(stats.live_objects >= 2);
    }
}