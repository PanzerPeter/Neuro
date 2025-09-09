//! Simple expression evaluator for NEURO
//! 
//! This module provides a basic interpreter for evaluating NEURO expressions.
//! It can evaluate literals, binary operations, and simple function calls.

use shared_types::{
    Expression, Value, 
    ast::{Literal, BinaryExpression, BinaryOperator, 
         UnaryExpression, UnaryOperator, CallExpression, Identifier}
};
use std::collections::HashMap;

/// Runtime error during evaluation
#[derive(Debug, Clone)]
pub struct EvalError {
    pub message: String,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Runtime Error: {}", self.message)
    }
}

impl std::error::Error for EvalError {}

impl EvalError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Simple environment for variable storage
pub struct Environment {
    variables: HashMap<String, Value>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    pub fn define(&mut self, name: String, value: Value) {
        self.variables.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn set(&mut self, name: String, value: Value) -> Result<(), EvalError> {
        if self.variables.contains_key(&name) {
            self.variables.insert(name, value);
            Ok(())
        } else {
            Err(EvalError::new(format!("Undefined variable: {}", name)))
        }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

/// Expression evaluator
pub struct Evaluator {
    environment: Environment,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            environment: Environment::new(),
        }
    }

    pub fn with_environment(environment: Environment) -> Self {
        Self { environment }
    }

    /// Define a variable in the environment
    pub fn define(&mut self, name: String, value: Value) {
        self.environment.define(name, value);
    }

    /// Evaluate an expression
    pub fn evaluate(&mut self, expr: &Expression) -> Result<Value, EvalError> {
        match expr {
            Expression::Literal(lit) => self.evaluate_literal(lit),
            Expression::Identifier(id) => self.evaluate_identifier(id),
            Expression::Binary(bin) => self.evaluate_binary(bin),
            Expression::Unary(un) => self.evaluate_unary(un),
            Expression::Call(call) => self.evaluate_call(call),
            _ => Err(EvalError::new("Unsupported expression type")),
        }
    }

    fn evaluate_literal(&self, literal: &Literal) -> Result<Value, EvalError> {
        match literal {
            Literal::Integer(val, _) => Ok(Value::Integer(*val)),
            Literal::Float(val, _) => Ok(Value::Float(*val)),
            Literal::String(val, _) => Ok(Value::String(val.clone())),
            Literal::Boolean(val, _) => Ok(Value::Boolean(*val)),
        }
    }

    fn evaluate_identifier(&self, identifier: &Identifier) -> Result<Value, EvalError> {
        match self.environment.get(&identifier.name) {
            Some(value) => Ok(value.clone()),
            None => Err(EvalError::new(format!("Undefined variable: {}", identifier.name))),
        }
    }

    fn evaluate_binary(&mut self, binary: &BinaryExpression) -> Result<Value, EvalError> {
        let left = self.evaluate(&binary.left)?;
        let right = self.evaluate(&binary.right)?;

        match binary.operator {
            BinaryOperator::Add => left.add(&right).map_err(EvalError::new),
            BinaryOperator::Subtract => left.subtract(&right).map_err(EvalError::new),
            BinaryOperator::Multiply => left.multiply(&right).map_err(EvalError::new),
            BinaryOperator::Divide => left.divide(&right).map_err(EvalError::new),
            BinaryOperator::Modulo => left.modulo(&right).map_err(EvalError::new),
            BinaryOperator::Equal => Ok(left.equals(&right)),
            BinaryOperator::NotEqual => Ok(left.not_equals(&right)),
            BinaryOperator::Less => left.less_than(&right).map_err(EvalError::new),
            BinaryOperator::LessEqual => left.less_equal(&right).map_err(EvalError::new),
            BinaryOperator::Greater => left.greater_than(&right).map_err(EvalError::new),
            BinaryOperator::GreaterEqual => left.greater_equal(&right).map_err(EvalError::new),
            BinaryOperator::LogicalAnd => Ok(left.logical_and(&right)),
            BinaryOperator::LogicalOr => Ok(left.logical_or(&right)),
        }
    }

    fn evaluate_unary(&mut self, unary: &UnaryExpression) -> Result<Value, EvalError> {
        let operand = self.evaluate(&unary.operand)?;

        match unary.operator {
            UnaryOperator::Negate => operand.negate().map_err(EvalError::new),
            UnaryOperator::Not => Ok(operand.logical_not()),
        }
    }

    fn evaluate_call(&mut self, call: &CallExpression) -> Result<Value, EvalError> {
        // For now, only support built-in functions
        match &*call.function {
            Expression::Identifier(id) => {
                match id.name.as_str() {
                    "print" => {
                        if call.arguments.len() != 1 {
                            return Err(EvalError::new("print() expects exactly 1 argument"));
                        }
                        let value = self.evaluate(&call.arguments[0])?;
                        println!("{}", value);
                        Ok(Value::Null)
                    }
                    "type_of" => {
                        if call.arguments.len() != 1 {
                            return Err(EvalError::new("type_of() expects exactly 1 argument"));
                        }
                        let value = self.evaluate(&call.arguments[0])?;
                        Ok(Value::String(value.type_name().to_string()))
                    }
                    _ => Err(EvalError::new(format!("Unknown function: {}", id.name))),
                }
            }
            _ => Err(EvalError::new("Function calls must use identifiers")),
        }
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::ast::{Literal, BinaryExpression, BinaryOperator, Identifier};
    use shared_types::Span;

    #[test]
    fn test_evaluate_integer_literal() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Literal(Literal::Integer(42, Span::new(0, 2)));
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_evaluate_binary_addition() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Literal(Literal::Integer(2, Span::new(0, 1)))),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Literal(Literal::Integer(3, Span::new(2, 3)))),
            span: Span::new(0, 3),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Integer(5));
    }

    #[test]
    fn test_evaluate_binary_subtraction() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Literal(Literal::Integer(10, Span::new(0, 2)))),
            operator: BinaryOperator::Subtract,
            right: Box::new(Expression::Literal(Literal::Integer(3, Span::new(3, 4)))),
            span: Span::new(0, 4),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Integer(7));
    }

    #[test]
    fn test_evaluate_variable() {
        let mut evaluator = Evaluator::new();
        evaluator.define("x".to_string(), Value::Integer(42));
        
        let expr = Expression::Identifier(Identifier {
            name: "x".to_string(),
            span: Span::new(0, 1),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Integer(42));
    }

    #[test]
    fn test_evaluate_undefined_variable() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Identifier(Identifier {
            name: "unknown".to_string(),
            span: Span::new(0, 7),
        });
        
        let result = evaluator.evaluate(&expr);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_string_concatenation() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Literal(Literal::String("Hello".to_string(), Span::new(0, 7)))),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Literal(Literal::String(" World".to_string(), Span::new(8, 16)))),
            span: Span::new(0, 16),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::String("Hello World".to_string()));
    }

    #[test]
    fn test_evaluate_comparison() {
        let mut evaluator = Evaluator::new();
        let expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Literal(Literal::Integer(5, Span::new(0, 1)))),
            operator: BinaryOperator::Greater,
            right: Box::new(Expression::Literal(Literal::Integer(3, Span::new(2, 3)))),
            span: Span::new(0, 3),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Boolean(true));
    }

    #[test]
    fn test_evaluate_complex_expression() {
        let mut evaluator = Evaluator::new();
        // (2 + 3) * 4 = 20
        let expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Binary(BinaryExpression {
                left: Box::new(Expression::Literal(Literal::Integer(2, Span::new(1, 2)))),
                operator: BinaryOperator::Add,
                right: Box::new(Expression::Literal(Literal::Integer(3, Span::new(5, 6)))),
                span: Span::new(1, 6),
            })),
            operator: BinaryOperator::Multiply,
            right: Box::new(Expression::Literal(Literal::Integer(4, Span::new(10, 11)))),
            span: Span::new(0, 11),
        });
        
        let result = evaluator.evaluate(&expr).unwrap();
        assert_eq!(result, Value::Integer(20));
    }
}