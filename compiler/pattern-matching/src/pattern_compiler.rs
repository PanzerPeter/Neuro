//! Main pattern compiler interface

use crate::{
    decision_tree::{DecisionTree, DecisionTreeBuilder, GuardCondition, TestValue},
    exhaustiveness::check_exhaustiveness,
    reachability::check_reachability,
    PatternError, PatternResult, PatternContext, CompilerConfig,
};
use shared_types::Span;

/// High-level pattern compiler
pub struct PatternCompiler {
    config: CompilerConfig,
}

/// Compiled match expression
#[derive(Debug, Clone)]
pub struct CompiledMatch {
    pub decision_tree: DecisionTree,
    pub is_exhaustive: bool,
    pub unreachable_patterns: Vec<usize>,
    pub bound_variables: Vec<String>,
}

/// Input pattern for compilation
#[derive(Debug, Clone)]
pub struct InputPattern {
    pub kind: InputPatternKind,
    pub guard: Option<String>, // Simplified guard expression
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum InputPatternKind {
    Wildcard,
    Identifier(String),
    Literal(PatternLiteral),
    Tuple(Vec<InputPattern>),
    Array { 
        elements: Vec<InputPattern>,
        rest: Option<Box<InputPattern>>,
    },
    Struct {
        name: String,
        fields: Vec<(String, Option<InputPattern>)>,
        rest: bool,
    },
    Enum {
        variant: String,
        payload: Option<Box<InputPattern>>,
    },
    Or(Vec<InputPattern>),
    Range {
        start: PatternLiteral,
        end: PatternLiteral,
        inclusive: bool,
    },
    TensorShape {
        dimensions: Vec<TensorDimPattern>,
    },
}

#[derive(Debug, Clone)]
pub enum PatternLiteral {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
}

#[derive(Debug, Clone)]
pub enum TensorDimPattern {
    Fixed(usize),
    Wildcard,
    Named(String),
    Range { start: usize, end: usize },
}

impl PatternCompiler {
    pub fn new(config: CompilerConfig) -> Self {
        Self { config }
    }
    
    pub fn with_default_config() -> Self {
        Self::new(CompilerConfig::default())
    }
    
    /// Compile a match expression
    pub fn compile_match(
        &self,
        patterns: Vec<InputPattern>,
        context: &mut PatternContext,
    ) -> PatternResult<CompiledMatch> {
        // Build decision tree
        let mut builder = DecisionTreeBuilder::new();
        let mut pattern_spans = Vec::new();
        
        for pattern in patterns.iter() {
            let compiled = self.compile_pattern(pattern, context)?;
            let guard = pattern.guard.as_ref().map(|expr| GuardCondition {
                expression: expr.clone(),
                bindings: Vec::new(), // Would extract from expression
            });
            
            builder.add_pattern(compiled, guard, pattern.span);
            pattern_spans.push(pattern.span);
        }
        
        let decision_tree = builder.build()?;
        
        // Run analysis passes if configured
        let is_exhaustive = if self.config.check_exhaustiveness {
            check_exhaustiveness(&patterns, context)?
        } else {
            true // Assume exhaustive if not checking
        };
        
        let unreachable_patterns = if self.config.check_reachability {
            check_reachability(&patterns, context)?
        } else {
            Vec::new()
        };
        
        // Extract bound variables
        let bound_variables = context.bound_vars.keys().cloned().collect();
        
        Ok(CompiledMatch {
            decision_tree,
            is_exhaustive,
            unreachable_patterns,
            bound_variables,
        })
    }
    
    /// Compile a single pattern
    fn compile_pattern(
        &self,
        pattern: &InputPattern,
        context: &mut PatternContext,
    ) -> PatternResult<crate::decision_tree::CompiledPattern> {
        use crate::decision_tree::CompiledPattern;
        
        match &pattern.kind {
            InputPatternKind::Wildcard => Ok(CompiledPattern::Wildcard),
            
            InputPatternKind::Identifier(name) => {
                // Bind variable in context
                context.bind_var(name.clone(), crate::PatternVarInfo {
                    name: name.clone(),
                    type_hint: None,
                    span: pattern.span,
                })?;
                Ok(CompiledPattern::Variable(name.clone()))
            },
            
            InputPatternKind::Literal(lit) => {
                let test_val = match lit {
                    PatternLiteral::Int(n) => TestValue::Int(*n),
                    PatternLiteral::Float(f) => TestValue::Float(*f),
                    PatternLiteral::String(s) => TestValue::String(s.clone()),
                    PatternLiteral::Bool(b) => TestValue::Bool(*b),
                    PatternLiteral::Char(c) => TestValue::Int(*c as i64),
                };
                Ok(CompiledPattern::Literal(test_val))
            },
            
            InputPatternKind::Tuple(elements) => {
                let mut subpatterns = Vec::new();
                for element in elements {
                    subpatterns.push(self.compile_pattern(element, context)?);
                }
                Ok(CompiledPattern::Constructor {
                    name: "Tuple".to_string(),
                    arity: elements.len(),
                    subpatterns,
                })
            },
            
            InputPatternKind::Struct { name, fields, .. } => {
                let mut subpatterns = Vec::new();
                for (field_name, field_pattern) in fields {
                    if let Some(pattern) = field_pattern {
                        subpatterns.push(self.compile_pattern(pattern, context)?);
                    } else {
                        // Shorthand field binding
                        context.bind_var(field_name.clone(), crate::PatternVarInfo {
                            name: field_name.clone(),
                            type_hint: None,
                            span: pattern.span,
                        })?;
                        subpatterns.push(CompiledPattern::Variable(field_name.clone()));
                    }
                }
                Ok(CompiledPattern::Constructor {
                    name: name.clone(),
                    arity: fields.len(),
                    subpatterns,
                })
            },
            
            InputPatternKind::Enum { variant, payload } => {
                let subpatterns = if let Some(payload_pattern) = payload {
                    vec![self.compile_pattern(payload_pattern, context)?]
                } else {
                    Vec::new()
                };
                Ok(CompiledPattern::Constructor {
                    name: variant.clone(),
                    arity: subpatterns.len(),
                    subpatterns,
                })
            },
            
            InputPatternKind::Or(patterns) => {
                let mut compiled_patterns = Vec::new();
                for pat in patterns {
                    compiled_patterns.push(self.compile_pattern(pat, context)?);
                }
                Ok(CompiledPattern::Or(compiled_patterns))
            },
            
            InputPatternKind::Array { elements, rest: _ } => {
                let mut subpatterns = Vec::new();
                for element in elements {
                    subpatterns.push(self.compile_pattern(element, context)?);
                }
                Ok(CompiledPattern::Constructor {
                    name: "Array".to_string(),
                    arity: elements.len(),
                    subpatterns,
                })
            },
            
            InputPatternKind::Range { start, end, inclusive } => {
                // For now, convert range to a special constructor
                // In a full implementation, this would be handled in decision tree generation
                let start_val = match start {
                    PatternLiteral::Int(n) => TestValue::Int(*n),
                    PatternLiteral::Char(c) => TestValue::Int(*c as i64),
                    _ => return Err(PatternError::UnsupportedPattern {
                        pattern_type: "non-integer range".to_string(),
                        span: pattern.span,
                    }),
                };
                
                let end_val = match end {
                    PatternLiteral::Int(n) => TestValue::Int(*n),
                    PatternLiteral::Char(c) => TestValue::Int(*c as i64),
                    _ => return Err(PatternError::UnsupportedPattern {
                        pattern_type: "non-integer range".to_string(),
                        span: pattern.span,
                    }),
                };
                
                // Simplified: convert to literal for now
                // Real implementation would handle ranges in decision tree
                Ok(CompiledPattern::Literal(start_val))
            },
            
            InputPatternKind::TensorShape { dimensions } => {
                // For tensor shapes, create a special constructor
                let mut subpatterns = Vec::new();
                for dim in dimensions {
                    match dim {
                        TensorDimPattern::Fixed(size) => {
                            subpatterns.push(CompiledPattern::Literal(TestValue::Int(*size as i64)));
                        },
                        TensorDimPattern::Wildcard => {
                            subpatterns.push(CompiledPattern::Wildcard);
                        },
                        TensorDimPattern::Named(name) => {
                            context.bind_var(name.clone(), crate::PatternVarInfo {
                                name: name.clone(),
                                type_hint: Some("usize".to_string()),
                                span: pattern.span,
                            })?;
                            subpatterns.push(CompiledPattern::Variable(name.clone()));
                        },
                        TensorDimPattern::Range { start, end } => {
                            // Create a range pattern (simplified)
                            subpatterns.push(CompiledPattern::Literal(TestValue::Int(*start as i64)));
                        },
                    }
                }
                Ok(CompiledPattern::Constructor {
                    name: "TensorShape".to_string(),
                    arity: dimensions.len(),
                    subpatterns,
                })
            },
        }
    }
}

impl Default for PatternCompiler {
    fn default() -> Self {
        Self::with_default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pattern_compiler_creation() {
        let compiler = PatternCompiler::with_default_config();
        assert!(compiler.config.check_exhaustiveness);
        assert!(compiler.config.check_reachability);
    }
    
    #[test]
    fn test_simple_pattern_compilation() {
        let compiler = PatternCompiler::with_default_config();
        let mut context = PatternContext::new();
        
        let pattern = InputPattern {
            kind: InputPatternKind::Literal(PatternLiteral::Int(42)),
            guard: None,
            span: Span::new(0, 0),
        };
        
        let compiled = compiler.compile_pattern(&pattern, &mut context).unwrap();
        assert!(matches!(compiled, crate::decision_tree::CompiledPattern::Literal(_)));
    }
    
    #[test]
    fn test_identifier_pattern_binding() {
        let compiler = PatternCompiler::with_default_config();
        let mut context = PatternContext::new();
        
        let pattern = InputPattern {
            kind: InputPatternKind::Identifier("x".to_string()),
            guard: None,
            span: Span::new(0, 0),
        };
        
        let _compiled = compiler.compile_pattern(&pattern, &mut context).unwrap();
        assert!(context.bound_vars.contains_key("x"));
    }
}