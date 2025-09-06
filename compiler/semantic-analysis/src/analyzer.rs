//! Main semantic analyzer implementation
//!
//! This module orchestrates the semantic analysis process

use shared_types::*;
use crate::{SemanticError, SemanticInfo, Scope, Symbol, TypeChecker};
use std::collections::HashMap;

/// Main semantic analyzer
#[derive(Debug)]
pub struct SemanticAnalyzer {
    scope: Scope,
    type_info: HashMap<String, Type>,
    errors: Vec<SemanticError>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            scope: Scope::new(),
            type_info: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Analyze a complete program
    pub fn analyze(&mut self, program: &Program) -> Result<SemanticInfo, SemanticError> {
        // First pass: collect function and struct declarations
        for item in &program.items {
            if let Err(error) = self.declare_item(item) {
                self.errors.push(error);
            }
        }

        // Second pass: analyze function bodies
        for item in &program.items {
            if let Item::Function(func) = item {
                if let Err(error) = self.analyze_function(func) {
                    self.errors.push(error);
                }
            }
        }

        Ok(SemanticInfo {
            symbols: self.scope.all_symbols(),
            type_info: self.type_info.clone(),
            errors: self.errors.clone(),
        })
    }

    /// Declare top-level items (first pass)
    fn declare_item(&mut self, item: &Item) -> Result<(), SemanticError> {
        match item {
            Item::Function(func) => self.declare_function(func),
            Item::Struct(_struct) => Ok(()), // TODO: Implement struct declarations
            Item::Import(_) => Ok(()), // Imports don't affect local scope
        }
    }

    /// Declare a function in the global scope
    fn declare_function(&mut self, func: &Function) -> Result<(), SemanticError> {
        let param_types: Vec<Type> = func.parameters.iter()
            .map(|param| param.param_type.clone())
            .collect();

        let return_type = func.return_type.clone().unwrap_or(Type::Unknown);

        let symbol = Symbol::Function {
            name: func.name.clone(),
            params: param_types,
            return_type,
            span: func.span,
        };

        if self.scope.exists_in_current_scope(&func.name) {
            return Err(SemanticError::FunctionAlreadyDefined {
                name: func.name.clone(),
                span: func.span,
            });
        }

        self.scope.insert(symbol);
        Ok(())
    }

    /// Analyze a function body
    fn analyze_function(&mut self, func: &Function) -> Result<(), SemanticError> {
        // Create new scope for function
        self.scope.push_scope();

        // Add parameters to function scope
        for param in &func.parameters {
            let symbol = Symbol::Variable {
                name: param.name.clone(),
                var_type: param.param_type.clone(),
                mutable: false, // Parameters are immutable by default
                span: param.span,
            };

            if self.scope.exists_in_current_scope(&param.name) {
                self.errors.push(SemanticError::VariableAlreadyDefined {
                    name: param.name.clone(),
                    span: param.span,
                });
            } else {
                self.scope.insert(symbol);
            }
        }

        // Analyze function body
        let result = self.analyze_block(&func.body, func.return_type.as_ref());

        // Pop function scope
        self.scope.pop_scope();

        result
    }

    /// Analyze a block of statements
    fn analyze_block(&mut self, block: &Block, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        for statement in &block.statements {
            if let Err(error) = self.analyze_statement(statement, expected_return) {
                self.errors.push(error);
            }
        }
        Ok(())
    }

    /// Analyze a single statement
    fn analyze_statement(&mut self, stmt: &Statement, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        match stmt {
            Statement::Expression(expr) => {
                self.check_expression_type(expr)?;
                Ok(())
            }
            Statement::Let(let_stmt) => self.analyze_let_statement(let_stmt),
            Statement::Assignment(assign_stmt) => self.analyze_assignment_statement(assign_stmt),
            Statement::Return(ret_stmt) => self.analyze_return_statement(ret_stmt, expected_return),
            Statement::If(if_stmt) => self.analyze_if_statement(if_stmt, expected_return),
            Statement::While(while_stmt) => self.analyze_while_statement(while_stmt, expected_return),
            Statement::For(for_stmt) => self.analyze_for_statement(for_stmt, expected_return),
            Statement::Break(_) | Statement::Continue(_) => Ok(()), // TODO: Check if in loop
            Statement::Block(block) => {
                self.scope.push_scope();
                let result = self.analyze_block(block, expected_return);
                self.scope.pop_scope();
                result
            }
        }
    }

    /// Analyze let statement
    fn analyze_let_statement(&mut self, let_stmt: &LetStatement) -> Result<(), SemanticError> {
        // Check if variable already exists in current scope
        if self.scope.exists_in_current_scope(&let_stmt.name) {
            return Err(SemanticError::VariableAlreadyDefined {
                name: let_stmt.name.clone(),
                span: let_stmt.span,
            });
        }

        // Determine variable type
        let var_type = if let Some(explicit_type) = &let_stmt.var_type {
            // Use explicit type
            explicit_type.clone()
        } else if let Some(initializer) = &let_stmt.initializer {
            // Infer type from initializer
            self.check_expression_type(initializer)?
        } else {
            return Err(SemanticError::TypeMismatch {
                expected: "explicit type or initializer".to_string(),
                found: "neither".to_string(),
                span: let_stmt.span,
            });
        };

        // Check initializer type matches declared type
        if let Some(initializer) = &let_stmt.initializer {
            let init_type = self.check_expression_type(initializer)?;
            if let Some(explicit_type) = &let_stmt.var_type {
                if init_type != *explicit_type && init_type != Type::Unknown {
                    return Err(SemanticError::TypeMismatch {
                        expected: format!("{}", explicit_type),
                        found: format!("{}", init_type),
                        span: initializer.span(),
                    });
                }
            }
        }

        // Add variable to scope
        let symbol = Symbol::Variable {
            name: let_stmt.name.clone(),
            var_type: var_type.clone(),
            mutable: let_stmt.mutable,
            span: let_stmt.span,
        };

        self.scope.insert(symbol);
        self.type_info.insert(let_stmt.name.clone(), var_type);

        Ok(())
    }

    /// Analyze assignment statement
    fn analyze_assignment_statement(&mut self, assign_stmt: &AssignmentStatement) -> Result<(), SemanticError> {
        // Check if variable exists and is mutable
        match self.scope.lookup(&assign_stmt.target) {
            Some(Symbol::Variable { mutable: true, var_type, .. }) => {
                let value_type = self.check_expression_type(&assign_stmt.value)?;
                if value_type != *var_type && value_type != Type::Unknown {
                    return Err(SemanticError::TypeMismatch {
                        expected: format!("{}", var_type),
                        found: format!("{}", value_type),
                        span: assign_stmt.value.span(),
                    });
                }
                Ok(())
            }
            Some(Symbol::Variable { mutable: false, .. }) => {
                Err(SemanticError::AssignToImmutable {
                    name: assign_stmt.target.clone(),
                    span: assign_stmt.span,
                })
            }
            Some(Symbol::Function { .. }) => {
                Err(SemanticError::TypeMismatch {
                    expected: "variable".to_string(),
                    found: "function".to_string(),
                    span: assign_stmt.span,
                })
            }
            None => {
                Err(SemanticError::UndefinedVariable {
                    name: assign_stmt.target.clone(),
                    span: assign_stmt.span,
                })
            }
        }
    }

    /// Analyze return statement
    fn analyze_return_statement(&mut self, ret_stmt: &ReturnStatement, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        if let Some(value) = &ret_stmt.value {
            let return_type = self.check_expression_type(value)?;
            if let Some(expected) = expected_return {
                if return_type != *expected && return_type != Type::Unknown {
                    return Err(SemanticError::ReturnTypeMismatch {
                        function: "current function".to_string(), // TODO: Track current function name
                        expected: format!("{}", expected),
                        found: format!("{}", return_type),
                        span: value.span(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Analyze if statement
    fn analyze_if_statement(&mut self, if_stmt: &IfStatement, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        let condition_type = self.check_expression_type(&if_stmt.condition)?;
        if condition_type != Type::Bool && condition_type != Type::Unknown {
            return Err(SemanticError::TypeMismatch {
                expected: "bool".to_string(),
                found: format!("{}", condition_type),
                span: if_stmt.condition.span(),
            });
        }

        self.scope.push_scope();
        self.analyze_block(&if_stmt.then_branch, expected_return)?;
        self.scope.pop_scope();

        if let Some(else_branch) = &if_stmt.else_branch {
            self.scope.push_scope();
            self.analyze_block(else_branch, expected_return)?;
            self.scope.pop_scope();
        }

        Ok(())
    }

    /// Analyze while statement
    fn analyze_while_statement(&mut self, while_stmt: &WhileStatement, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        let condition_type = self.check_expression_type(&while_stmt.condition)?;
        if condition_type != Type::Bool && condition_type != Type::Unknown {
            return Err(SemanticError::TypeMismatch {
                expected: "bool".to_string(),
                found: format!("{}", condition_type),
                span: while_stmt.condition.span(),
            });
        }

        self.scope.push_scope();
        let result = self.analyze_block(&while_stmt.body, expected_return);
        self.scope.pop_scope();

        result
    }

    /// Analyze for statement
    fn analyze_for_statement(&mut self, for_stmt: &ForStatement, expected_return: Option<&Type>) -> Result<(), SemanticError> {
        // TODO: Implement proper for loop analysis with iterables
        self.scope.push_scope();
        
        // Add iterator variable to scope (assuming int for now)
        let symbol = Symbol::Variable {
            name: for_stmt.variable.clone(),
            var_type: Type::Int,
            mutable: false,
            span: for_stmt.body.span,
        };
        self.scope.insert(symbol);
        
        let result = self.analyze_block(&for_stmt.body, expected_return);
        self.scope.pop_scope();

        result
    }

    /// Check expression type using type checker
    fn check_expression_type(&self, expr: &Expression) -> Result<Type, SemanticError> {
        let type_checker = TypeChecker::new(&self.scope);
        type_checker.check_expression(expr)
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}