//! Function compilation to LLVM IR
//! 
//! This module handles the compilation of NEURO functions to LLVM IR,
//! including parameter handling, local variables, and code generation.

use shared_types::{
    ast::{Function, Statement, Expression, BinaryOperator, UnaryOperator, Literal},
    Type, Span
};
use crate::LLVMError;
use std::collections::HashMap;

/// Text-based builder for compiling NEURO functions to LLVM IR
pub struct TextBasedFunctionBuilder {
    /// Current variable counter for generating unique SSA names
    var_counter: u32,
    
    /// Symbol table for local variables
    local_variables: HashMap<String, (String, Type)>, // (llvm_name, type)
    
    /// Function parameters
    parameters: HashMap<String, (String, Type)>, // (llvm_name, type)
    
    /// Generated IR lines
    ir_lines: Vec<String>,
}

impl TextBasedFunctionBuilder {
    /// Create a new text-based function builder
    pub fn new() -> Self {
        Self {
            var_counter: 0,
            local_variables: HashMap::new(),
            parameters: HashMap::new(),
            ir_lines: Vec::new(),
        }
    }
    
    /// Generate next SSA variable name
    fn next_var_name(&mut self) -> String {
        let name = format!("%{}", self.var_counter);
        self.var_counter += 1;
        name
    }
    
    /// Build a NEURO function into LLVM IR text
    pub fn build_function(&mut self, function: &Function) -> Result<String, LLVMError> {
        tracing::debug!("Compiling function: {}", function.name);
        
        self.ir_lines.clear();
        self.local_variables.clear();
        self.parameters.clear();
        self.var_counter = 0;
        
        // Generate function signature
        let return_type = self.map_optional_type_to_llvm(&function.return_type);
        let mut param_types = Vec::new();
        
        for (i, param) in function.parameters.iter().enumerate() {
            let llvm_type = self.map_type_to_llvm(&param.param_type);
            param_types.push(llvm_type);
            
            // Store parameter info
            let param_name = format!("%param_{}", i);
            self.parameters.insert(param.name.clone(), (param_name, param.param_type.clone()));
        }
        
        // Function declaration
        let param_list = param_types.join(", ");
        self.ir_lines.push(format!("define {} @{}({}) {{", return_type, function.name, param_list));
        
        // Entry block
        self.ir_lines.push("entry:".to_string());
        
        // Allocate space for parameters (promote to memory)
        for (i, param) in function.parameters.iter().enumerate() {
            let alloca_name = format!("%{}_addr", param.name);
            let param_name = format!("%param_{}", i);
            let llvm_type = self.map_type_to_llvm(&param.param_type);
            
            self.ir_lines.push(format!("  {} = alloca {}", alloca_name, llvm_type));
            self.ir_lines.push(format!("  store {} {}, {} {}", llvm_type, param_name, llvm_type, alloca_name));
            
            // Update parameter mapping to point to alloca
            self.parameters.insert(param.name.clone(), (alloca_name, param.param_type.clone()));
        }
        
        // Compile function body
        for statement in &function.body.statements {
            self.compile_statement(statement)?;
        }
        
        // Add default return if needed
        self.ensure_function_return(&function.return_type)?;
        
        self.ir_lines.push("}".to_string());
        
        Ok(self.ir_lines.join("\n"))
    }
    
    /// Map NEURO type to LLVM type string
    fn map_type_to_llvm(&self, neuro_type: &Type) -> String {
        match neuro_type {
            Type::Int => "i32".to_string(),
            Type::Float => "float".to_string(),
            Type::Bool => "i1".to_string(),
            Type::String => "i8*".to_string(),
            Type::Unknown => "i32".to_string(), // Default to i32
            Type::Tensor { element_type, .. } => {
                format!("{}*", self.map_type_to_llvm(element_type)) // Tensor as pointer
            },
            Type::Function { .. } => "i8*".to_string(), // Function pointer
            Type::Generic(_) => "i32".to_string(), // Default for generics
        }
    }
    
    /// Map optional NEURO type to LLVM type string (for return types)
    fn map_optional_type_to_llvm(&self, neuro_type: &Option<Type>) -> String {
        match neuro_type {
            Some(t) => self.map_type_to_llvm(t),
            None => "void".to_string(),
        }
    }
    
    /// Compile a statement
    fn compile_statement(&mut self, statement: &Statement) -> Result<(), LLVMError> {
        match statement {
            Statement::Let(let_stmt) => {
                let var_type = let_stmt.var_type.as_ref().unwrap_or(&Type::Unknown);
                let llvm_type = self.map_type_to_llvm(var_type);
                let alloca_name = format!("%{}_addr", let_stmt.name);
                
                // Create alloca
                self.ir_lines.push(format!("  {} = alloca {}", alloca_name, llvm_type));
                
                // Initialize if there's an initializer
                if let Some(init) = &let_stmt.initializer {
                    let init_value = self.compile_expression(init)?;
                    self.ir_lines.push(format!("  store {} {}, {} {}", llvm_type, init_value, llvm_type, alloca_name));
                }
                
                // Store variable info
                self.local_variables.insert(let_stmt.name.clone(), (alloca_name, var_type.clone()));
            },
            
            Statement::Return(ret_stmt) => {
                if let Some(expr) = &ret_stmt.value {
                    let return_value = self.compile_expression(expr)?;
                    let return_type = self.infer_expression_type(expr);
                    let llvm_type = self.map_type_to_llvm(&return_type);
                    self.ir_lines.push(format!("  ret {} {}", llvm_type, return_value));
                } else {
                    self.ir_lines.push("  ret void".to_string());
                }
            },
            
            Statement::Expression(expr) => {
                // Just compile the expression and discard the result
                self.compile_expression(expr)?;
            },
            
            _ => {
                return Err(LLVMError::CodeGeneration {
                    message: "Statement type not yet implemented".to_string(),
                    span: Span::new(0, 0),
                });
            }
        }
        
        Ok(())
    }
    
    /// Compile an expression and return the SSA variable name
    fn compile_expression(&mut self, expression: &Expression) -> Result<String, LLVMError> {
        match expression {
            Expression::Literal(lit) => self.compile_literal(lit),
            Expression::Identifier(ident) => self.compile_identifier(&ident.name),
            Expression::Binary(bin_expr) => self.compile_binary_expression(bin_expr),
            Expression::Unary(unary_expr) => self.compile_unary_expression(unary_expr),
            Expression::Call(call_expr) => self.compile_function_call(call_expr),
            
            _ => Err(LLVMError::CodeGeneration {
                message: "Expression type not yet implemented".to_string(),
                span: Span::new(0, 0),
            }),
        }
    }
    
    /// Compile a literal value
    fn compile_literal(&mut self, literal: &Literal) -> Result<String, LLVMError> {
        match literal {
            Literal::Integer(i, _span) => Ok(i.to_string()),
            Literal::Float(f, _span) => Ok(f.to_string()),
            Literal::Boolean(b, _span) => Ok(if *b { "1" } else { "0" }.to_string()),
            Literal::String(s, _span) => {
                // For strings, we need to create a global constant
                let global_name = format!("@.str.{}", self.var_counter);
                self.var_counter += 1;
                
                // This is a simplification - in real LLVM IR, strings are more complex
                let str_len = s.len() + 1;
                Ok(format!("getelementptr inbounds ([{} x i8], [{} x i8]* {}, i64 0, i64 0)", 
                    str_len, str_len, global_name))
            },
        }
    }
    
    /// Compile an identifier (variable reference)
    fn compile_identifier(&mut self, name: &str) -> Result<String, LLVMError> {
        // Check local variables first
        if let Some((alloca_name, var_type)) = self.local_variables.get(name).cloned() {
            let load_name = self.next_var_name();
            let llvm_type = self.map_type_to_llvm(&var_type);
            self.ir_lines.push(format!("  {} = load {}, {} {}", load_name, llvm_type, llvm_type, alloca_name));
            Ok(load_name)
        }
        // Check parameters
        else if let Some((param_name, var_type)) = self.parameters.get(name).cloned() {
            let load_name = self.next_var_name();
            let llvm_type = self.map_type_to_llvm(&var_type);
            self.ir_lines.push(format!("  {} = load {}, {} {}", load_name, llvm_type, llvm_type, param_name));
            Ok(load_name)
        }
        // Variable not found
        else {
            Err(LLVMError::CodeGeneration {
                message: format!("Undefined variable '{}'", name),
                span: Span::new(0, 0),
            })
        }
    }
    
    /// Compile a binary expression
    fn compile_binary_expression(&mut self, bin_expr: &shared_types::ast::BinaryExpression) -> Result<String, LLVMError> {
        let left = self.compile_expression(&bin_expr.left)?;
        let right = self.compile_expression(&bin_expr.right)?;
        let result_name = self.next_var_name();
        
        let instruction = match bin_expr.operator {
            BinaryOperator::Add => format!("  {} = add i32 {}, {}", result_name, left, right),
            BinaryOperator::Subtract => format!("  {} = sub i32 {}, {}", result_name, left, right),
            BinaryOperator::Multiply => format!("  {} = mul i32 {}, {}", result_name, left, right),
            BinaryOperator::Divide => format!("  {} = sdiv i32 {}, {}", result_name, left, right),
            BinaryOperator::Equal => format!("  {} = icmp eq i32 {}, {}", result_name, left, right),
            BinaryOperator::NotEqual => format!("  {} = icmp ne i32 {}, {}", result_name, left, right),
            BinaryOperator::Less => format!("  {} = icmp slt i32 {}, {}", result_name, left, right),
            BinaryOperator::LessEqual => format!("  {} = icmp sle i32 {}, {}", result_name, left, right),
            BinaryOperator::Greater => format!("  {} = icmp sgt i32 {}, {}", result_name, left, right),
            BinaryOperator::GreaterEqual => format!("  {} = icmp sge i32 {}, {}", result_name, left, right),
            _ => {
                return Err(LLVMError::CodeGeneration {
                    message: format!("Binary operator {:?} not yet implemented", bin_expr.operator),
                    span: bin_expr.span,
                });
            }
        };
        
        self.ir_lines.push(instruction);
        Ok(result_name)
    }
    
    /// Compile a unary expression
    fn compile_unary_expression(&mut self, unary_expr: &shared_types::ast::UnaryExpression) -> Result<String, LLVMError> {
        let operand = self.compile_expression(&unary_expr.operand)?;
        let result_name = self.next_var_name();
        
        let instruction = match unary_expr.operator {
            UnaryOperator::Negate => format!("  {} = sub i32 0, {}", result_name, operand),
            _ => {
                return Err(LLVMError::CodeGeneration {
                    message: format!("Unary operator {:?} not yet implemented", unary_expr.operator),
                    span: unary_expr.span,
                });
            }
        };
        
        self.ir_lines.push(instruction);
        Ok(result_name)
    }
    
    /// Compile a function call
    fn compile_function_call(&mut self, call_expr: &shared_types::ast::CallExpression) -> Result<String, LLVMError> {
        let mut args = Vec::new();
        
        // Compile arguments
        for arg in &call_expr.arguments {
            let arg_value = self.compile_expression(arg)?;
            let arg_type = self.infer_expression_type(arg);
            let llvm_type = self.map_type_to_llvm(&arg_type);
            args.push(format!("{} {}", llvm_type, arg_value));
        }
        
        let result_name = self.next_var_name();
        let args_str = args.join(", ");
        
        // Extract function name from the function expression
        let function_name = match &*call_expr.function {
            Expression::Identifier(ident) => &ident.name,
            _ => {
                return Err(LLVMError::CodeGeneration {
                    message: "Only simple function calls supported for now".to_string(),
                    span: call_expr.span,
                });
            }
        };
        
        // For now, assume all functions return i32
        let instruction = format!("  {} = call i32 @{}({})", result_name, function_name, args_str);
        self.ir_lines.push(instruction);
        
        Ok(result_name)
    }
    
    /// Infer the type of an expression (simplified)
    fn infer_expression_type(&self, expression: &Expression) -> Type {
        match expression {
            Expression::Literal(lit) => match lit {
                Literal::Integer(_, _) => Type::Int,
                Literal::Float(_, _) => Type::Float,
                Literal::Boolean(_, _) => Type::Bool,
                Literal::String(_, _) => Type::String,
            },
            Expression::Identifier(ident) => {
                if let Some((_, var_type)) = self.local_variables.get(&ident.name) {
                    var_type.clone()
                } else if let Some((_, var_type)) = self.parameters.get(&ident.name) {
                    var_type.clone()
                } else {
                    Type::Unknown
                }
            },
            Expression::Binary(_) => Type::Int, // Simplified
            Expression::Unary(_) => Type::Int,  // Simplified
            Expression::Call(_) => Type::Int,   // Simplified
            _ => Type::Unknown,
        }
    }
    
    /// Ensure the function has a proper return
    fn ensure_function_return(&mut self, return_type: &Option<Type>) -> Result<(), LLVMError> {
        // Check if the last instruction is already a return
        if let Some(last_line) = self.ir_lines.last() {
            if last_line.trim().starts_with("ret") {
                return Ok(()); // Already has a return
            }
        }
        
        // Add default return
        match return_type {
            None => {
                self.ir_lines.push("  ret void".to_string());
            },
            Some(Type::Int) => {
                self.ir_lines.push("  ret i32 0".to_string());
            },
            Some(Type::Float) => {
                self.ir_lines.push("  ret float 0.0".to_string());
            },
            Some(Type::Bool) => {
                self.ir_lines.push("  ret i1 0".to_string());
            },
            Some(_) => {
                self.ir_lines.push("  ret i32 0".to_string());
            }
        }
        
        Ok(())
    }
}
