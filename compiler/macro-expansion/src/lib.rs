//! Macro and template preprocessing for NEURO language
//! 
//! This module provides macro expansion, template processing, and code generation
//! capabilities for the NEURO programming language.

pub mod macro_expander;
pub mod template_engine;
pub mod procedural;
pub mod attribute_macros;
pub mod hygiene;

use shared_types::Span;
use thiserror::Error;
use std::collections::HashMap;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum MacroError {
    #[error("Macro not found: {name} at {span:?}")]
    MacroNotFound { name: String, span: Span },
    
    #[error("Invalid macro syntax at {span:?}: {message}")]
    InvalidSyntax { message: String, span: Span },
    
    #[error("Macro expansion failed: {message} at {span:?}")]
    ExpansionFailed { message: String, span: Span },
    
    #[error("Template error: {message} at {span:?}")]
    TemplateError { message: String, span: Span },
    
    #[error("Recursive macro expansion detected: {name} at {span:?}")]
    RecursiveExpansion { name: String, span: Span },
    
    #[error("Too many macro expansions (limit: {limit}) at {span:?}")]
    ExpansionLimitExceeded { limit: usize, span: Span },
    
    #[error("Hygiene violation: {variable} at {span:?}")]
    HygieneViolation { variable: String, span: Span },
    
    #[error("Unsupported macro feature: {feature} at {span:?}")]
    UnsupportedFeature { feature: String, span: Span },
}

pub type MacroResult<T> = Result<T, MacroError>;

/// Configuration for macro expansion
#[derive(Debug, Clone)]
pub struct MacroConfig {
    /// Maximum number of macro expansions allowed
    pub max_expansions: usize,
    /// Enable hygiene checking
    pub enable_hygiene: bool,
    /// Allow recursive macros (with cycle detection)
    pub allow_recursion: bool,
    /// Maximum recursion depth
    pub max_recursion_depth: usize,
    /// Enable debug output
    pub debug_expansion: bool,
}

impl Default for MacroConfig {
    fn default() -> Self {
        Self {
            max_expansions: 1000,
            enable_hygiene: true,
            allow_recursion: true,
            max_recursion_depth: 100,
            debug_expansion: false,
        }
    }
}

/// Macro definition
#[derive(Debug, Clone)]
pub struct MacroDefinition {
    pub name: String,
    pub parameters: Vec<MacroParameter>,
    pub body: MacroBody,
    pub kind: MacroKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MacroParameter {
    pub name: String,
    pub param_type: MacroParameterType,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone)]
pub enum MacroParameterType {
    /// Token parameter (single token)
    Token,
    /// Expression parameter 
    Expression,
    /// Statement parameter
    Statement,
    /// Type parameter
    Type,
    /// Identifier parameter
    Identifier,
    /// Literal parameter
    Literal,
    /// Block parameter
    Block,
    /// Variadic parameter (accepts multiple arguments)
    Variadic(Box<MacroParameterType>),
}

#[derive(Debug, Clone)]
pub enum MacroBody {
    /// Template-based macro body
    Template(String),
    /// Token stream macro body
    TokenStream(String),
    /// Procedural macro (Rust-like)
    Procedural(String),
    /// Built-in macro
    BuiltIn(BuiltInMacro),
}

#[derive(Debug, Clone)]
pub enum MacroKind {
    /// Function-like macro: `macro!(args)`
    FunctionLike,
    /// Attribute macro: `#[macro_name]` or `#[macro_name(args)]`
    Attribute,
    /// Derive macro: `#[derive(MacroName)]`
    Derive,
    /// Template macro: `template<T> macro_name { ... }`
    Template,
}

#[derive(Debug, Clone)]
pub enum BuiltInMacro {
    /// Print debug information
    Debug,
    /// Compile-time assertions
    CompileAssert,
    /// Include file content
    Include,
    /// Generate tensor operations
    TensorOps,
    /// Neural network layer generation
    NeuralLayer,
    /// Automatic differentiation
    AutoDiff,
}

/// Macro expansion context
#[derive(Debug, Clone)]
pub struct MacroContext {
    /// Registered macros
    pub macros: HashMap<String, MacroDefinition>,
    /// Current expansion depth
    pub expansion_depth: usize,
    /// Expansion count
    pub expansion_count: usize,
    /// Hygiene context for variable scoping
    pub hygiene_ctx: hygiene::HygieneContext,
    /// Template variables
    pub template_vars: HashMap<String, TemplateValue>,
}

#[derive(Debug, Clone)]
pub enum TemplateValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<TemplateValue>),
    Object(HashMap<String, TemplateValue>),
}

impl MacroContext {
    pub fn new() -> Self {
        Self {
            macros: HashMap::new(),
            expansion_depth: 0,
            expansion_count: 0,
            hygiene_ctx: hygiene::HygieneContext::new(),
            template_vars: HashMap::new(),
        }
    }
    
    /// Register a macro definition
    pub fn register_macro(&mut self, macro_def: MacroDefinition) -> MacroResult<()> {
        if self.macros.contains_key(&macro_def.name) {
            return Err(MacroError::InvalidSyntax {
                message: format!("Macro {} is already defined", macro_def.name),
                span: macro_def.span,
            });
        }
        
        self.macros.insert(macro_def.name.clone(), macro_def);
        Ok(())
    }
    
    /// Get a macro definition
    pub fn get_macro(&self, name: &str) -> Option<&MacroDefinition> {
        self.macros.get(name)
    }
    
    /// Set template variable
    pub fn set_template_var(&mut self, name: String, value: TemplateValue) {
        self.template_vars.insert(name, value);
    }
    
    /// Get template variable
    pub fn get_template_var(&self, name: &str) -> Option<&TemplateValue> {
        self.template_vars.get(name)
    }
    
    /// Enter expansion scope
    pub fn enter_expansion(&mut self, config: &MacroConfig) -> MacroResult<()> {
        if self.expansion_count >= config.max_expansions {
            return Err(MacroError::ExpansionLimitExceeded {
                limit: config.max_expansions,
                span: Span::new(0, 0), // Would be passed from caller
            });
        }
        
        self.expansion_depth += 1;
        self.expansion_count += 1;
        
        if self.expansion_depth > config.max_recursion_depth {
            return Err(MacroError::RecursiveExpansion {
                name: "unknown".to_string(), // Would be passed from caller
                span: Span::new(0, 0),
            });
        }
        
        Ok(())
    }
    
    /// Exit expansion scope
    pub fn exit_expansion(&mut self) {
        if self.expansion_depth > 0 {
            self.expansion_depth -= 1;
        }
    }
}

impl Default for MacroContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro invocation
#[derive(Debug, Clone)]
pub struct MacroInvocation {
    pub name: String,
    pub arguments: Vec<MacroArgument>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct MacroArgument {
    pub value: String,
    pub arg_type: MacroArgumentType,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum MacroArgumentType {
    Token,
    Expression,
    Statement,
    Type,
    Block,
}

/// Main macro expansion interface
pub struct MacroExpansionEngine {
    config: MacroConfig,
    context: MacroContext,
}

impl MacroExpansionEngine {
    pub fn new(config: MacroConfig) -> Self {
        let mut context = MacroContext::new();
        
        // Register built-in macros
        Self::register_builtin_macros(&mut context);
        
        Self { config, context }
    }
    
    pub fn with_default_config() -> Self {
        Self::new(MacroConfig::default())
    }
    
    /// Register built-in macros
    fn register_builtin_macros(context: &mut MacroContext) {
        let builtin_macros = vec![
            ("debug", BuiltInMacro::Debug),
            ("compile_assert", BuiltInMacro::CompileAssert),
            ("include", BuiltInMacro::Include),
            ("tensor_ops", BuiltInMacro::TensorOps),
            ("neural_layer", BuiltInMacro::NeuralLayer),
            ("auto_diff", BuiltInMacro::AutoDiff),
        ];
        
        for (name, builtin) in builtin_macros {
            let macro_def = MacroDefinition {
                name: name.to_string(),
                parameters: Vec::new(), // Built-ins have flexible parameters
                body: MacroBody::BuiltIn(builtin),
                kind: MacroKind::FunctionLike,
                span: Span::new(0, 0),
            };
            
            let _ = context.register_macro(macro_def);
        }
    }
    
    /// Expand a macro invocation
    pub fn expand_macro(&mut self, invocation: &MacroInvocation) -> MacroResult<String> {
        self.context.enter_expansion(&self.config)?;
        
        let result = if let Some(macro_def) = self.context.get_macro(&invocation.name).cloned() {
            self.expand_macro_definition(&macro_def, invocation)
        } else {
            Err(MacroError::MacroNotFound {
                name: invocation.name.clone(),
                span: invocation.span,
            })
        };
        
        self.context.exit_expansion();
        result
    }
    
    /// Expand a specific macro definition
    fn expand_macro_definition(
        &mut self,
        macro_def: &MacroDefinition,
        invocation: &MacroInvocation,
    ) -> MacroResult<String> {
        // Validate arguments
        self.validate_arguments(macro_def, invocation)?;
        
        // Bind parameters
        let bindings = self.bind_parameters(macro_def, invocation)?;
        
        // Expand body
        match &macro_def.body {
            MacroBody::Template(template) => {
                self.expand_template(template, &bindings)
            }
            MacroBody::TokenStream(tokens) => {
                self.expand_token_stream(tokens, &bindings)
            }
            MacroBody::Procedural(code) => {
                self.expand_procedural(code, &bindings)
            }
            MacroBody::BuiltIn(builtin) => {
                self.expand_builtin(builtin, invocation)
            }
        }
    }
    
    /// Validate macro arguments against parameters
    fn validate_arguments(
        &self,
        macro_def: &MacroDefinition,
        invocation: &MacroInvocation,
    ) -> MacroResult<()> {
        // Basic arity check (simplified)
        let required_params = macro_def.parameters.iter()
            .filter(|p| p.default_value.is_none())
            .count();
        
        if invocation.arguments.len() < required_params {
            return Err(MacroError::InvalidSyntax {
                message: format!(
                    "Macro {} requires {} arguments, got {}",
                    macro_def.name,
                    required_params,
                    invocation.arguments.len()
                ),
                span: invocation.span,
            });
        }
        
        Ok(())
    }
    
    /// Bind macro parameters to arguments
    fn bind_parameters(
        &self,
        macro_def: &MacroDefinition,
        invocation: &MacroInvocation,
    ) -> MacroResult<HashMap<String, String>> {
        let mut bindings = HashMap::new();
        
        for (i, param) in macro_def.parameters.iter().enumerate() {
            let value = if i < invocation.arguments.len() {
                invocation.arguments[i].value.clone()
            } else if let Some(default) = &param.default_value {
                default.clone()
            } else {
                return Err(MacroError::InvalidSyntax {
                    message: format!("Missing argument for parameter {}", param.name),
                    span: invocation.span,
                });
            };
            
            bindings.insert(param.name.clone(), value);
        }
        
        Ok(bindings)
    }
    
    /// Expand template-based macro
    fn expand_template(
        &self,
        template: &str,
        bindings: &HashMap<String, String>,
    ) -> MacroResult<String> {
        template_engine::expand_template(template, bindings)
            .map_err(|e| MacroError::TemplateError {
                message: e.to_string(),
                span: Span::new(0, 0),
            })
    }
    
    /// Expand token stream macro
    fn expand_token_stream(
        &self,
        tokens: &str,
        bindings: &HashMap<String, String>,
    ) -> MacroResult<String> {
        let mut result = tokens.to_string();
        
        // Simple substitution (real implementation would parse tokens)
        for (param, value) in bindings {
            let pattern = format!("${}", param);
            result = result.replace(&pattern, value);
        }
        
        Ok(result)
    }
    
    /// Expand procedural macro
    fn expand_procedural(
        &self,
        _code: &str,
        _bindings: &HashMap<String, String>,
    ) -> MacroResult<String> {
        // Placeholder - would execute Rust-like procedural macro
        Err(MacroError::UnsupportedFeature {
            feature: "procedural macros".to_string(),
            span: Span::new(0, 0),
        })
    }
    
    /// Expand built-in macro
    fn expand_builtin(
        &self,
        builtin: &BuiltInMacro,
        invocation: &MacroInvocation,
    ) -> MacroResult<String> {
        match builtin {
            BuiltInMacro::Debug => {
                if let Some(arg) = invocation.arguments.first() {
                    Ok(format!("println!(\"Debug: {{}}\", {});", arg.value))
                } else {
                    Ok("println!(\"Debug: no args\");".to_string())
                }
            }
            BuiltInMacro::CompileAssert => {
                if let Some(condition) = invocation.arguments.first() {
                    Ok(format!("static_assert!({});", condition.value))
                } else {
                    Err(MacroError::InvalidSyntax {
                        message: "compile_assert requires a condition".to_string(),
                        span: invocation.span,
                    })
                }
            }
            BuiltInMacro::Include => {
                if let Some(file_arg) = invocation.arguments.first() {
                    // Simplified - would actually read and include file
                    Ok(format!("// Include: {}", file_arg.value))
                } else {
                    Err(MacroError::InvalidSyntax {
                        message: "include requires a filename".to_string(),
                        span: invocation.span,
                    })
                }
            }
            _ => Err(MacroError::UnsupportedFeature {
                feature: format!("built-in macro {:?}", builtin),
                span: invocation.span,
            }),
        }
    }
    
    /// Get current context
    pub fn context(&self) -> &MacroContext {
        &self.context
    }
    
    /// Get mutable context
    pub fn context_mut(&mut self) -> &mut MacroContext {
        &mut self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_macro_context_creation() {
        let context = MacroContext::new();
        assert_eq!(context.expansion_depth, 0);
        assert_eq!(context.expansion_count, 0);
        assert!(context.macros.is_empty());
    }
    
    #[test]
    fn test_macro_registration() {
        let mut context = MacroContext::new();
        
        let macro_def = MacroDefinition {
            name: "test_macro".to_string(),
            parameters: Vec::new(),
            body: MacroBody::Template("Hello, world!".to_string()),
            kind: MacroKind::FunctionLike,
            span: Span::new(0, 0),
        };
        
        assert!(context.register_macro(macro_def).is_ok());
        assert!(context.get_macro("test_macro").is_some());
    }
    
    #[test]
    fn test_expansion_engine_creation() {
        let engine = MacroExpansionEngine::with_default_config();
        assert_eq!(engine.config.max_expansions, 1000);
        assert!(engine.config.enable_hygiene);
    }
    
    #[test]
    fn test_builtin_debug_macro() {
        let mut engine = MacroExpansionEngine::with_default_config();
        
        let invocation = MacroInvocation {
            name: "debug".to_string(),
            arguments: vec![MacroArgument {
                value: "x".to_string(),
                arg_type: MacroArgumentType::Expression,
                span: Span::new(0, 0),
            }],
            span: Span::new(0, 0),
        };
        
        let result = engine.expand_macro(&invocation).unwrap();
        assert!(result.contains("println!"));
        assert!(result.contains("x"));
    }
}