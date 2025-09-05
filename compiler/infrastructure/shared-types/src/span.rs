//! Source location span types

use serde::{Deserialize, Serialize};

/// Represents a span of text in the source code
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
    
    pub fn contains(&self, other: &Span) -> bool {
        self.start <= other.start && other.end <= self.end
    }
    
    pub fn merge(&self, other: &Span) -> Span {
        Span::new(
            self.start.min(other.start),
            self.end.max(other.end)
        )
    }
}