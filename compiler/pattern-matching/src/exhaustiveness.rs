//! Exhaustiveness checking for pattern matching

use crate::{PatternResult, PatternContext};
use crate::pattern_compiler::{InputPattern, InputPatternKind, PatternLiteral};
use shared_types::Span;
use std::collections::{HashSet, HashMap};

/// Check if patterns are exhaustive
pub fn check_exhaustiveness(
    patterns: &[InputPattern],
    _context: &PatternContext,
) -> PatternResult<bool> {
    if patterns.is_empty() {
        return Ok(false);
    }
    
    // Simple exhaustiveness check - look for wildcard or catch-all patterns
    let mut has_wildcard = false;
    let mut covered_literals = HashSet::new();
    let mut covered_constructors = HashSet::new();
    
    for pattern in patterns {
        if covers_all_values(&pattern.kind, &mut covered_literals, &mut covered_constructors) {
            has_wildcard = true;
            break;
        }
    }
    
    // For now, simple heuristic: exhaustive if has wildcard or identifier pattern
    Ok(has_wildcard)
}

/// Check if a pattern covers all possible values
fn covers_all_values(
    pattern: &InputPatternKind,
    covered_literals: &mut HashSet<String>,
    covered_constructors: &mut HashSet<String>,
) -> bool {
    match pattern {
        InputPatternKind::Wildcard => true,
        InputPatternKind::Identifier(_) => true,
        
        InputPatternKind::Literal(lit) => {
            let key = match lit {
                PatternLiteral::Int(n) => format!("int_{}", n),
                PatternLiteral::Float(f) => format!("float_{}", f),
                PatternLiteral::String(s) => format!("string_{}", s),
                PatternLiteral::Bool(b) => format!("bool_{}", b),
                PatternLiteral::Char(c) => format!("char_{}", c),
            };
            covered_literals.insert(key);
            false // Single literal doesn't cover all
        },
        
        InputPatternKind::Tuple(elements) => {
            // Tuple is exhaustive if all elements are exhaustive
            elements.iter().all(|elem| covers_all_values(&elem.kind, covered_literals, covered_constructors))
        },
        
        InputPatternKind::Struct { name, fields, rest } => {
            covered_constructors.insert(name.clone());
            *rest || fields.iter().all(|(_, field_pat)| {
                field_pat.as_ref().map_or(true, |pat| covers_all_values(&pat.kind, covered_literals, covered_constructors))
            })
        },
        
        InputPatternKind::Enum { variant, payload } => {
            covered_constructors.insert(variant.clone());
            payload.as_ref().map_or(true, |pat| covers_all_values(&pat.kind, covered_literals, covered_constructors))
        },
        
        InputPatternKind::Array { elements, rest } => {
            rest.is_some() && elements.iter().all(|elem| covers_all_values(&elem.kind, covered_literals, covered_constructors))
        },
        
        InputPatternKind::Or(patterns) => {
            patterns.iter().any(|pat| covers_all_values(&pat.kind, covered_literals, covered_constructors))
        },
        
        InputPatternKind::Range { .. } => {
            // Ranges don't typically cover all values unless it's a full range
            false
        },
        
        InputPatternKind::TensorShape { dimensions } => {
            // Tensor shapes with all wildcards could be exhaustive
            dimensions.iter().all(|dim| matches!(dim, crate::pattern_compiler::TensorDimPattern::Wildcard))
        },
    }
}

/// Generate missing patterns for exhaustiveness
pub fn generate_missing_patterns(
    patterns: &[InputPattern],
    _context: &PatternContext,
) -> Vec<InputPattern> {
    let mut missing = Vec::new();
    
    // Analyze what's covered
    let mut analysis = ExhaustivenessAnalysis::new();
    for pattern in patterns {
        analysis.add_pattern(pattern);
    }
    
    // Generate missing patterns based on analysis
    if !analysis.has_wildcard && !analysis.has_catch_all {
        // Add wildcard pattern as missing
        missing.push(InputPattern {
            kind: InputPatternKind::Wildcard,
            guard: None,
            span: Span::new(0, 0),
        });
    }
    
    missing
}

/// Analysis state for exhaustiveness checking
struct ExhaustivenessAnalysis {
    has_wildcard: bool,
    has_catch_all: bool,
    literal_coverage: HashMap<String, HashSet<String>>,
    constructor_coverage: HashMap<String, HashSet<String>>,
}

impl ExhaustivenessAnalysis {
    fn new() -> Self {
        Self {
            has_wildcard: false,
            has_catch_all: false,
            literal_coverage: HashMap::new(),
            constructor_coverage: HashMap::new(),
        }
    }
    
    fn add_pattern(&mut self, pattern: &InputPattern) {
        self.analyze_pattern(&pattern.kind);
    }
    
    fn analyze_pattern(&mut self, pattern: &InputPatternKind) {
        match pattern {
            InputPatternKind::Wildcard | InputPatternKind::Identifier(_) => {
                self.has_wildcard = true;
                self.has_catch_all = true;
            },
            
            InputPatternKind::Literal(lit) => {
                let type_key = match lit {
                    PatternLiteral::Int(_) => "int",
                    PatternLiteral::Float(_) => "float", 
                    PatternLiteral::String(_) => "string",
                    PatternLiteral::Bool(_) => "bool",
                    PatternLiteral::Char(_) => "char",
                };
                
                let value_key = match lit {
                    PatternLiteral::Int(n) => n.to_string(),
                    PatternLiteral::Float(f) => f.to_string(),
                    PatternLiteral::String(s) => s.clone(),
                    PatternLiteral::Bool(b) => b.to_string(),
                    PatternLiteral::Char(c) => c.to_string(),
                };
                
                self.literal_coverage
                    .entry(type_key.to_string())
                    .or_default()
                    .insert(value_key);
            },
            
            InputPatternKind::Struct { name, .. } | InputPatternKind::Enum { variant: name, .. } => {
                self.constructor_coverage
                    .entry("constructors".to_string())
                    .or_default()
                    .insert(name.clone());
            },
            
            InputPatternKind::Tuple(elements) => {
                for element in elements {
                    self.analyze_pattern(&element.kind);
                }
            },
            
            InputPatternKind::Array { elements, .. } => {
                for element in elements {
                    self.analyze_pattern(&element.kind);
                }
            },
            
            InputPatternKind::Or(patterns) => {
                for pat in patterns {
                    self.analyze_pattern(&pat.kind);
                }
            },
            
            _ => {
                // Other patterns don't affect exhaustiveness analysis significantly
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatternContext;
    
    #[test]
    fn test_wildcard_exhaustiveness() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Wildcard,
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let exhaustive = check_exhaustiveness(&patterns, &context).unwrap();
        assert!(exhaustive);
    }
    
    #[test]
    fn test_identifier_exhaustiveness() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Identifier("x".to_string()),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let exhaustive = check_exhaustiveness(&patterns, &context).unwrap();
        assert!(exhaustive);
    }
    
    #[test]
    fn test_literal_non_exhaustiveness() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(1)),
                guard: None,
                span: Span::new(0, 0),
            },
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(2)),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let exhaustive = check_exhaustiveness(&patterns, &context).unwrap();
        assert!(!exhaustive);
    }
    
    #[test]
    fn test_missing_pattern_generation() {
        let patterns = vec![
            InputPattern {
                kind: InputPatternKind::Literal(PatternLiteral::Int(1)),
                guard: None,
                span: Span::new(0, 0),
            }
        ];
        
        let context = PatternContext::new();
        let missing = generate_missing_patterns(&patterns, &context);
        assert!(!missing.is_empty());
        assert!(matches!(missing[0].kind, InputPatternKind::Wildcard));
    }
}