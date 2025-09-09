//! Tests for semantic analysis edge cases

#[cfg(test)]
mod tests {
    use crate::analyze_program;
    use shared_types::{Program, Item, Function, Parameter, Block, Statement, Expression, Type};
    use shared_types::ast::{Literal, Identifier, BinaryExpression, BinaryOperator};
    use shared_types::{Span, ReturnStatement, LetStatement};
    
    fn create_test_span() -> Span {
        Span::new(0, 10)
    }
    
    /// Test semantic analysis of simple function
    #[test]
    fn test_simple_function_analysis() {
        // Create AST for: fn test() -> int { return 42; }
        let return_stmt = Statement::Return(ReturnStatement {
            value: Some(Expression::Literal(Literal::Integer(42, create_test_span()))),
            span: create_test_span(),
        });
        
        let function = Function {
            name: "test".to_string(),
            parameters: vec![],
            return_type: Some(Type::Int),
            body: Block {
                statements: vec![return_stmt],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let program = Program {
            items: vec![Item::Function(function)],
            span: create_test_span(),
        };
        
        let result = analyze_program(&program);
        assert!(result.is_ok(), "Simple function should analyze successfully");
    }
    
    /// Test semantic analysis with variables
    #[test]
    fn test_variable_analysis() {
        // Create AST for: fn test() -> int { let x = 42; return x; }
        let let_stmt = Statement::Let(LetStatement {
            name: "x".to_string(),
            mutable: false,
            var_type: Some(Type::Int),
            initializer: Some(Expression::Literal(Literal::Integer(42, create_test_span()))),
            span: create_test_span(),
        });
        
        let return_stmt = Statement::Return(ReturnStatement {
            value: Some(Expression::Identifier(Identifier {
                name: "x".to_string(),
                span: create_test_span(),
            })),
            span: create_test_span(),
        });
        
        let function = Function {
            name: "test".to_string(),
            parameters: vec![],
            return_type: Some(Type::Int),
            body: Block {
                statements: vec![let_stmt, return_stmt],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let program = Program {
            items: vec![Item::Function(function)],
            span: create_test_span(),
        };
        
        let result = analyze_program(&program);
        assert!(result.is_ok(), "Function with variable should analyze successfully");
    }
    
    /// Test semantic analysis with function parameters
    #[test]
    fn test_function_parameters() {
        // Create AST for: fn add(x: int, y: int) -> int { return x + y; }
        let params = vec![
            Parameter {
                name: "x".to_string(),
                param_type: Type::Int,
                span: create_test_span(),
            },
            Parameter {
                name: "y".to_string(),
                param_type: Type::Int,
                span: create_test_span(),
            },
        ];
        
        let add_expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Identifier(Identifier {
                name: "x".to_string(),
                span: create_test_span(),
            })),
            operator: BinaryOperator::Add,
            right: Box::new(Expression::Identifier(Identifier {
                name: "y".to_string(),
                span: create_test_span(),
            })),
            span: create_test_span(),
        });
        
        let return_stmt = Statement::Return(ReturnStatement {
            value: Some(add_expr),
            span: create_test_span(),
        });
        
        let function = Function {
            name: "add".to_string(),
            parameters: params,
            return_type: Some(Type::Int),
            body: Block {
                statements: vec![return_stmt],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let program = Program {
            items: vec![Item::Function(function)],
            span: create_test_span(),
        };
        
        let result = analyze_program(&program);
        assert!(result.is_ok(), "Function with parameters should analyze successfully");
    }
    
    /// Test semantic analysis with multiple functions
    #[test]
    fn test_multiple_functions() {
        // Create two simple functions
        let func1 = Function {
            name: "func1".to_string(),
            parameters: vec![],
            return_type: Some(Type::Int),
            body: Block {
                statements: vec![Statement::Return(ReturnStatement {
                    value: Some(Expression::Literal(Literal::Integer(1, create_test_span()))),
                    span: create_test_span(),
                })],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let func2 = Function {
            name: "func2".to_string(),
            parameters: vec![],
            return_type: Some(Type::Int),
            body: Block {
                statements: vec![Statement::Return(ReturnStatement {
                    value: Some(Expression::Literal(Literal::Integer(2, create_test_span()))),
                    span: create_test_span(),
                })],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let program = Program {
            items: vec![Item::Function(func1), Item::Function(func2)],
            span: create_test_span(),
        };
        
        let result = analyze_program(&program);
        assert!(result.is_ok(), "Multiple functions should analyze successfully");
    }
    
    /// Test semantic analysis with boolean expressions
    #[test]
    fn test_boolean_expressions() {
        // Create AST for: fn test() -> bool { return true && false; }
        let bool_expr = Expression::Binary(BinaryExpression {
            left: Box::new(Expression::Literal(Literal::Boolean(true, create_test_span()))),
            operator: BinaryOperator::LogicalAnd,
            right: Box::new(Expression::Literal(Literal::Boolean(false, create_test_span()))),
            span: create_test_span(),
        });
        
        let return_stmt = Statement::Return(ReturnStatement {
            value: Some(bool_expr),
            span: create_test_span(),
        });
        
        let function = Function {
            name: "test".to_string(),
            parameters: vec![],
            return_type: Some(Type::Bool),
            body: Block {
                statements: vec![return_stmt],
                span: create_test_span(),
            },
            span: create_test_span(),
        };
        
        let program = Program {
            items: vec![Item::Function(function)],
            span: create_test_span(),
        };
        
        let result = analyze_program(&program);
        assert!(result.is_ok(), "Function with boolean expressions should analyze successfully");
    }
}