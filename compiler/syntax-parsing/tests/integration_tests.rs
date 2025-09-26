//! Integration tests for the full parsing and evaluation pipeline

use syntax_parsing::{Parser, Evaluator};
use lexical_analysis::Lexer;
use shared_types::{Value, Item, Statement};

/// Helper function to evaluate a NEURO expression from source code
fn eval_expression(source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    // Tokenize
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    
    // Parse
    let mut parser = Parser::new(tokens);
    let program = parser.parse()?;
    
    // Extract the expression (assume it's in a function body)
    if let Some(Item::Function(func)) = program.items.first() {
        if let Some(Statement::Expression(expr)) = func.body.statements.first() {
            let mut evaluator = Evaluator::new();
            return Ok(evaluator.evaluate(expr)?);
        }
    }
    
    Err("No expression found in program".into())
}

/// Helper function to evaluate a NEURO expression directly
fn eval_expr_direct(expr_source: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let full_source = format!("fn test() {{ {}; }}", expr_source);
    eval_expression(&full_source)
}

#[test]
fn test_simple_arithmetic() {
    let result = eval_expr_direct("2 + 3").unwrap();
    assert_eq!(result, Value::Integer(5));
}

#[test]
fn test_operator_precedence() {
    let result = eval_expr_direct("2 + 3 * 4").unwrap();
    assert_eq!(result, Value::Integer(14)); // 2 + (3 * 4)
}

#[test]
fn test_parentheses_precedence() {
    let result = eval_expr_direct("(2 + 3) * 4").unwrap();
    assert_eq!(result, Value::Integer(20)); // (2 + 3) * 4
}

#[test]
fn test_floating_point_arithmetic() {
    let result = eval_expr_direct("3.5 + 2.1").unwrap();
    match result {
        Value::Float(f) => assert!((f - 5.6).abs() < f64::EPSILON),
        _ => panic!("Expected float result"),
    }
}

#[test]
fn test_mixed_arithmetic() {
    let result = eval_expr_direct("3 + 2.5").unwrap();
    match result {
        Value::Float(f) => assert!((f - 5.5).abs() < f64::EPSILON),
        _ => panic!("Expected float result"),
    }
}

#[test]
fn test_string_concatenation() {
    let result = eval_expr_direct(r#""Hello" + " World""#).unwrap();
    assert_eq!(result, Value::String("Hello World".to_string()));
}

#[test]
fn test_comparison_operations() {
    let test_cases = vec![
        ("5 > 3", Value::Boolean(true)),
        ("2 < 8", Value::Boolean(true)),
        ("4 == 4", Value::Boolean(true)),
        ("4 != 5", Value::Boolean(true)),
        ("10 >= 10", Value::Boolean(true)),
        ("7 <= 6", Value::Boolean(false)),
    ];
    
    for (expr, expected) in test_cases {
        let result = eval_expr_direct(expr).unwrap();
        assert_eq!(result, expected, "Failed for expression: {}", expr);
    }
}

#[test]
fn test_boolean_literals() {
    let result_true = eval_expr_direct("true").unwrap();
    assert_eq!(result_true, Value::Boolean(true));
    
    let result_false = eval_expr_direct("false").unwrap();
    assert_eq!(result_false, Value::Boolean(false));
}

#[test]
fn test_unary_minus() {
    let result = eval_expr_direct("-42").unwrap();
    assert_eq!(result, Value::Integer(-42));
    
    let result_float = eval_expr_direct("-3.14").unwrap();
    match result_float {
        Value::Float(f) => assert!((f - (-3.14)).abs() < f64::EPSILON),
        _ => panic!("Expected float result"),
    }
}

#[test]
fn test_complex_expression() {
    // Test: (10 + 5) * 2 - 8 / 4
    let result = eval_expr_direct("(10 + 5) * 2 - 8 / 4").unwrap();
    match result {
        Value::Float(f) => assert!((f - 28.0).abs() < f64::EPSILON), // 15 * 2 - 2.0 = 28.0
        _ => panic!("Expected float result due to division"),
    }
}

#[test]
fn test_division_by_zero() {
    let result = eval_expr_direct("5 / 0");
    assert!(result.is_err(), "Division by zero should fail");
}

#[test]
fn test_modulo_operation() {
    let result = eval_expr_direct("17 % 5").unwrap();
    assert_eq!(result, Value::Integer(2));
}

#[test]
fn test_string_comparison() {
    let result = eval_expr_direct(r#""apple" < "banana""#).unwrap();
    assert_eq!(result, Value::Boolean(true));
}

#[test]
fn test_full_program_with_function() {
    let source = r#"
        fn calculate() {
            return 2 * (3 + 4);
        }
    "#;
    
    // For now, we can't evaluate function calls, so let's just test parsing
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    
    assert_eq!(program.items.len(), 1);
    match &program.items[0] {
        Item::Function(func) => {
            assert_eq!(func.name, "calculate");
            assert_eq!(func.body.statements.len(), 1);
        }
        _ => panic!("Expected function"),
    }
}

#[test]
fn test_error_handling() {
    // Test various error conditions
    
    // Invalid syntax
    let result = eval_expr_direct("2 +");
    assert!(result.is_err());
    
    // Invalid operation
    let result = eval_expr_direct(r#""hello" - "world""#);
    assert!(result.is_err());
}

#[test]
fn test_evaluation_with_variables() {
    // This test shows how we'd use the evaluator with variables
    let source = "fn test() { x + 5; }";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap();
    
    if let Some(Item::Function(func)) = program.items.first() {
        if let Some(Statement::Expression(expr)) = func.body.statements.first() {
            let mut evaluator = Evaluator::new();
            
            // Define variable
            evaluator.define("x".to_string(), Value::Integer(10));
            
            // Evaluate expression
            let result = evaluator.evaluate(expr).unwrap();
            assert_eq!(result, Value::Integer(15));
        }
    }
}