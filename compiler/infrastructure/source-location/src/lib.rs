// NEURO Programming Language - Source Location
// Infrastructure component for source file mapping and position tracking

use shared_types::Span;

/// Represents a position in source code with line and column
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// Source file representation with line mapping
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub content: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(path: String, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);
        Self {
            path,
            content,
            line_starts,
        }
    }

    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    /// Convert byte offset to line and column
    pub fn position_at(&self, offset: usize) -> Position {
        let line = self
            .line_starts
            .binary_search(&offset)
            .unwrap_or_else(|x| x.saturating_sub(1));

        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let column = offset.saturating_sub(line_start);

        Position::new(line as u32 + 1, column as u32 + 1)
    }

    /// Get source text for a span
    pub fn snippet(&self, span: Span) -> &str {
        let start = span.start.min(self.content.len());
        let end = span.end.min(self.content.len());
        &self.content[start..end]
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

        let pos = source.position_at(14); // Start of second line (after newline at pos 13)
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }
}
