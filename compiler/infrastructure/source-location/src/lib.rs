//! Source location tracking infrastructure
//!
//! Provides source mapping and span tracking capabilities
//! for all NEURO compiler slices.

pub use shared_types::{Span};

/// Source map for tracking original source locations
#[derive(Debug, Clone)]
pub struct SourceMap {
    pub content: String,
    pub filename: String,
}

impl SourceMap {
    pub fn new(filename: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            content: content.into(),
        }
    }
    
    pub fn get_line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        
        for (i, ch) in self.content.char_indices() {
            if i >= offset {
                break;
            }
            
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        
        (line, col)
    }
}