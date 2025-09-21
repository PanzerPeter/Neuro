//! JIT executor for running NEURO programs without external LLVM tools
//!
//! This module provides a simple JIT-like execution environment that can run
//! NEURO programs directly from the AST without requiring external linkers.

use crate::LLVMError;
use shared_types::{Program, ast::*};
use std::collections::HashMap;

/// JIT execution result
#[derive(Debug, Clone)]
pub struct JitResult {
    pub exit_code: i32,
    pub output: Vec<String>,
}

/// Simple JIT executor for NEURO programs
pub struct JitExecutor {
    _globals: HashMap<String, Value>, // Reserved for future global variable support
}

/// Runtime value representation
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i32),
    Float(f64),
    Bool(bool),
    String(String),
    Void,
}

impl JitExecutor {
    /// Create a new JIT executor
    pub fn new() -> Self {
        Self {
            _globals: HashMap::new(),
        }
    }

    /// Execute a NEURO program using JIT compilation
    pub fn execute(&mut self, program: &Program) -> Result<JitResult, LLVMError> {
        tracing::info!("Starting JIT execution");

        // Find the main function
        let main_function = program.items.iter()
            .find_map(|item| {
                if let Item::Function(func) = item {
                    if func.name == "main" {
                        Some(func)
                    } else {
                        None
                    }
                } else {
                    None
                }
            });

        let main_function = main_function.ok_or_else(|| {
            LLVMError::ModuleGeneration {
                message: "No main function found".to_string(),
            }
        })?;

        // Execute main function
        let mut context = ExecutionContext::new();
        let result = self.execute_function(main_function, &mut context)?;

        let exit_code = match result {
            Value::Int(code) => code,
            Value::Void => 0,
            _ => {
                return Err(LLVMError::ModuleGeneration {
                    message: format!("Main function returned unexpected type: {:?}", result),
                });
            }
        };

        Ok(JitResult {
            exit_code,
            output: context.output,
        })
    }

    /// Execute a function
    fn execute_function(
        &mut self,
        function: &Function,
        context: &mut ExecutionContext,
    ) -> Result<Value, LLVMError> {
        tracing::debug!("Executing function: {}", function.name);

        // Execute function body
        let mut result = Value::Void;
        for statement in &function.body.statements {
            match self.execute_statement(statement, context)? {
                StatementResult::Continue => continue,
                StatementResult::Return(value) => {
                    result = value;
                    break;
                }
                StatementResult::Break | StatementResult::ControlFlow => {
                    break;
                }
            }
        }

        Ok(result)
    }

    /// Execute a statement
    fn execute_statement(
        &mut self,
        statement: &Statement,
        context: &mut ExecutionContext,
    ) -> Result<StatementResult, LLVMError> {
        match statement {
            Statement::Let(let_stmt) => {
                let value = if let Some(ref expr) = let_stmt.initializer {
                    self.evaluate_expression(expr, context)?
                } else {
                    Value::Int(0) // Default initialization
                };

                context.set_variable(&let_stmt.name, value);
                Ok(StatementResult::Continue)
            }
            Statement::Return(return_stmt) => {
                let value = if let Some(ref expr) = return_stmt.value {
                    self.evaluate_expression(expr, context)?
                } else {
                    Value::Void
                };
                Ok(StatementResult::Return(value))
            }
            Statement::Expression(expr) => {
                let _value = self.evaluate_expression(expr, context)?;
                Ok(StatementResult::Continue)
            }
            Statement::If(if_stmt) => {
                let condition = self.evaluate_expression(&if_stmt.condition, context)?;
                let should_execute = match condition {
                    Value::Bool(b) => b,
                    Value::Int(i) => i != 0,
                    _ => false,
                };

                if should_execute {
                    for stmt in &if_stmt.then_branch.statements {
                        match self.execute_statement(stmt, context)? {
                            StatementResult::Continue => continue,
                            other => return Ok(other),
                        }
                    }
                } else if let Some(ref else_branch) = if_stmt.else_branch {
                    for stmt in &else_branch.statements {
                        match self.execute_statement(stmt, context)? {
                            StatementResult::Continue => continue,
                            other => return Ok(other),
                        }
                    }
                }

                Ok(StatementResult::Continue)
            }
            Statement::While(while_stmt) => {
                loop {
                    let condition = self.evaluate_expression(&while_stmt.condition, context)?;
                    let should_continue = match condition {
                        Value::Bool(b) => b,
                        Value::Int(i) => i != 0,
                        _ => false,
                    };

                    if !should_continue {
                        break;
                    }

                    for stmt in &while_stmt.body.statements {
                        match self.execute_statement(stmt, context)? {
                            StatementResult::Continue => continue,
                            StatementResult::Break => return Ok(StatementResult::Continue),
                            other => return Ok(other),
                        }
                    }
                }

                Ok(StatementResult::Continue)
            }
            _ => {
                tracing::warn!("Unsupported statement type: {:?}", statement);
                Ok(StatementResult::Continue)
            }
        }
    }

    /// Evaluate an expression
    fn evaluate_expression(
        &mut self,
        expression: &Expression,
        context: &mut ExecutionContext,
    ) -> Result<Value, LLVMError> {
        match expression {
            Expression::Literal(lit) => Ok(self.evaluate_literal(lit)?),
            Expression::Identifier(ident) => {
                context.get_variable(&ident.name).ok_or_else(|| {
                    LLVMError::CodeGeneration {
                        message: format!("Undefined variable: {}", ident.name),
                        span: ident.span,
                    }
                })
            }
            Expression::Binary(binary) => {
                let left = self.evaluate_expression(&binary.left, context)?;
                let right = self.evaluate_expression(&binary.right, context)?;
                self.evaluate_binary_operation(&binary.operator, left, right)
            }
            Expression::Unary(unary) => {
                let operand = self.evaluate_expression(&unary.operand, context)?;
                self.evaluate_unary_operation(&unary.operator, operand)
            }
            Expression::Call(call) => {
                // Extract function name from the function expression
                let function_name = match call.function.as_ref() {
                    Expression::Identifier(ident) => &ident.name,
                    _ => return Ok(Value::Int(0)), // Unknown function call structure
                };

                // Handle built-in functions
                if function_name == "print" {
                    if let Some(arg) = call.arguments.first() {
                        let value = self.evaluate_expression(arg, context)?;
                        let output = match value {
                            Value::Int(n) => n.to_string(),
                            Value::Float(f) => f.to_string(),
                            Value::Bool(b) => b.to_string(),
                            Value::String(s) => s,
                            Value::Void => "void".to_string(),
                        };
                        context.output.push(output.clone());
                    }
                    Ok(Value::Void)
                } else {
                    // For now, return a default value for unknown functions
                    Ok(Value::Int(0))
                }
            }
            _ => {
                tracing::warn!("Unsupported expression type: {:?}", expression);
                Ok(Value::Int(0))
            }
        }
    }

    /// Evaluate a literal
    fn evaluate_literal(&self, literal: &Literal) -> Result<Value, LLVMError> {
        match literal {
            Literal::Integer(n, _) => Ok(Value::Int(*n as i32)),
            Literal::Float(f, _) => Ok(Value::Float(*f)),
            Literal::Boolean(b, _) => Ok(Value::Bool(*b)),
            Literal::String(s, _) => Ok(Value::String(s.clone())),
        }
    }

    /// Evaluate binary operation
    fn evaluate_binary_operation(
        &self,
        op: &BinaryOperator,
        left: Value,
        right: Value,
    ) -> Result<Value, LLVMError> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                let result = match op {
                    BinaryOperator::Add => a + b,
                    BinaryOperator::Subtract => a - b,
                    BinaryOperator::Multiply => a * b,
                    BinaryOperator::Divide => a / b,
                    BinaryOperator::Modulo => a % b,
                    BinaryOperator::Equal => return Ok(Value::Bool(a == b)),
                    BinaryOperator::NotEqual => return Ok(Value::Bool(a != b)),
                    BinaryOperator::Less => return Ok(Value::Bool(a < b)),
                    BinaryOperator::LessEqual => return Ok(Value::Bool(a <= b)),
                    BinaryOperator::Greater => return Ok(Value::Bool(a > b)),
                    BinaryOperator::GreaterEqual => return Ok(Value::Bool(a >= b)),
                    BinaryOperator::LogicalAnd => return Ok(Value::Bool((a != 0) && (b != 0))),
                    BinaryOperator::LogicalOr => return Ok(Value::Bool((a != 0) || (b != 0))),
                };
                Ok(Value::Int(result))
            }
            _ => Ok(Value::Int(0)), // Default for unsupported operations
        }
    }

    /// Evaluate unary operation
    fn evaluate_unary_operation(
        &self,
        op: &UnaryOperator,
        operand: Value,
    ) -> Result<Value, LLVMError> {
        match operand {
            Value::Int(n) => {
                let result = match op {
                    UnaryOperator::Negate => -n,
                    UnaryOperator::Not => if n == 0 { 1 } else { 0 },
                };
                Ok(Value::Int(result))
            }
            Value::Bool(b) => {
                match op {
                    UnaryOperator::Not => Ok(Value::Bool(!b)),
                    _ => Ok(Value::Bool(b)),
                }
            }
            _ => Ok(Value::Int(0)), // Default
        }
    }
}

/// Execution context for variables and program state
struct ExecutionContext {
    variables: HashMap<String, Value>,
    output: Vec<String>,
}

impl ExecutionContext {
    fn new() -> Self {
        Self {
            variables: HashMap::new(),
            output: Vec::new(),
        }
    }

    fn set_variable(&mut self, name: &str, value: Value) {
        self.variables.insert(name.to_string(), value);
    }

    fn get_variable(&self, name: &str) -> Option<Value> {
        self.variables.get(name).cloned()
    }
}

/// Statement execution result
enum StatementResult {
    Continue,
    Return(Value),
    #[allow(dead_code)]
    Break, // Reserved for future loop support
    #[allow(dead_code)]
    ControlFlow, // Reserved for future control flow support
}

impl Default for JitExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a NEURO program using JIT compilation
pub fn execute_program(program: &Program) -> Result<JitResult, LLVMError> {
    let mut executor = JitExecutor::new();
    executor.execute(program)
}