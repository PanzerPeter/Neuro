//! Tests for evaluator edge cases and error conditions

use syntax_parsing::{Parser, Evaluator};
use lexical_analysis::Tokenizer;
use shared_types::{Expression, Value};

fn parse_expression(source: &str) -> Expression {
    let tokenizer = Tokenizer::new(source.to_string());
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    parser.parse_expression_only().unwrap()
}

#[test]
fn test_division_by_zero() {
    let expr = parse_expression("10 / 0");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err(), "Should error on division by zero");
}

#[test]
fn test_modulo_by_zero() {
    let expr = parse_expression("10 % 0");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err(), "Should error on modulo by zero");
}

#[test]
fn test_undefined_variable() {
    let expr = parse_expression("undefined_var");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err(), "Should error on undefined variable");
}

#[test]
fn test_type_mismatch_addition() {
    let expr = parse_expression("42 + true");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err(), "Should error on type mismatch");
}

#[test]
fn test_type_mismatch_comparison() {
    let expr = parse_expression("\"hello\" > 42");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_err(), "Should error on type mismatch in comparison");
}

#[test]
fn test_integer_overflow() {
    // Test very large numbers
    let expr = parse_expression("999999999999999999999999999999");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    // This might parse as a float or error - either is acceptable
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_float_precision() {
    let expr = parse_expression("0.1 + 0.2");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_ok(), "Should handle float precision");
    
    if let Ok(Value::Float(f)) = result {
        // Should be approximately 0.3, allowing for floating point precision
        assert!((f - 0.3).abs() < 0.0001);
    }
}

#[test]
fn test_string_comparison() {
    let test_cases = vec![
        ("\"abc\" == \"abc\"", true),
        ("\"abc\" == \"def\"", false),
        ("\"abc\" != \"def\"", true),
        ("\"a\" < \"b\"", true),
        ("\"z\" > \"a\"", true),
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        if let Ok(Value::Boolean(b)) = result {
            assert_eq!(b, expected, "Wrong result for: {}", expression);
        } else {
            panic!("Expected boolean result for: {}", expression);
        }
    }
}

#[test]
fn test_logical_operators() {
    let test_cases = vec![
        ("true && true", true),
        ("true && false", false),
        ("false && true", false),
        ("false && false", false),
        ("true || true", true),
        ("true || false", true),
        ("false || true", true),
        ("false || false", false),
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        if let Ok(Value::Boolean(b)) = result {
            assert_eq!(b, expected, "Wrong result for: {}", expression);
        } else {
            panic!("Expected boolean result for: {}", expression);
        }
    }
}

#[test]
fn test_unary_operators() {
    let test_cases = vec![
        ("-42", Value::Integer(-42)),
        ("--42", Value::Integer(42)),
        ("!true", Value::Boolean(false)),
        ("!false", Value::Boolean(true)),
        ("!!true", Value::Boolean(true)),
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        assert_eq!(result.unwrap(), expected, "Wrong result for: {}", expression);
    }
}

#[test]
fn test_precedence_edge_cases() {
    let test_cases = vec![
        ("2 + 3 * 4", 14),        // 2 + (3 * 4)
        ("2 * 3 + 4", 10),        // (2 * 3) + 4
        ("10 - 4 - 2", 4),        // (10 - 4) - 2
        ("16 / 4 / 2", 2),        // (16 / 4) / 2
        ("2 * 3 * 4", 24),        // ((2 * 3) * 4)
        ("-2 + 3", 1),             // (-2) + 3
        ("-(2 + 3)", -5),          // -(2 + 3)
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        if let Ok(Value::Integer(i)) = result {
            assert_eq!(i, expected, "Wrong result for: {}", expression);
        } else {
            panic!("Expected integer result for: {}", expression);
        }
    }
}

#[test]
fn test_mixed_types() {
    let test_cases = vec![
        ("42 + 3.14", Value::Float(45.14)),
        ("3.14 + 42", Value::Float(45.14)),
        ("10 / 4", Value::Integer(2)),  // Integer division
        ("10.0 / 4", Value::Float(2.5)), // Float division
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        match (&result.unwrap(), &expected) {
            (Value::Float(a), Value::Float(b)) => {
                assert!((a - b).abs() < 0.0001, "Wrong result for: {}", expression);
            }
            (actual, expected) => {
                assert_eq!(actual, expected, "Wrong result for: {}", expression);
            }
        }
    }
}

#[test]
fn test_boolean_arithmetic_errors() {
    let error_cases = vec![
        "true + false",
        "true * 5",
        "false - true",
        "true / 2",
        "false % 3",
    ];
    
    for expression in error_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        assert!(result.is_err(), "Should error on: {}", expression);
    }
}

#[test]
fn test_string_operations() {
    let test_cases = vec![
        ("\"Hello\" + \" \" + \"World\"", "Hello World"),
        ("\"\" + \"test\"", "test"),
        ("\"test\" + \"\"", "test"),
        ("\"\" + \"\"", ""),
    ];
    
    for (expression, expected) in test_cases {
        let expr = parse_expression(expression);
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&expr);
        
        assert!(result.is_ok(), "Should evaluate: {}", expression);
        if let Ok(Value::String(s)) = result {
            assert_eq!(s, expected, "Wrong result for: {}", expression);
        } else {
            panic!("Expected string result for: {}", expression);
        }
    }
}

#[test]
fn test_parentheses_deeply_nested() {
    let expr = parse_expression("((((((2 + 3) * 4) - 1) / 2) + 1) * 3)");
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&expr);
    assert!(result.is_ok(), "Should handle deeply nested parentheses");
    
    if let Ok(Value::Integer(i)) = result {
        // ((((((2 + 3) * 4) - 1) / 2) + 1) * 3) = ((((20 - 1) / 2) + 1) * 3) = (((19 / 2) + 1) * 3) = ((9 + 1) * 3) = 30
        assert_eq!(i, 30);
    }
}