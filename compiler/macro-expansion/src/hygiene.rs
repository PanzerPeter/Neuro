//! Hygiene system for macro expansion
//! 
//! Ensures that macro-generated code doesn't accidentally capture variables
//! from the surrounding scope or introduce naming conflicts.

use crate::{MacroError, MacroResult};
use shared_types::Span;
use std::collections::{HashMap, HashSet};

/// Hygiene context for tracking variable scopes during macro expansion
#[derive(Debug, Clone)]
pub struct HygieneContext {
    /// Current scope depth
    scope_depth: usize,
    /// Variable definitions by scope
    scopes: Vec<ScopeInfo>,
    /// Generated unique identifiers
    unique_counter: usize,
    /// Macro expansion context
    expansion_contexts: Vec<ExpansionContext>,
}

#[derive(Debug, Clone)]
struct ScopeInfo {
    #[allow(dead_code)]
    depth: usize, // Reserved for future scope depth tracking
    variables: HashMap<String, VariableInfo>,
    /// Macro-generated variables that need unique names
    generated_vars: HashSet<String>,
}

#[derive(Debug, Clone)]
struct VariableInfo {
    name: String,
    #[allow(dead_code)]
    original_name: String, // Reserved for hygiene tracking
    is_macro_generated: bool,
    #[allow(dead_code)]
    definition_span: Span, // Reserved for error reporting
    scope_depth: usize,
}

#[derive(Debug, Clone)]
struct ExpansionContext {
    #[allow(dead_code)]
    macro_name: String, // Reserved for macro hygiene tracking
    expansion_id: usize,
    #[allow(dead_code)]
    call_site_span: Span, // Reserved for error reporting
    #[allow(dead_code)]
    variables_in_scope: HashSet<String>, // Reserved for hygiene analysis
}

impl HygieneContext {
    pub fn new() -> Self {
        Self {
            scope_depth: 0,
            scopes: vec![ScopeInfo {
                depth: 0,
                variables: HashMap::new(),
                generated_vars: HashSet::new(),
            }],
            unique_counter: 0,
            expansion_contexts: Vec::new(),
        }
    }
    
    /// Enter a new lexical scope
    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
        self.scopes.push(ScopeInfo {
            depth: self.scope_depth,
            variables: HashMap::new(),
            generated_vars: HashSet::new(),
        });
    }
    
    /// Exit the current lexical scope
    pub fn exit_scope(&mut self) {
        if self.scope_depth > 0 {
            self.scopes.pop();
            self.scope_depth -= 1;
        }
    }
    
    /// Enter macro expansion context
    pub fn enter_macro_expansion(
        &mut self,
        macro_name: String,
        call_site_span: Span,
    ) -> usize {
        let expansion_id = self.unique_counter;
        self.unique_counter += 1;
        
        // Capture current variable scope
        let variables_in_scope = self.get_current_variables();
        
        self.expansion_contexts.push(ExpansionContext {
            macro_name,
            expansion_id,
            call_site_span,
            variables_in_scope,
        });
        
        expansion_id
    }
    
    /// Exit macro expansion context
    pub fn exit_macro_expansion(&mut self) {
        self.expansion_contexts.pop();
    }
    
    /// Register a variable in the current scope
    pub fn define_variable(
        &mut self,
        name: &str,
        span: Span,
        is_macro_generated: bool,
    ) -> MacroResult<String> {
        let current_scope = self.scopes.last_mut()
            .ok_or_else(|| MacroError::HygieneViolation {
                variable: name.to_string(),
                span,
            })?;
        
        // Check for conflicts with existing variables
        if current_scope.variables.contains_key(name) {
            return Err(MacroError::HygieneViolation {
                variable: format!("Variable {} already defined in current scope", name),
                span,
            });
        }
        
        let final_name = if is_macro_generated {
            let unique_id = self.unique_counter;
            self.unique_counter += 1;
            let expansion_id = self.expansion_contexts.last()
                .map(|ctx| ctx.expansion_id)
                .unwrap_or(0);
            format!("__{}_{}_{}", name, expansion_id, unique_id)
        } else {
            name.to_string()
        };
        
        let var_info = VariableInfo {
            name: final_name.clone(),
            original_name: name.to_string(),
            is_macro_generated,
            definition_span: span,
            scope_depth: self.scope_depth,
        };
        
        current_scope.variables.insert(name.to_string(), var_info);
        
        if is_macro_generated {
            current_scope.generated_vars.insert(final_name.clone());
        }
        
        Ok(final_name)
    }
    
    /// Resolve a variable reference, applying hygiene rules
    pub fn resolve_variable(&self, name: &str, reference_span: Span) -> MacroResult<String> {
        // Search scopes from innermost to outermost
        for scope in self.scopes.iter().rev() {
            if let Some(var_info) = scope.variables.get(name) {
                // Check hygiene rules
                if var_info.is_macro_generated && !self.is_in_same_expansion_context(var_info) {
                    return Err(MacroError::HygieneViolation {
                        variable: format!(
                            "Cannot access macro-generated variable {} from different expansion context",
                            name
                        ),
                        span: reference_span,
                    });
                }
                
                return Ok(var_info.name.clone());
            }
        }
        
        // Variable not found in any scope
        Err(MacroError::HygieneViolation {
            variable: format!("Undefined variable: {}", name),
            span: reference_span,
        })
    }
    
    /// Generate a unique name for macro-generated variables
    pub fn generate_unique_name(&mut self, base_name: &str) -> String {
        let unique_id = self.unique_counter;
        self.unique_counter += 1;
        
        let expansion_id = self.expansion_contexts.last()
            .map(|ctx| ctx.expansion_id)
            .unwrap_or(0);
        
        format!("__{}_{}_{}", base_name, expansion_id, unique_id)
    }
    
    /// Check if a variable is accessible under hygiene rules
    fn is_in_same_expansion_context(&self, var_info: &VariableInfo) -> bool {
        // Simplified: macro-generated variables are accessible if generated
        // in the same or parent expansion context
        var_info.scope_depth <= self.scope_depth
    }
    
    /// Get all variables visible in current scope
    fn get_current_variables(&self) -> HashSet<String> {
        let mut variables = HashSet::new();
        
        for scope in &self.scopes {
            for var_name in scope.variables.keys() {
                variables.insert(var_name.clone());
            }
        }
        
        variables
    }
    
    /// Transform code to apply hygiene renaming
    pub fn apply_hygiene_transform(&self, code: &str) -> MacroResult<String> {
        let mut transformed = code.to_string();
        
        // Apply variable renamings (simplified implementation)
        for scope in &self.scopes {
            for (original_name, var_info) in &scope.variables {
                if var_info.is_macro_generated && original_name != &var_info.name {
                    // Replace all occurrences of the original name with the hygienic name
                    transformed = self.replace_identifier(&transformed, original_name, &var_info.name);
                }
            }
        }
        
        Ok(transformed)
    }
    
    /// Replace identifier in code (simplified - real implementation would parse)
    fn replace_identifier(&self, code: &str, old_name: &str, new_name: &str) -> String {
        // This is a simplified replacement that should be replaced with proper parsing
        let words: Vec<&str> = code.split_whitespace().collect();
        let replaced: Vec<String> = words
            .iter()
            .map(|word| {
                if *word == old_name {
                    new_name.to_string()
                } else {
                    word.to_string()
                }
            })
            .collect();
        
        replaced.join(" ")
    }
    
    /// Get hygiene statistics for debugging
    pub fn get_stats(&self) -> HygieneStats {
        let total_variables = self.scopes.iter()
            .map(|scope| scope.variables.len())
            .sum();
        
        let generated_variables = self.scopes.iter()
            .map(|scope| scope.generated_vars.len())
            .sum();
        
        HygieneStats {
            scope_depth: self.scope_depth,
            total_variables,
            generated_variables,
            expansion_contexts: self.expansion_contexts.len(),
        }
    }
}

impl Default for HygieneContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about hygiene context
#[derive(Debug, Clone)]
pub struct HygieneStats {
    pub scope_depth: usize,
    pub total_variables: usize,
    pub generated_variables: usize,
    pub expansion_contexts: usize,
}

/// Hygiene checker for validating macro expansions
pub struct HygieneChecker {
    context: HygieneContext,
    violations: Vec<HygieneViolation>,
}

#[derive(Debug, Clone)]
pub struct HygieneViolation {
    pub message: String,
    pub variable_name: String,
    pub span: Span,
    pub violation_type: ViolationType,
}

#[derive(Debug, Clone)]
pub enum ViolationType {
    /// Variable captured from outer scope
    VariableCapture,
    /// Naming conflict
    NamingConflict,
    /// Undefined variable reference
    UndefinedReference,
    /// Macro-generated variable accessed incorrectly
    ImproperAccess,
}

impl HygieneChecker {
    pub fn new() -> Self {
        Self {
            context: HygieneContext::new(),
            violations: Vec::new(),
        }
    }
    
    /// Check code for hygiene violations
    pub fn check_code(&mut self, code: &str, macro_name: &str) -> MacroResult<()> {
        self.context.enter_macro_expansion(macro_name.to_string(), Span::new(0, 0));
        
        // Simplified analysis - would normally parse the code properly
        self.analyze_simple_code(code)?;
        
        self.context.exit_macro_expansion();
        
        if self.violations.is_empty() {
            Ok(())
        } else {
            Err(MacroError::HygieneViolation {
                variable: format!("Found {} hygiene violations", self.violations.len()),
                span: Span::new(0, 0),
            })
        }
    }
    
    /// Simple code analysis for hygiene checking
    fn analyze_simple_code(&mut self, code: &str) -> MacroResult<()> {
        // This is a simplified implementation
        // Real implementation would use proper parsing
        
        let lines = code.lines();
        for (line_num, line) in lines.enumerate() {
            if line.trim().starts_with("let ") || line.trim().starts_with("var ") {
                // Variable declaration
                if let Some(var_name) = self.extract_variable_name(line) {
                    let span = Span::new(line_num, line_num);
                    let _ = self.context.define_variable(&var_name, span, true);
                }
            }
        }
        
        Ok(())
    }
    
    /// Extract variable name from declaration (simplified)
    fn extract_variable_name(&self, line: &str) -> Option<String> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && (parts[0] == "let" || parts[0] == "var") {
            Some(parts[1].trim_end_matches(':').to_string())
        } else {
            None
        }
    }
    
    /// Get all violations found during checking
    pub fn get_violations(&self) -> &[HygieneViolation] {
        &self.violations
    }
    
    /// Clear violations for next check
    pub fn clear_violations(&mut self) {
        self.violations.clear();
    }
}

impl Default for HygieneChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hygiene_context_creation() {
        let context = HygieneContext::new();
        assert_eq!(context.scope_depth, 0);
        assert_eq!(context.scopes.len(), 1);
    }
    
    #[test]
    fn test_scope_management() {
        let mut context = HygieneContext::new();
        
        context.enter_scope();
        assert_eq!(context.scope_depth, 1);
        assert_eq!(context.scopes.len(), 2);
        
        context.exit_scope();
        assert_eq!(context.scope_depth, 0);
        assert_eq!(context.scopes.len(), 1);
    }
    
    #[test]
    fn test_variable_definition() {
        let mut context = HygieneContext::new();
        
        let result = context.define_variable("test_var", Span::new(0, 0), false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_var");
    }
    
    #[test]
    fn test_macro_generated_variable() {
        let mut context = HygieneContext::new();
        
        let expansion_id = context.enter_macro_expansion("test_macro".to_string(), Span::new(0, 0));
        let result = context.define_variable("temp", Span::new(0, 0), true);
        
        assert!(result.is_ok());
        let unique_name = result.unwrap();
        assert!(unique_name.starts_with("__temp_"));
        assert!(unique_name.contains(&expansion_id.to_string()));
        
        context.exit_macro_expansion();
    }
    
    #[test]
    fn test_variable_resolution() {
        let mut context = HygieneContext::new();
        
        let _ = context.define_variable("test_var", Span::new(0, 0), false);
        let result = context.resolve_variable("test_var", Span::new(1, 1));
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_var");
    }
    
    #[test]
    fn test_unique_name_generation() {
        let mut context = HygieneContext::new();
        
        let name1 = context.generate_unique_name("temp");
        let name2 = context.generate_unique_name("temp");
        
        assert_ne!(name1, name2);
        assert!(name1.starts_with("__temp_"));
        assert!(name2.starts_with("__temp_"));
    }
    
    #[test]
    fn test_hygiene_checker() {
        let mut checker = HygieneChecker::new();
        
        let code = "let x: i32 = 42;\nlet y: f32 = 3.14;";
        let result = checker.check_code(code, "test_macro");
        
        assert!(result.is_ok());
    }
}