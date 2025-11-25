//! NEURO Programming Language - Source Location
//!
//! Infrastructure component for mapping byte offsets to human-readable line/column positions
//! and extracting source code snippets for error reporting.
//!
//! # Overview
//!
//! This crate provides utilities for:
//! - Converting byte offsets to line/column positions
//! - Extracting source code snippets for error messages
//! - Caching line start positions for efficient lookups
//!
//! # Architecture
//!
//! Pure infrastructure with no business logic. Used by the diagnostics system
//! and error reporting throughout the compiler.

use shared_types::Span;

/// Human-readable position in source code (line and column).
///
/// Line and column numbers are 1-indexed to match standard text editor conventions.
/// This is a display-oriented representation, derived from byte offsets.
///
/// # Examples
///
/// ```
/// use source_location::Position;
///
/// let pos = Position::new(10, 5);
/// assert_eq!(pos.line, 10);
/// assert_eq!(pos.column, 5);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub column: u32,
}

impl Position {
    /// Creates a new position with the given line and column numbers.
    ///
    /// Both line and column are 1-indexed.
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// Source file with cached line start positions for efficient position lookups.
///
/// This structure precomputes the byte offset of each line start during construction,
/// enabling O(log n) position lookups via binary search.
///
/// # Examples
///
/// ```
/// use source_location::SourceFile;
/// use shared_types::Span;
///
/// let source = SourceFile::new(
///     "test.nr".to_string(),
///     "line 1\nline 2\nline 3".to_string()
/// );
///
/// let pos = source.position_at(7);  // First char of line 2
/// assert_eq!(pos.line, 2);
/// assert_eq!(pos.column, 1);
///
/// let snippet = source.snippet(Span::new(0, 6));
/// assert_eq!(snippet, Some("line 1"));
/// ```
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// File path (for error messages)
    pub path: String,
    /// Complete source code content
    pub content: String,
    /// Cached byte offsets of line starts (for fast position lookups)
    line_starts: Vec<usize>,
}

impl SourceFile {
    /// Creates a new source file and computes line start positions.
    ///
    /// This performs a single pass over the content to find all newline positions,
    /// enabling efficient position lookups later.
    ///
    /// # Examples
    ///
    /// ```
    /// use source_location::SourceFile;
    ///
    /// let source = SourceFile::new(
    ///     "example.nr".to_string(),
    ///     "func main() {\n  return 0\n}".to_string()
    /// );
    /// ```
    pub fn new(path: String, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);
        Self {
            path,
            content,
            line_starts,
        }
    }

    /// Computes byte offsets of all line starts in the content.
    ///
    /// Returns a vector where each element is the byte offset of the start of that line.
    /// Line 0 always starts at byte 0.
    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    /// Converts a byte offset to a human-readable line and column position.
    ///
    /// Uses binary search over the precomputed line starts for O(log n) performance.
    /// Returns 1-indexed line and column numbers matching text editor conventions.
    ///
    /// # Examples
    ///
    /// ```
    /// use source_location::SourceFile;
    ///
    /// let source = SourceFile::new(
    ///     "test.nr".to_string(),
    ///     "abc\ndef".to_string()
    /// );
    ///
    /// let pos = source.position_at(4);  // 'd' in "def"
    /// assert_eq!(pos.line, 2);
    /// assert_eq!(pos.column, 1);
    /// ```
    pub fn position_at(&self, offset: usize) -> Position {
        let line = self
            .line_starts
            .binary_search(&offset)
            .unwrap_or_else(|x| x.saturating_sub(1));

        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let column = offset.saturating_sub(line_start);

        Position::new(line as u32 + 1, column as u32 + 1)
    }

    /// Extracts a source code snippet for the given span.
    ///
    /// Returns `None` if:
    /// - The span is invalid (start > end)
    /// - The span is out of bounds
    /// - The span doesn't align with UTF-8 character boundaries
    ///
    /// # Examples
    ///
    /// ```
    /// use source_location::SourceFile;
    /// use shared_types::Span;
    ///
    /// let source = SourceFile::new(
    ///     "test.nr".to_string(),
    ///     "hello world".to_string()
    /// );
    ///
    /// assert_eq!(source.snippet(Span::new(0, 5)), Some("hello"));
    /// assert_eq!(source.snippet(Span::new(6, 11)), Some("world"));
    /// assert_eq!(source.snippet(Span::new(5, 0)), None);  // Invalid: start > end
    /// ```
    pub fn snippet(&self, span: Span) -> Option<&str> {
        if span.start > span.end {
            return None;
        }
        self.content.get(span.start..span.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_calculation() {
        let source = SourceFile::new(
            "test.nr".to_string(),
            "func main() {\n  val x = 42\n}".to_string(),
        );

        let pos = source.position_at(14);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn snippet_valid_span() {
        let source = SourceFile::new("test.nr".to_string(), "hello world".to_string());
        let span = Span::new(0, 5);
        assert_eq!(source.snippet(span), Some("hello"));
    }

    #[test]
    fn snippet_invalid_span_reversed() {
        let source = SourceFile::new("test.nr".to_string(), "hello world".to_string());
        let span = Span::new(5, 0);
        assert_eq!(source.snippet(span), None);
    }

    #[test]
    fn snippet_out_of_bounds() {
        let source = SourceFile::new("test.nr".to_string(), "hello".to_string());
        let span = Span::new(0, 100);
        assert_eq!(source.snippet(span), None);
    }

    #[test]
    fn snippet_empty_span() {
        let source = SourceFile::new("test.nr".to_string(), "hello world".to_string());
        let span = Span::new(5, 5);
        assert_eq!(source.snippet(span), Some(""));
    }

    #[test]
    fn snippet_utf8_boundaries() {
        let source = SourceFile::new("test.nr".to_string(), "Hello 世界".to_string());
        let span = Span::new(0, 6);
        assert_eq!(source.snippet(span), Some("Hello "));
    }

    #[test]
    fn position_at_empty_file() {
        let source = SourceFile::new("test.nr".to_string(), String::new());
        let pos = source.position_at(0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    #[test]
    fn position_at_multiline() {
        let source = SourceFile::new("test.nr".to_string(), "line1\nline2\nline3".to_string());
        let pos = source.position_at(12);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.column, 1);
    }
}
