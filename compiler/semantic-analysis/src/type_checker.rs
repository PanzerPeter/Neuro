//! Type checking functionality
//!
//! This module provides type inference and checking for NEURO expressions

use shared_types::*;
use shared_types::ast::{Literal, Identifier};
use crate::{SemanticError, Scope, Symbol};

/// Type checker for expressions and statements
#[derive(Debug)]
pub struct TypeChecker<'a> {
    scope: &'a Scope,
}

impl<'a> TypeChecker<'a> {
    pub fn new(scope: &'a Scope) -> Self {
        Self { scope }
    }

    /// Check the type of an expression
    pub fn check_expression(&self, expr: &Expression) -> Result<Type, SemanticError> {
        match expr {
            Expression::Literal(lit) => Ok(self.literal_type(lit)),
            Expression::Identifier(id) => self.check_identifier(id),
            Expression::Binary(bin) => self.check_binary_expression(bin),
            Expression::Unary(un) => self.check_unary_expression(un),
            Expression::Call(call) => self.check_call_expression(call),
            Expression::Index(_) => Ok(Type::Unknown), // TODO: Implement array indexing
            Expression::Member(_) => Ok(Type::Unknown), // TODO: Implement member access
            Expression::TensorLiteral(_) => Ok(Type::Tensor { 
                element_type: Box::new(Type::Float), 
                shape: None 
            }),
        }
    }

    /// Get the type of a literal
    fn literal_type(&self, lit: &Literal) -> Type {
        match lit {
            Literal::Integer(_, _) => Type::Int,
            Literal::Float(_, _) => Type::Float,
            Literal::String(_, _) => Type::String,
            Literal::Boolean(_, _) => Type::Bool,
        }
    }

    /// Check identifier type
    fn check_identifier(&self, id: &Identifier) -> Result<Type, SemanticError> {
        match self.scope.lookup(&id.name) {
            Some(Symbol::Variable { var_type, .. }) => Ok(var_type.clone()),
            Some(Symbol::Function { .. }) => Err(SemanticError::TypeMismatch {
                expected: "variable".to_string(),
                found: "function".to_string(),
                span: id.span,
            }),
            None => Err(SemanticError::UndefinedVariable {
                name: id.name.clone(),
                span: id.span,
            }),
        }
    }

    /// Check binary expression type
    fn check_binary_expression(&self, bin: &BinaryExpression) -> Result<Type, SemanticError> {
        let left_type = self.check_expression(&bin.left)?;
        let right_type = self.check_expression(&bin.right)?;

        match bin.operator {
            BinaryOperator::Add | BinaryOperator::Subtract | 
            BinaryOperator::Multiply | BinaryOperator::Divide | 
            BinaryOperator::Modulo => {
                // Arithmetic operators require numeric types
                match (&left_type, &right_type) {
                    (Type::Int, Type::Int) => Ok(Type::Int),
                    (Type::Float, Type::Float) => Ok(Type::Float),
                    (Type::Int, Type::Float) | (Type::Float, Type::Int) => Ok(Type::Float),
                    _ => Err(SemanticError::TypeMismatch {
                        expected: "numeric types".to_string(),
                        found: format!("{} and {}", left_type, right_type),
                        span: bin.span,
                    }),
                }
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                // Equality operators work on same types
                if left_type == right_type {
                    Ok(Type::Bool)
                } else {
                    Err(SemanticError::TypeMismatch {
                        expected: format!("{}", left_type),
                        found: format!("{}", right_type),
                        span: bin.span,
                    })
                }
            }
            BinaryOperator::Less | BinaryOperator::LessEqual |
            BinaryOperator::Greater | BinaryOperator::GreaterEqual => {
                // Comparison operators require numeric types
                match (&left_type, &right_type) {
                    (Type::Int, Type::Int) | (Type::Float, Type::Float) |
                    (Type::Int, Type::Float) | (Type::Float, Type::Int) => Ok(Type::Bool),
                    _ => Err(SemanticError::TypeMismatch {
                        expected: "numeric types".to_string(),
                        found: format!("{} and {}", left_type, right_type),
                        span: bin.span,
                    }),
                }
            }
        }
    }

    /// Check unary expression type
    fn check_unary_expression(&self, un: &UnaryExpression) -> Result<Type, SemanticError> {
        let operand_type = self.check_expression(&un.operand)?;

        match un.operator {
            UnaryOperator::Negate => {
                match operand_type {
                    Type::Int | Type::Float => Ok(operand_type),
                    _ => Err(SemanticError::TypeMismatch {
                        expected: "numeric type".to_string(),
                        found: format!("{}", operand_type),
                        span: un.span,
                    }),
                }
            }
            UnaryOperator::Not => {
                match operand_type {
                    Type::Bool => Ok(Type::Bool),
                    _ => Err(SemanticError::TypeMismatch {
                        expected: "bool".to_string(),
                        found: format!("{}", operand_type),
                        span: un.span,
                    }),
                }
            }
        }
    }

    /// Check function call expression
    fn check_call_expression(&self, call: &CallExpression) -> Result<Type, SemanticError> {
        // Get the function being called
        let function_name = match call.function.as_ref() {
            Expression::Identifier(id) => &id.name,
            _ => return Ok(Type::Unknown), // TODO: Support more complex function expressions
        };

        match self.scope.lookup(function_name) {
            Some(Symbol::Function { params, return_type, .. }) => {
                // Check argument count
                if call.arguments.len() != params.len() {
                    return Err(SemanticError::ArgumentCountMismatch {
                        name: function_name.clone(),
                        expected: params.len(),
                        found: call.arguments.len(),
                        span: call.span,
                    });
                }

                // Check argument types
                for (arg, expected_type) in call.arguments.iter().zip(params.iter()) {
                    let arg_type = self.check_expression(arg)?;
                    if arg_type != *expected_type && arg_type != Type::Unknown {
                        return Err(SemanticError::TypeMismatch {
                            expected: format!("{}", expected_type),
                            found: format!("{}", arg_type),
                            span: arg.span(),
                        });
                    }
                }

                Ok(return_type.clone())
            }
            Some(Symbol::Variable { .. }) => Err(SemanticError::TypeMismatch {
                expected: "function".to_string(),
                found: "variable".to_string(),
                span: call.span,
            }),
            None => Err(SemanticError::FunctionNotFound {
                name: function_name.clone(),
                span: call.span,
            }),
        }
    }
}