//! Decision tree generation for pattern matching

use crate::{PatternError, PatternResult};
use shared_types::Span;
use smallvec::{SmallVec, smallvec};
use std::collections::HashMap;

/// Represents a compiled decision tree for pattern matching
#[derive(Debug, Clone)]
pub struct DecisionTree {
    pub root: DecisionNode,
    pub total_patterns: usize,
}

/// A node in the decision tree
#[derive(Debug, Clone)]
pub enum DecisionNode {
    /// Leaf node - match found, execute body
    Success {
        pattern_id: usize,
        bindings: HashMap<String, Binding>,
    },
    
    /// Failure node - no match, potentially unreachable
    Failure,
    
    /// Switch on a value
    Switch {
        /// Path to the value being tested
        test_path: TestPath,
        /// Test branches
        branches: Vec<SwitchBranch>,
        /// Default case if no branch matches
        default: Option<Box<DecisionNode>>,
    },
    
    /// Test a guard condition
    Guard {
        pattern_id: usize,
        condition: GuardCondition,
        then_branch: Box<DecisionNode>,
        else_branch: Box<DecisionNode>,
    },
}

/// Path to a value in the tested expression
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TestPath {
    /// Components of the path (field names, indices, etc.)
    pub components: SmallVec<[PathComponent; 4]>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum PathComponent {
    /// Field access `.field`
    Field(String),
    /// Tuple index `[0]`
    TupleIndex(usize),
    /// Array index `[i]`
    ArrayIndex(usize),
    /// Array length check
    ArrayLength,
    /// Enum variant check
    EnumVariant(String),
    /// Tensor dimension access
    TensorDim(usize),
}

#[derive(Debug, Clone)]
pub struct SwitchBranch {
    pub test: TestCondition,
    pub node: DecisionNode,
}

/// Condition for testing values
#[derive(Debug, Clone)]
pub enum TestCondition {
    /// Exact value match
    Equals(TestValue),
    /// Range check
    Range { start: TestValue, end: TestValue, inclusive: bool },
    /// Type check (for enum variants)
    IsVariant(String),
    /// Array length check
    HasLength(usize),
    /// Greater than
    GreaterThan(TestValue),
    /// Less than
    LessThan(TestValue),
}

#[derive(Debug, Clone)]
pub enum TestValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),
}

/// Variable binding in a successful match
#[derive(Debug, Clone)]
pub struct Binding {
    pub name: String,
    pub path: TestPath,
    pub type_hint: Option<String>,
}

/// Guard condition for pattern guards
#[derive(Debug, Clone)]
pub struct GuardCondition {
    pub expression: String, // Simplified - would be actual expression AST
    pub bindings: Vec<String>,
}

impl TestPath {
    pub fn root() -> Self {
        Self {
            components: smallvec![],
        }
    }
    
    pub fn field(mut self, name: String) -> Self {
        self.components.push(PathComponent::Field(name));
        self
    }
    
    pub fn tuple_index(mut self, index: usize) -> Self {
        self.components.push(PathComponent::TupleIndex(index));
        self
    }
    
    pub fn array_index(mut self, index: usize) -> Self {
        self.components.push(PathComponent::ArrayIndex(index));
        self
    }
    
    pub fn array_length(mut self) -> Self {
        self.components.push(PathComponent::ArrayLength);
        self
    }
    
    pub fn enum_variant(mut self, variant: String) -> Self {
        self.components.push(PathComponent::EnumVariant(variant));
        self
    }
    
    pub fn tensor_dim(mut self, dim: usize) -> Self {
        self.components.push(PathComponent::TensorDim(dim));
        self
    }
}

/// Builder for decision trees
pub struct DecisionTreeBuilder {
    patterns: Vec<PatternRow>,
    next_pattern_id: usize,
}

/// A row in the pattern matrix during compilation
#[derive(Debug, Clone)]
struct PatternRow {
    id: usize,
    patterns: Vec<CompiledPattern>,
    guard: Option<GuardCondition>,
    bindings: HashMap<String, Binding>,
    span: Span,
}

/// Internal representation of patterns during compilation
#[derive(Debug, Clone)]
pub enum CompiledPattern {
    Wildcard,
    Literal(TestValue),
    Constructor { 
        name: String, 
        arity: usize,
        subpatterns: Vec<CompiledPattern>,
    },
    Variable(String),
    Or(Vec<CompiledPattern>),
}

impl DecisionTreeBuilder {
    pub fn new() -> Self {
        Self {
            patterns: Vec::new(),
            next_pattern_id: 0,
        }
    }
    
    /// Add a pattern to be compiled
    pub fn add_pattern(
        &mut self,
        pattern: CompiledPattern,
        guard: Option<GuardCondition>,
        span: Span,
    ) -> usize {
        let id = self.next_pattern_id;
        self.next_pattern_id += 1;
        
        self.patterns.push(PatternRow {
            id,
            patterns: vec![pattern],
            guard,
            bindings: HashMap::new(),
            span,
        });
        
        id
    }
    
    /// Build the decision tree
    pub fn build(self) -> PatternResult<DecisionTree> {
        if self.patterns.is_empty() {
            return Ok(DecisionTree {
                root: DecisionNode::Failure,
                total_patterns: 0,
            });
        }
        
        let root = self.compile_patterns(
            &self.patterns,
            &TestPath::root(),
            0,
        )?;
        
        Ok(DecisionTree {
            root,
            total_patterns: self.patterns.len(),
        })
    }
    
    /// Compile pattern matrix into decision tree
    fn compile_patterns(
        &self,
        patterns: &[PatternRow],
        path: &TestPath,
        column: usize,
    ) -> PatternResult<DecisionNode> {
        // Base case: no patterns left
        if patterns.is_empty() {
            return Ok(DecisionNode::Failure);
        }
        
        // Base case: no more columns to process
        if column >= patterns[0].patterns.len() {
            // Check guards and select first matching pattern
            for pattern in patterns {
                if let Some(guard) = &pattern.guard {
                    // Create guard node
                    let then_branch = Box::new(DecisionNode::Success {
                        pattern_id: pattern.id,
                        bindings: pattern.bindings.clone(),
                    });
                    
                    let else_patterns: Vec<_> = patterns.iter()
                        .skip_while(|p| p.id != pattern.id)
                        .skip(1)
                        .cloned()
                        .collect();
                    
                    let else_branch = Box::new(self.compile_patterns(&else_patterns, path, column)?);
                    
                    return Ok(DecisionNode::Guard {
                        pattern_id: pattern.id,
                        condition: guard.clone(),
                        then_branch,
                        else_branch,
                    });
                } else {
                    // Unconditional match
                    return Ok(DecisionNode::Success {
                        pattern_id: pattern.id,
                        bindings: pattern.bindings.clone(),
                    });
                }
            }
            return Ok(DecisionNode::Failure);
        }
        
        // Group patterns by their head pattern
        let groups = self.group_patterns_by_head(patterns, column);
        
        // If all patterns are wildcards or variables, skip to next column
        if groups.len() == 1 && matches!(groups[0].0, CompiledPattern::Wildcard | CompiledPattern::Variable(_)) {
            return self.compile_patterns(patterns, path, column + 1);
        }
        
        // Create switch node
        let mut branches = Vec::new();
        let mut default_patterns = Vec::new();
        
        for (head_pattern, group_patterns) in groups {
            match head_pattern {
                CompiledPattern::Literal(value) => {
                    let condition = TestCondition::Equals(value);
                    let node = self.specialize_patterns(&group_patterns, path, column)?;
                    branches.push(SwitchBranch {
                        test: condition,
                        node,
                    });
                }
                CompiledPattern::Constructor { name, .. } => {
                    let condition = TestCondition::IsVariant(name);
                    let node = self.specialize_patterns(&group_patterns, path, column)?;
                    branches.push(SwitchBranch {
                        test: condition,
                        node,
                    });
                }
                CompiledPattern::Wildcard | CompiledPattern::Variable(_) => {
                    default_patterns.extend(group_patterns);
                }
                _ => {
                    return Err(PatternError::UnsupportedPattern {
                        pattern_type: "complex pattern in decision tree".to_string(),
                        span: patterns[0].span,
                    });
                }
            }
        }
        
        let default = if default_patterns.is_empty() {
            None
        } else {
            Some(Box::new(self.compile_patterns(&default_patterns, path, column + 1)?))
        };
        
        Ok(DecisionNode::Switch {
            test_path: path.clone(),
            branches,
            default,
        })
    }
    
    /// Group patterns by their head pattern constructor
    fn group_patterns_by_head(&self, patterns: &[PatternRow], column: usize) -> Vec<(CompiledPattern, Vec<PatternRow>)> {
        let mut groups: HashMap<String, Vec<PatternRow>> = HashMap::new();
        
        for pattern in patterns {
            if let Some(head) = pattern.patterns.get(column) {
                let key = match head {
                    CompiledPattern::Literal(TestValue::Int(n)) => format!("lit_int_{}", n),
                    CompiledPattern::Literal(TestValue::String(s)) => format!("lit_str_{}", s),
                    CompiledPattern::Literal(TestValue::Bool(b)) => format!("lit_bool_{}", b),
                    CompiledPattern::Constructor { name, .. } => format!("ctor_{}", name),
                    CompiledPattern::Wildcard => "wildcard".to_string(),
                    CompiledPattern::Variable(name) => format!("var_{}", name),
                    _ => "other".to_string(),
                };
                
                groups.entry(key).or_default().push(pattern.clone());
            }
        }
        
        // Convert back to vec with original patterns
        groups.into_iter()
            .map(|(_, group)| {
                let head = group[0].patterns.get(column).unwrap().clone();
                (head, group)
            })
            .collect()
    }
    
    /// Specialize patterns for a specific constructor
    fn specialize_patterns(&self, patterns: &[PatternRow], _path: &TestPath, column: usize) -> PatternResult<DecisionNode> {
        let mut specialized = Vec::new();
        
        for pattern in patterns {
            if let Some(head) = pattern.patterns.get(column) {
                match head {
                    CompiledPattern::Constructor { subpatterns, .. } => {
                        let mut new_pattern = pattern.clone();
                        // Replace current column with subpatterns
                        new_pattern.patterns.splice(column..=column, subpatterns.iter().cloned());
                        specialized.push(new_pattern);
                    }
                    CompiledPattern::Literal(_) => {
                        // Remove the literal pattern
                        let mut new_pattern = pattern.clone();
                        new_pattern.patterns.remove(column);
                        specialized.push(new_pattern);
                    }
                    _ => {
                        // Keep other patterns as-is
                        specialized.push(pattern.clone());
                    }
                }
            }
        }
        
        self.compile_patterns(&specialized, _path, column)
    }
}

impl Default for DecisionTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decision_tree_builder() {
        let mut builder = DecisionTreeBuilder::new();
        
        // Add simple literal pattern
        let pattern = CompiledPattern::Literal(TestValue::Int(42));
        let id = builder.add_pattern(pattern, None, Span::new(0, 0));
        assert_eq!(id, 0);
        
        // Build tree
        let tree = builder.build().unwrap();
        assert_eq!(tree.total_patterns, 1);
    }
    
    #[test]
    fn test_test_path() {
        let path = TestPath::root()
            .field("x".to_string())
            .tuple_index(0)
            .array_index(1);
        
        assert_eq!(path.components.len(), 3);
        assert!(matches!(path.components[0], PathComponent::Field(_)));
        assert!(matches!(path.components[1], PathComponent::TupleIndex(0)));
        assert!(matches!(path.components[2], PathComponent::ArrayIndex(1)));
    }
}