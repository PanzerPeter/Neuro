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
#[derive(Clone)]
pub struct TextBasedFunctionBuilder {
    /// Current variable counter for generating unique SSA names
    var_counter: u32,

    /// Symbol table for local variables
    local_variables: HashMap<String, (String, Type)>, // (llvm_name, type)

    /// Function parameters
    parameters: HashMap<String, (String, Type)>, // (llvm_name, type)

    /// Function signature registry for return types
    function_signatures: HashMap<String, Type>, // (function_name, return_type)

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
            function_signatures: HashMap::new(),
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

        // Register function signature for return type lookup
        let return_type = function.return_type.as_ref().unwrap_or(&Type::Int).clone();
        self.function_signatures.insert(function.name.clone(), return_type);

        self.ir_lines.clear();
        self.local_variables.clear();
        self.parameters.clear();
        self.var_counter = 0;
        
        // Generate function signature
        let return_type = self.map_optional_type_to_llvm(&function.return_type);
        let mut param_types = Vec::new();
        
        for (i, param) in function.parameters.iter().enumerate() {
            let llvm_type = self.map_type_to_llvm(&param.param_type);
            let param_name = format!("%param_{}", i);
            param_types.push(format!("{} {}", llvm_type, param_name));

            // Store parameter info
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
            self.ir_lines.push(format!("  store {} {}, ptr {}", llvm_type, param_name, alloca_name));
            
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

    /// Infer the type of an expression (simplified type inference)
    fn infer_expression_type(&self, expression: &Expression) -> Type {
        match expression {
            Expression::Literal(literal) => {
                match literal {
                    Literal::Integer(_, _) => Type::Int,
                    Literal::Float(_, _) => Type::Float,
                    Literal::Boolean(_, _) => Type::Bool,
                    Literal::String(_, _) => Type::String,
                }
            },
            Expression::Identifier(ident) => {
                // Look up variable type
                if let Some((_, var_type)) = self.local_variables.get(&ident.name) {
                    var_type.clone()
                } else if let Some((_, param_type)) = self.parameters.get(&ident.name) {
                    param_type.clone()
                } else {
                    Type::Unknown
                }
            },
            Expression::Binary(bin_expr) => {
                // For comparison operators, result is always boolean
                match bin_expr.operator {
                    BinaryOperator::Equal | BinaryOperator::NotEqual |
                    BinaryOperator::Less | BinaryOperator::LessEqual |
                    BinaryOperator::Greater | BinaryOperator::GreaterEqual |
                    BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr => Type::Bool,
                    _ => {
                        // For arithmetic operators, use the type of the left operand
                        self.infer_expression_type(&bin_expr.left)
                    }
                }
            },
            Expression::Unary(unary_expr) => {
                match unary_expr.operator {
                    UnaryOperator::Not => Type::Bool,
                    UnaryOperator::Negate => self.infer_expression_type(&unary_expr.operand),
                    _ => Type::Unknown,
                }
            },
            Expression::Call(call_expr) => {
                // Look up function return type
                if let Expression::Identifier(ident) = call_expr.function.as_ref() {
                    self.function_signatures.get(&ident.name).cloned().unwrap_or(Type::Unknown)
                } else {
                    Type::Unknown
                }
            },
            _ => Type::Unknown,
        }
    }
    
    /// Compile a statement
    fn compile_statement(&mut self, statement: &Statement) -> Result<(), LLVMError> {
        match statement {
            Statement::Let(let_stmt) => {
                // Infer type from initializer if no explicit type is given
                let var_type = if let Some(explicit_type) = &let_stmt.var_type {
                    explicit_type.clone()
                } else if let Some(init) = &let_stmt.initializer {
                    self.infer_expression_type(init)
                } else {
                    Type::Unknown
                };

                let llvm_type = self.map_type_to_llvm(&var_type);
                let alloca_name = format!("%{}_addr", let_stmt.name);

                // Create alloca
                self.ir_lines.push(format!("  {} = alloca {}", alloca_name, llvm_type));

                // Initialize if there's an initializer
                if let Some(init) = &let_stmt.initializer {
                    let init_value = self.compile_expression(init)?;
                    self.ir_lines.push(format!("  store {} {}, ptr {}", llvm_type, init_value, alloca_name));
                }

                // Store variable info
                self.local_variables.insert(let_stmt.name.clone(), (alloca_name, var_type));
            },
            
            Statement::Assignment(assign_stmt) => {
                // Look up the variable
                if let Some((alloca_name, var_type)) = self.local_variables.get(&assign_stmt.target).cloned() {
                    let llvm_type = self.map_type_to_llvm(&var_type);
                    let value = self.compile_expression(&assign_stmt.value)?;
                    self.ir_lines.push(format!("  store {} {}, ptr {}", llvm_type, value, alloca_name));
                } else if let Some((param_name, var_type)) = self.parameters.get(&assign_stmt.target).cloned() {
                    // Assignment to parameter
                    let llvm_type = self.map_type_to_llvm(&var_type);
                    let value = self.compile_expression(&assign_stmt.value)?;
                    self.ir_lines.push(format!("  store {} {}, {} {}", llvm_type, value, llvm_type, param_name));
                } else {
                    return Err(LLVMError::CodeGeneration {
                        message: format!("Undefined variable '{}' in assignment", assign_stmt.target),
                        span: assign_stmt.span,
                    });
                }
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
            
            Statement::If(if_stmt) => {
                self.compile_if_statement(if_stmt)?;
            },
            
            Statement::While(while_stmt) => {
                self.compile_while_statement(while_stmt)?;
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
    
    /// Compile an if statement
    fn compile_if_statement(&mut self, if_stmt: &shared_types::ast::IfStatement) -> Result<(), LLVMError> {
        let cond_value = self.compile_expression(&if_stmt.condition)?;
        
        // Generate unique labels
        let then_label = format!("if.then.{}", self.var_counter);
        let else_label = format!("if.else.{}", self.var_counter);
        let end_label = format!("if.end.{}", self.var_counter);
        self.var_counter += 1;
        
        // Branch based on condition
        if if_stmt.else_branch.is_some() {
            self.ir_lines.push(format!("  br i1 {}, label %{}, label %{}", cond_value, then_label, else_label));
        } else {
            self.ir_lines.push(format!("  br i1 {}, label %{}, label %{}", cond_value, then_label, end_label));
        }
        
        // Then block
        self.ir_lines.push(format!("{}:", then_label));
        for stmt in &if_stmt.then_branch.statements {
            self.compile_statement(stmt)?;
        }
        self.ir_lines.push(format!("  br label %{}", end_label));
        
        // Else block (if present)
        if let Some(else_block) = &if_stmt.else_branch {
            self.ir_lines.push(format!("{}:", else_label));
            for stmt in &else_block.statements {
                self.compile_statement(stmt)?;
            }
            self.ir_lines.push(format!("  br label %{}", end_label));
        }
        
        // End block
        self.ir_lines.push(format!("{}:", end_label));
        
        Ok(())
    }
    
    /// Compile a while statement
    fn compile_while_statement(&mut self, while_stmt: &shared_types::ast::WhileStatement) -> Result<(), LLVMError> {
        // Generate unique labels
        let loop_header = format!("while.cond.{}", self.var_counter);
        let loop_body = format!("while.body.{}", self.var_counter);
        let loop_end = format!("while.end.{}", self.var_counter);
        self.var_counter += 1;
        
        // Jump to condition check
        self.ir_lines.push(format!("  br label %{}", loop_header));
        
        // Loop condition block
        self.ir_lines.push(format!("{}:", loop_header));
        let cond_value = self.compile_expression(&while_stmt.condition)?;
        self.ir_lines.push(format!("  br i1 {}, label %{}, label %{}", cond_value, loop_body, loop_end));
        
        // Loop body
        self.ir_lines.push(format!("{}:", loop_body));
        for stmt in &while_stmt.body.statements {
            self.compile_statement(stmt)?;
        }
        self.ir_lines.push(format!("  br label %{}", loop_header));
        
        // Loop end
        self.ir_lines.push(format!("{}:", loop_end));
        
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
            Literal::Float(f, _span) => {
                // Ensure float literals have decimal points for LLVM IR
                let formatted = format!("{}", f);
                if formatted.contains('.') {
                    Ok(formatted)
                } else {
                    Ok(format!("{}.0", formatted))
                }
            },
            Literal::Boolean(b, _span) => Ok(if *b { "true" } else { "false" }.to_string()),
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
            self.ir_lines.push(format!("  {} = load {}, ptr {}", load_name, llvm_type, alloca_name));
            Ok(load_name)
        }
        // Check parameters
        else if let Some((param_name, var_type)) = self.parameters.get(name).cloned() {
            let load_name = self.next_var_name();
            let llvm_type = self.map_type_to_llvm(&var_type);
            self.ir_lines.push(format!("  {} = load {}, ptr {}", load_name, llvm_type, param_name));
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

        // Determine operation type based on operands (simplified type inference)
        let op_type = self.infer_expression_type(&bin_expr.left);
        let llvm_type = self.map_type_to_llvm(&op_type);

        let instruction = match bin_expr.operator {
            BinaryOperator::Add => {
                if op_type == Type::Float {
                    format!("  {} = fadd float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = add i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Subtract => {
                if op_type == Type::Float {
                    format!("  {} = fsub float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = sub i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Multiply => {
                if op_type == Type::Float {
                    format!("  {} = fmul float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = mul i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Divide => {
                if op_type == Type::Float {
                    format!("  {} = fdiv float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = sdiv i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Equal => {
                if op_type == Type::Float {
                    format!("  {} = fcmp oeq float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp eq i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::NotEqual => {
                if op_type == Type::Float {
                    format!("  {} = fcmp one float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp ne i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Less => {
                if op_type == Type::Float {
                    format!("  {} = fcmp olt float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp slt i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::LessEqual => {
                if op_type == Type::Float {
                    format!("  {} = fcmp ole float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp sle i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::Greater => {
                if op_type == Type::Float {
                    format!("  {} = fcmp ogt float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp sgt i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::GreaterEqual => {
                if op_type == Type::Float {
                    format!("  {} = fcmp oge float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = icmp sge i32 {}, {}", result_name, left, right)
                }
            },
            BinaryOperator::LogicalAnd => format!("  {} = and i1 {}, {}", result_name, left, right),
            BinaryOperator::LogicalOr => format!("  {} = or i1 {}, {}", result_name, left, right),
            BinaryOperator::Modulo => {
                if op_type == Type::Float {
                    format!("  {} = frem float {}, {}", result_name, left, right)
                } else {
                    format!("  {} = srem i32 {}, {}", result_name, left, right)
                }
            },
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
        
        // Determine operation type
        let op_type = self.infer_expression_type(&unary_expr.operand);

        let instruction = match unary_expr.operator {
            UnaryOperator::Negate => {
                if op_type == Type::Float {
                    format!("  {} = fsub float 0.0, {}", result_name, operand)
                } else {
                    format!("  {} = sub i32 0, {}", result_name, operand)
                }
            },
            UnaryOperator::Not => format!("  {} = xor i1 {}, true", result_name, operand),
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
        let function_name = match call_expr.function.as_ref() {
            Expression::Identifier(ident) => &ident.name,
            _ => {
                return Err(LLVMError::CodeGeneration {
                    message: "Only simple function calls supported for now".to_string(),
                    span: call_expr.span,
                });
            }
        };
        
        // Determine return type from function signature registry
        let return_type = self.function_signatures.get(function_name)
            .cloned()
            .unwrap_or(Type::Int); // Default to int if not found
        let llvm_return_type = self.map_type_to_llvm(&return_type);

        let instruction = format!("  {} = call {} @{}({})", result_name, llvm_return_type, function_name, args_str);
        self.ir_lines.push(instruction);
        
        Ok(result_name)
    }
    
    /// Infer the type of an expression (simplified)
    
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

    /// Register a function signature for type inference
    pub fn register_function_signature(&mut self, function: &Function) {
        if let Some(return_type) = &function.return_type {
            self.function_signatures.insert(function.name.clone(), return_type.clone());
        }
    }
}
