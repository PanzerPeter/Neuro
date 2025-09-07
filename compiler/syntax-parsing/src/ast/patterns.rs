//! Pattern AST nodes for NEURO language

use crate::ast::{Literal, Identifier};
use shared_types::Span;
use serde::{Deserialize, Serialize};

/// A pattern used in match expressions, function parameters, and destructuring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
    pub type_hint: Option<Box<crate::ast::TypeAnnotation>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PatternKind {
    /// Wildcard pattern `_`
    Wildcard,
    
    /// Identifier pattern `x`
    Identifier(Identifier),
    
    /// Literal pattern `42`, `"hello"`, `true`
    Literal(Literal),
    
    /// Tuple pattern `(x, y, z)`
    Tuple(Vec<Pattern>),
    
    /// Array pattern `[x, y, z]` or `[head, ..tail]`
    Array {
        elements: Vec<Pattern>,
        rest: Option<Box<Pattern>>, // For spread patterns
    },
    
    /// Struct pattern `Point { x, y }` or `Point { x: px, y: py }`
    Struct {
        name: Identifier,
        fields: Vec<FieldPattern>,
        rest: bool, // For `..` in struct patterns
    },
    
    /// Enum pattern `Some(x)` or `None`
    Enum {
        variant: Identifier,
        payload: Option<Box<Pattern>>,
    },
    
    /// Range pattern `1..=10` or `'a'..='z'`
    Range {
        start: Box<Pattern>,
        end: Box<Pattern>,
        inclusive: bool,
    },
    
    /// Or pattern `x | y | z`
    Or(Vec<Pattern>),
    
    /// Guard pattern with condition `x if x > 0`
    Guard {
        pattern: Box<Pattern>,
        condition: Box<crate::ast::Expression>,
    },
    
    /// Tensor shape pattern for ML types `tensor[?, 128, 64]`
    TensorShape {
        dimensions: Vec<TensorDimPattern>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldPattern {
    pub name: Identifier,
    pub pattern: Option<Pattern>, // None for shorthand like `{ x, y }`
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TensorDimPattern {
    /// Fixed dimension `128`
    Fixed(usize),
    /// Wildcard dimension `?`
    Wildcard,
    /// Named dimension `n` (binds to variable)
    Named(Identifier),
    /// Range dimension `1..10`
    Range { start: usize, end: usize },
}

impl Pattern {
    pub fn new(kind: PatternKind, span: Span) -> Self {
        Self {
            kind,
            span,
            type_hint: None,
        }
    }
    
    pub fn with_type(mut self, type_hint: crate::ast::TypeAnnotation) -> Self {
        self.type_hint = Some(Box::new(type_hint));
        self
    }
    
    /// Returns true if this pattern is irrefutable (always matches)
    pub fn is_irrefutable(&self) -> bool {
        match &self.kind {
            PatternKind::Wildcard | PatternKind::Identifier(_) => true,
            PatternKind::Tuple(patterns) => patterns.iter().all(|p| p.is_irrefutable()),
            PatternKind::Array { elements, rest } => {
                elements.iter().all(|p| p.is_irrefutable()) && rest.is_none()
            }
            PatternKind::Struct { fields, rest: false } => {
                fields.iter().all(|f| f.pattern.as_ref().map_or(true, |p| p.is_irrefutable()))
            }
            _ => false,
        }
    }
    
    /// Get all identifiers bound by this pattern
    pub fn bound_identifiers(&self) -> Vec<&Identifier> {
        let mut ids = Vec::new();
        self.collect_identifiers(&mut ids);
        ids
    }
    
    fn collect_identifiers(&self, ids: &mut Vec<&Identifier>) {
        match &self.kind {
            PatternKind::Identifier(id) => ids.push(id),
            PatternKind::Tuple(patterns) => {
                for pattern in patterns {
                    pattern.collect_identifiers(ids);
                }
            }
            PatternKind::Array { elements, rest } => {
                for pattern in elements {
                    pattern.collect_identifiers(ids);
                }
                if let Some(rest_pattern) = rest {
                    rest_pattern.collect_identifiers(ids);
                }
            }
            PatternKind::Struct { fields, .. } => {
                for field in fields {
                    if let Some(pattern) = &field.pattern {
                        pattern.collect_identifiers(ids);
                    } else {
                        ids.push(&field.name);
                    }
                }
            }
            PatternKind::Enum { payload, .. } => {
                if let Some(payload_pattern) = payload {
                    payload_pattern.collect_identifiers(ids);
                }
            }
            PatternKind::Or(patterns) => {
                // For or-patterns, we need to ensure all branches bind the same variables
                for pattern in patterns {
                    pattern.collect_identifiers(ids);
                }
            }
            PatternKind::Guard { pattern, .. } => {
                pattern.collect_identifiers(ids);
            }
            PatternKind::TensorShape { dimensions } => {
                for dim in dimensions {
                    if let TensorDimPattern::Named(id) = dim {
                        ids.push(id);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A match arm containing a pattern and optional guard
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<crate::ast::Expression>,
    pub body: crate::ast::Expression,
    pub span: Span,
}