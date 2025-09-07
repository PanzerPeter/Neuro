//! Core macro expansion functionality

use crate::{MacroInvocation, MacroResult, MacroContext, MacroConfig, MacroError};
use shared_types::Span;
use std::collections::HashSet;

/// Core macro expander implementation
pub struct MacroExpander {
    config: MacroConfig,
    expansion_stack: Vec<String>,
}

impl MacroExpander {
    pub fn new(config: MacroConfig) -> Self {
        Self {
            config,
            expansion_stack: Vec::new(),
        }
    }
    
    /// Expand macro invocations in source code
    pub fn expand_in_source(&mut self, source: &str, context: &mut MacroContext) -> MacroResult<String> {
        let mut result = source.to_string();
        let mut changed = true;
        let mut iteration = 0;
        
        while changed && iteration < self.config.max_expansions {
            changed = false;
            let invocations = self.find_macro_invocations(&result)?;
            
            for invocation in invocations.iter().rev() {
                if self.should_expand_macro(&invocation.name)? {
                    let expanded = self.expand_single_macro(invocation, context)?;
                    result = self.replace_macro_invocation(&result, invocation, &expanded);
                    changed = true;
                }
            }
            
            iteration += 1;
        }
        
        if iteration >= self.config.max_expansions {
            return Err(MacroError::ExpansionLimitExceeded {
                limit: self.config.max_expansions,
                span: Span::new(0, 0),
            });
        }
        
        Ok(result)
    }
    
    /// Find all macro invocations in source
    fn find_macro_invocations(&self, source: &str) -> MacroResult<Vec<MacroInvocation>> {
        let mut invocations = Vec::new();
        
        // Simplified macro detection - would use proper parsing in real implementation
        for (line_num, line) in source.lines().enumerate() {
            if let Some(invocation) = self.parse_macro_invocation(line, line_num)? {
                invocations.push(invocation);
            }
        }
        
        Ok(invocations)
    }
    
    /// Parse a single macro invocation from a line
    fn parse_macro_invocation(&self, line: &str, line_num: usize) -> MacroResult<Option<MacroInvocation>> {
        let trimmed = line.trim();
        
        // Look for function-like macros: macro_name!(args)
        if let Some(end_pos) = trimmed.find('!') {
            if let Some(paren_pos) = trimmed[end_pos..].find('(') {
                let macro_name = trimmed[..end_pos].trim().to_string();
                let paren_start = end_pos + paren_pos;
                
                if let Some(closing_paren) = trimmed[paren_start..].rfind(')') {
                    let args_str = &trimmed[paren_start + 1..paren_start + closing_paren];
                    let arguments = self.parse_macro_arguments(args_str, line_num)?;
                    
                    return Ok(Some(MacroInvocation {
                        name: macro_name,
                        arguments,
                        span: Span::new(line_num, line_num),
                    }));
                }
            }
        }
        
        Ok(None)
    }
    
    /// Parse macro arguments
    fn parse_macro_arguments(&self, args_str: &str, line_num: usize) -> MacroResult<Vec<crate::MacroArgument>> {
        if args_str.trim().is_empty() {
            return Ok(Vec::new());
        }
        
        // Simple comma-separated parsing
        let args: Vec<_> = args_str.split(',')
            .map(|arg| crate::MacroArgument {
                value: arg.trim().to_string(),
                arg_type: crate::MacroArgumentType::Expression,
                span: Span::new(line_num, line_num),
            })
            .collect();
        
        Ok(args)
    }
    
    /// Check if a macro should be expanded (cycle detection)
    fn should_expand_macro(&mut self, macro_name: &str) -> MacroResult<bool> {
        if self.expansion_stack.contains(&macro_name.to_string()) {
            if self.config.allow_recursion {
                if self.expansion_stack.len() >= self.config.max_recursion_depth {
                    return Err(MacroError::RecursiveExpansion {
                        name: macro_name.to_string(),
                        span: Span::new(0, 0),
                    });
                }
            } else {
                return Err(MacroError::RecursiveExpansion {
                    name: macro_name.to_string(),
                    span: Span::new(0, 0),
                });
            }
        }
        
        Ok(true)
    }
    
    /// Expand a single macro invocation
    fn expand_single_macro(&mut self, invocation: &MacroInvocation, context: &mut MacroContext) -> MacroResult<String> {
        self.expansion_stack.push(invocation.name.clone());
        
        let result = {
            let mut engine = crate::MacroExpansionEngine::new(self.config.clone());
            *engine.context_mut() = context.clone();
            engine.expand_macro(invocation)
        };
        
        self.expansion_stack.pop();
        result
    }
    
    /// Replace macro invocation with expanded code
    fn replace_macro_invocation(&self, source: &str, invocation: &MacroInvocation, expanded: &str) -> String {
        // Simplified replacement - would be more sophisticated in real implementation
        let pattern = format!("{}!(", invocation.name);
        if let Some(start) = source.find(&pattern) {
            if let Some(end) = source[start..].find(')') {
                let before = &source[..start];
                let after = &source[start + end + 1..];
                return format!("{}{}{}", before, expanded, after);
            }
        }
        
        source.to_string()
    }
}

/// Macro expansion statistics
#[derive(Debug, Clone)]
pub struct ExpansionStats {
    pub total_expansions: usize,
    pub unique_macros: HashSet<String>,
    pub max_depth: usize,
    pub iterations: usize,
}

impl Default for MacroExpander {
    fn default() -> Self {
        Self::new(MacroConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_macro_expander_creation() {
        let expander = MacroExpander::new(MacroConfig::default());
        assert!(expander.expansion_stack.is_empty());
    }
    
    #[test]
    fn test_macro_invocation_parsing() {
        let expander = MacroExpander::default();
        
        let invocation = expander.parse_macro_invocation("debug!(x, y)", 0).unwrap();
        assert!(invocation.is_some());
        
        let inv = invocation.unwrap();
        assert_eq!(inv.name, "debug");
        assert_eq!(inv.arguments.len(), 2);
        assert_eq!(inv.arguments[0].value, "x");
        assert_eq!(inv.arguments[1].value, "y");
    }
    
    #[test]
    fn test_empty_macro_arguments() {
        let expander = MacroExpander::default();
        
        let invocation = expander.parse_macro_invocation("test!()", 0).unwrap();
        assert!(invocation.is_some());
        
        let inv = invocation.unwrap();
        assert_eq!(inv.name, "test");
        assert_eq!(inv.arguments.len(), 0);
    }
}