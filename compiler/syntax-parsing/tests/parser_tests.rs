//! Tests for the NEURO parser

use syntax_parsing::{Parser, ParseError};
use lexical_analysis::Lexer;
use shared_types::{
    Program, Item, Statement, Expression, 
    ast::{Literal, BinaryExpression, BinaryOperator},
    TokenType, Keyword,
};

/// Helper function to parse a string into an AST
fn parse_string(input: &str) -> Result<Program, ParseError> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[test]
fn test_parse_empty_program() {
    let program = parse_string("").unwrap();
    assert!(program.items.is_empty());
}

#[test]
fn test_parse_simple_function() {
    let source = "fn test() {}";
    let program = parse_string(source).unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.name, "test");
            assert!(func.parameters.is_empty());
            assert!(func.return_type.is_none());
            assert!(func.body.statements.is_empty());
        }
        _ => panic!("Expected function item"),
    }
}

#[test]
fn test_parse_function_with_parameters() {
    let source = "fn add(a, b) {}";
    let program = parse_string(source).unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.name, "add");
            assert_eq!(func.parameters.len(), 2);
            assert_eq!(func.parameters[0].name, "a");
            assert_eq!(func.parameters[1].name, "b");
        }
        _ => panic!("Expected function item"),
    }
}

#[test]
fn test_parse_function_with_return_type() {
    let source = "fn get_value() -> int {}";
    let program = parse_string(source).unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.name, "get_value");
            assert!(func.return_type.is_some());
        }
        _ => panic!("Expected function item"),
    }
}

#[test]
fn test_parse_function_with_body() {
    let source = r#"
        fn test() {
            let x = 42;
            return x;
        }
    "#;
    let program = parse_string(source).unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.name, "test");
            assert_eq!(func.body.statements.len(), 2);
            
            // Check let statement
            match &func.body.statements[0] {
                Statement::Let(let_stmt) => {
                    assert_eq!(let_stmt.name, "x");
                    assert!(!let_stmt.mutable);
                    assert!(let_stmt.initializer.is_some());
                }
                _ => panic!("Expected let statement"),
            }
            
            // Check return statement
            match &func.body.statements[1] {
                Statement::Return(ret_stmt) => {
                    assert!(ret_stmt.value.is_some());
                }
                _ => panic!("Expected return statement"),
            }
        }
        _ => panic!("Expected function item"),
    }
}

#[test]
fn test_parse_import_statement() {
    let source = r#"import "math";"#;
    let program = parse_string(source).unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Import(import) => {
            assert_eq!(import.path, "math");
        }
        _ => panic!("Expected import item"),
    }
}

#[test]
fn test_parse_let_statement() {
    let source = "fn test() { let x = 10; }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Let(let_stmt) => {
                    assert_eq!(let_stmt.name, "x");
                    assert!(!let_stmt.mutable);
                    assert!(let_stmt.initializer.is_some());
                    
                    match let_stmt.initializer.as_ref().unwrap() {
                        Expression::Literal(Literal::Integer(val, _)) => {
                            assert_eq!(*val, 10);
                        }
                        _ => panic!("Expected integer literal"),
                    }
                }
                _ => panic!("Expected let statement"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_mutable_let_statement() {
    let source = "fn test() { let mut x = 5; }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Let(let_stmt) => {
                    assert_eq!(let_stmt.name, "x");
                    assert!(let_stmt.mutable);
                }
                _ => panic!("Expected let statement"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_binary_expressions() {
    let source = "fn test() { 1 + 2; }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Expression(Expression::Binary(bin_expr)) => {
                    assert_eq!(bin_expr.operator, BinaryOperator::Add);
                    
                    match (&*bin_expr.left, &*bin_expr.right) {
                        (Expression::Literal(Literal::Integer(1, _)), 
                         Expression::Literal(Literal::Integer(2, _))) => {
                            // Expected
                        }
                        _ => panic!("Expected integer literals"),
                    }
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_multiple_binary_operations() {
    let source = "fn test() { 1 + 2 * 3; }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Expression(Expression::Binary(bin_expr)) => {
                    // Should parse as 1 + (2 * 3) due to precedence
                    assert_eq!(bin_expr.operator, BinaryOperator::Add);
                    
                    match &*bin_expr.left {
                        Expression::Literal(Literal::Integer(1, _)) => {},
                        _ => panic!("Expected 1 on left side"),
                    }
                    
                    match &*bin_expr.right {
                        Expression::Binary(right_bin) => {
                            assert_eq!(right_bin.operator, BinaryOperator::Multiply);
                        }
                        _ => panic!("Expected binary expression on right side"),
                    }
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_parenthesized_expressions() {
    let source = "fn test() { (1 + 2) * 3; }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Expression(Expression::Binary(bin_expr)) => {
                    // Should parse as (1 + 2) * 3
                    assert_eq!(bin_expr.operator, BinaryOperator::Multiply);
                    
                    match &*bin_expr.left {
                        Expression::Binary(left_bin) => {
                            assert_eq!(left_bin.operator, BinaryOperator::Add);
                        }
                        _ => panic!("Expected binary expression on left side"),
                    }
                    
                    match &*bin_expr.right {
                        Expression::Literal(Literal::Integer(3, _)) => {},
                        _ => panic!("Expected 3 on right side"),
                    }
                }
                _ => panic!("Expected binary expression"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_function_call() {
    let source = "fn test() { print(); }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Expression(Expression::Call(call)) => {
                    match &*call.function {
                        Expression::Identifier(id) => {
                            assert_eq!(id.name, "print");
                        }
                        _ => panic!("Expected identifier"),
                    }
                    assert!(call.arguments.is_empty());
                }
                _ => panic!("Expected function call"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_function_call_with_args() {
    let source = "fn test() { add(1, 2); }";
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::Expression(Expression::Call(call)) => {
                    match &*call.function {
                        Expression::Identifier(id) => {
                            assert_eq!(id.name, "add");
                        }
                        _ => panic!("Expected identifier"),
                    }
                    assert_eq!(call.arguments.len(), 2);
                }
                _ => panic!("Expected function call"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_if_statement() {
    let source = r#"
        fn test() {
            if true {
                return 1;
            }
        }
    "#;
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::If(if_stmt) => {
                    match &if_stmt.condition {
                        Expression::Literal(Literal::Boolean(true, _)) => {},
                        _ => panic!("Expected boolean literal"),
                    }
                    assert_eq!(if_stmt.then_branch.statements.len(), 1);
                    assert!(if_stmt.else_branch.is_none());
                }
                _ => panic!("Expected if statement"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_if_else_statement() {
    let source = r#"
        fn test() {
            if false {
                return 1;
            } else {
                return 2;
            }
        }
    "#;
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::If(if_stmt) => {
                    assert!(if_stmt.else_branch.is_some());
                    assert_eq!(if_stmt.else_branch.as_ref().unwrap().statements.len(), 1);
                }
                _ => panic!("Expected if statement"),
            }
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_parse_while_statement() {
    let source = r#"
        fn test() {
            while true {
                break;
            }
        }
    "#;
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            match &func.body.statements[0] {
                Statement::While(while_stmt) => {
                    match &while_stmt.condition {
                        Expression::Literal(Literal::Boolean(true, _)) => {},
                        _ => panic!("Expected boolean literal"),
                    }
                    assert_eq!(while_stmt.body.statements.len(), 1);
                }
                _ => panic!("Expected while statement"),
            }
        }
        _ => panic!("Expected function"),
    }
}

// Error cases

#[test]
fn test_parse_error_unexpected_token() {
    let result = parse_string("123");
    assert!(result.is_err());
}

#[test]
fn test_parse_error_unclosed_function() {
    let result = parse_string("fn test() {");
    assert!(result.is_err());
}

#[test]
fn test_parse_error_missing_semicolon() {
    let result = parse_string("fn test() { let x = 5 }");
    assert!(result.is_err());
}

#[test]
fn test_parse_literals() {
    let source = r#"
        fn test() {
            42;
            3.14;
            "hello";
            true;
            false;
        }
    "#;
    let program = parse_string(source).unwrap();
    
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.body.statements.len(), 5);
            
            // Integer
            match &func.body.statements[0] {
                Statement::Expression(Expression::Literal(Literal::Integer(42, _))) => {},
                _ => panic!("Expected integer literal"),
            }
            
            // Float
            match &func.body.statements[1] {
                Statement::Expression(Expression::Literal(Literal::Float(val, _))) => {
                    assert!((val - 3.14).abs() < f64::EPSILON);
                },
                _ => panic!("Expected float literal"),
            }
            
            // String
            match &func.body.statements[2] {
                Statement::Expression(Expression::Literal(Literal::String(s, _))) => {
                    assert_eq!(s, "hello");
                },
                _ => panic!("Expected string literal"),
            }
            
            // Boolean true
            match &func.body.statements[3] {
                Statement::Expression(Expression::Literal(Literal::Boolean(true, _))) => {},
                _ => panic!("Expected boolean literal true"),
            }
            
            // Boolean false
            match &func.body.statements[4] {
                Statement::Expression(Expression::Literal(Literal::Boolean(false, _))) => {},
                _ => panic!("Expected boolean literal false"),
            }
        }
        _ => panic!("Expected function"),
    }
}