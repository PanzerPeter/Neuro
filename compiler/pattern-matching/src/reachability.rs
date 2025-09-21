//! Reachability analysis for pattern matching

use crate::{PatternResult, PatternContext};
use crate::pattern_compiler::{InputPattern, InputPatternKind, PatternLiteral};
use std::collections::HashSet;

/// Check for unreachable patterns
pub fn check_reachability(
    patterns: &[InputPattern],
    _context: &PatternContext,
) -> PatternResult<Vec<usize>> {
    let mut unreachable = Vec::new();
    let mut reachability_state = ReachabilityState::new();
    
    for (i, pattern) in patterns.iter().enumerate() {
        if reachability_state.is_pattern_unreachable(&pattern.kind) {
            unreachable.push(i);
        } else {
            reachability_state.add_pattern(&pattern.kind);
        }
    }
    
    Ok(unreachable)
}

/// State tracking for reachability analysis
struct ReachabilityState {
    /// Whether we've seen a catch-all pattern
    has_catch_all: bool,
    /// Covered literal values by type
    covered_literals: HashSet<String>,
    /// Covered constructors
    covered_constructors: HashSet<String>,
}

impl ReachabilityState {
    fn new() -> Self {
        Self {
            has_catch_all: false,
            covered_literals: HashSet::new(),
            covered_constructors: HashSet::new(),
        }
    }
    
    /// Check if a pattern is unreachable given current state
    fn is_pattern_unreachable(&self, pattern: &InputPatternKind) -> bool {
        // If we've already seen a catch-all, everything else is unreachable
        if self.has_catch_all {
            return true;
        }
        
        match pattern {
            InputPatternKind::Wildcard | InputPatternKind::Identifier(_) => {
                // These are catch-all patterns, unreachable if we already have one
                self.has_catch_all
            },
            
            InputPatternKind::Literal(lit) => {
                let key = literal_key(lit);
                self.covered_literals.contains(&key)
            },
            
            InputPatternKind::Struct { name, .. } | InputPatternKind::Enum { variant: name, .. } => {
                self.covered_constructors.contains(name)
            },
            
            InputPatternKind::Tuple(elements) => {
                // Tuple is unreachable if any element pattern is unreachable
                // This is a simplified analysis
                elements.iter().any(|elem| self.is_pattern_unreachable(&elem.kind))
            },
            
            InputPatternKind::Array { elements, .. } => {
                // Similar to tuples
                elements.iter().any(|elem| self.is_pattern_unreachable(&elem.kind))
            },
            
            InputPatternKind::Or(patterns) => {
                // Or-pattern is unreachable if all alternatives are unreachable
                patterns.iter().all(|pat| self.is_pattern_unreachable(&pat.kind))
            },
            
            InputPatternKind::Range { .. } => {
                // Range patterns are complex to analyze, simplified for now
                false
            },
            
            InputPatternKind::TensorShape { .. } => {
                // Tensor shape patterns are domain-specific
                false
            },
        }
    }
    
    /// Add a pattern to the reachability state
    fn add_pattern(&mut self, pattern: &InputPatternKind) {
        match pattern {
            InputPatternKind::Wildcard | InputPatternKind::Identifier(_) => {
                self.has_catch_all = true;
            },
            
            InputPatternKind::Literal(lit) => {
                let key = literal_key(lit);
                self.covered_literals.insert(key);
            },
            
            InputPatternKind::Struct { name, .. } | InputPatternKind::Enum { variant: name, .. } => {
                self.covered_constructors.insert(name.clone());
            },
            
            InputPatternKind::Tuple(elements) => {
                for element in elements {
                    self.add_pattern(&element.kind);
                }
            },
            
            InputPatternKind::Array { elements, .. } => {
                for element in elements {
                    self.add_pattern(&element.kind);
                }
            },
            
            InputPatternKind::Or(patterns) => {
                for pat in patterns {
                    self.add_pattern(&pat.kind);
                }
            },
            
            _ => {
                // Other patterns don't significantly affect reachability
            }
        }
    }
}

/// Generate a unique key for a literal pattern
fn literal_key(lit: &PatternLiteral) -> String {
    match lit {
        PatternLiteral::Int(n) => format!("int_{}", n),
        PatternLiteral::Float(f) => format!("float_{}", f),
        PatternLiteral::String(s) => format!("string_{}", s),
        PatternLiteral::Bool(b) => format!("bool_{}", b),
        PatternLiteral::Char(c) => format!("char_{}", *c as u32),
    }
}

/// Detailed reachability analysis with path tracking
pub struct DetailedReachabilityAnalyzer {
    pattern_paths: Vec<PatternPath>,
}

#[derive(Debug, Clone)]
struct PatternPath {
    #[allow(dead_code)]
    pattern_id: usize,
    conditions: Vec<PathCondition>,
}

#[derive(Debug, Clone)]
enum PathCondition {
    LiteralEquals(String),
    ConstructorMatch(String),
    FieldAccess(String),
    TupleIndex(usize),
}

impl DetailedReachabilityAnalyzer {
    pub fn new() -> Self {
        Self {
            pattern_paths: Vec::new(),
        }
    }
    
    /// Analyze reachability with detailed path tracking
    pub fn analyze(&mut self, patterns: &[InputPattern]) -> PatternResult<Vec<usize>> {
        let mut unreachable = Vec::new();
        
        for (i, pattern) in patterns.iter().enumerate() {
            let path = self.extract_pattern_path(i, &pattern.kind);
            
            if self.is_path_dominated(&path) {
                unreachable.push(i);
            } else {
                self.pattern_paths.push(path);
            }
        }
        
        Ok(unreachable)
    }
    
    /// Extract the logical path for a pattern
    fn extract_pattern_path(&self, id: usize, pattern: &InputPatternKind) -> PatternPath {
        let mut conditions = Vec::new();
        self.extract_conditions(pattern, &mut conditions);
        
        PatternPath {
            pattern_id: id,
            conditions,
        }
    }
    
    /// Extract conditions from a pattern recursively
    fn extract_conditions(&self, pattern: &InputPatternKind, conditions: &mut Vec<PathCondition>) {
        match pattern {
            InputPatternKind::Literal(lit) => {
                conditions.push(PathCondition::LiteralEquals(literal_key(lit)));
            },
            
            InputPatternKind::Struct { name, fields, .. } => {
                conditions.push(PathCondition::ConstructorMatch(name.clone()));
                for (field_name, field_pattern) in fields {
                    conditions.push(PathCondition::FieldAccess(field_name.clone()));
                    if let Some(pat) = field_pattern {
                        self.extract_conditions(&pat.kind, conditions);
                    }
                }
            },
            
            InputPatternKind::Enum { variant, payload } => {
                conditions.push(PathCondition::ConstructorMatch(variant.clone()));
                if let Some(payload_pat) = payload {
                    self.extract_conditions(&payload_pat.kind, conditions);
                }
            },
            
            InputPatternKind::Tuple(elements) => {
                for (i, element) in elements.iter().enumerate() {
                    conditions.push(PathCondition::TupleIndex(i));
                    self.extract_conditions(&element.kind, conditions);
                }
            },
            
            _ => {
                // Other patterns don't add specific conditions
            }
        }
    }
    
    /// Check if a path is dominated by existing paths
    fn is_path_dominated(&self, path: &PatternPath) -> bool {
        for existing_path in &self.pattern_paths {
            if self.dominates(existing_path, path) {
                return true;
            }
        }
        false
    }
    
    /// Check if path1 dominates path2 (makes it unreachable)
    fn dominates(&self, path1: &PatternPath, path2: &PatternPath) -> bool {
        // Simplified domination check
        // Path1 dominates path2 if path1 is more general
        
        if path1.conditions.is_empty() {
            // Empty path (wildcard) dominates everything
            return true;
        }
        
        // Check if all conditions in path2 are subsumed by path1
        for condition2 in &path2.conditions {
            let mut found_match = false;
            for condition1 in &path1.conditions {
                if conditions_compatible(condition1, condition2) {
                    found_match = true;
                    break;
                }
            }
            if !found_match {
                return false;
            }
        }
        
        true
    }
}

/// Check if two path conditions are compatible
fn conditions_compatible(cond1: &PathCondition, cond2: &PathCondition) -> bool {
    match (cond1, cond2) {
        (PathCondition::LiteralEquals(a), PathCondition::LiteralEquals(b)) => a == b,
        (PathCondition::ConstructorMatch(a), PathCondition::ConstructorMatch(b)) => a == b,
        (PathCondition::FieldAccess(a), PathCondition::FieldAccess(b)) => a == b,
        (PathCondition::TupleIndex(a), PathCondition::TupleIndex(b)) => a == b,
        _ => false,
    }
}

impl Default for DetailedReachabilityAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatternContext;
    use shared_types::Span;
    
    #[test]
    fn test_basic_reachability() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(1)),
                guard: None,
                span: Span::new(0, 0),
            },
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(1)),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let unreachable = check_reachability(&patterns, &context).unwrap();
        assert_eq!(unreachable, vec![1]); // Second pattern is unreachable
    }
    
    #[test]
    fn test_wildcard_makes_all_unreachable() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Wildcard,
                guard: None,
                span: Span::new(0, 0),
            },
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(1)),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let unreachable = check_reachability(&patterns, &context).unwrap();
        assert_eq!(unreachable, vec![1]); // Literal after wildcard is unreachable
    }
    
    #[test]
    fn test_detailed_analyzer() {
        let mut analyzer = DetailedReachabilityAnalyzer::new();
        
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(42)),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let unreachable = analyzer.analyze(&patterns).unwrap();
        assert!(unreachable.is_empty());
    }
}