// NEURO Programming Language - Shared Types
// Infrastructure component providing common type definitions across slices

/// Source code span representing a location in the source file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Identifier representation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identifier {
    pub name: String,
    pub span: Span,
}

impl Identifier {
    pub fn new(name: String, span: Span) -> Self {
        Self { name, span }
    }
}

/// Literal value types
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge_works() {
        let span1 = Span::new(0, 5);
        let span2 = Span::new(3, 8);
        let merged = span1.merge(span2);
        assert_eq!(merged, Span::new(0, 8));
    }
}
