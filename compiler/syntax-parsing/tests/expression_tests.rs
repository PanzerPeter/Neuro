// NEURO Programming Language - Syntax Parsing Tests
// Expression parsing tests

use shared_types::Literal;
use syntax_parsing::{parse_expr, BinaryOp, Expr, UnaryOp};

#[test]
fn test_parse_integer_literal() {
    let result = parse_expr("42");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Literal(Literal::Integer(n), _) => assert_eq!(n, 42),
        _ => panic!("Expected integer literal, got {:?}", expr),
    }
}

#[test]
fn test_parse_float_literal() {
    let result = parse_expr("2.5");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Literal(Literal::Float(f), _) => assert!((f - 2.5).abs() < 0.0001),
        _ => panic!("Expected float literal, got {:?}", expr),
    }
}

#[test]
fn test_parse_string_literal() {
    let result = parse_expr("\"hello world\"");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Literal(Literal::String(s), _) => assert_eq!(s, "hello world"),
        _ => panic!("Expected string literal, got {:?}", expr),
    }
}

#[test]
fn test_parse_boolean_true() {
    let result = parse_expr("true");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Literal(Literal::Boolean(b), _) => assert!(b),
        _ => panic!("Expected true literal, got {:?}", expr),
    }
}

#[test]
fn test_parse_boolean_false() {
    let result = parse_expr("false");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Literal(Literal::Boolean(b), _) => assert!(!b),
        _ => panic!("Expected false literal, got {:?}", expr),
    }
}

#[test]
fn test_parse_identifier() {
    let result = parse_expr("my_variable");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Identifier(ident) => assert_eq!(ident.name, "my_variable"),
        _ => panic!("Expected identifier, got {:?}", expr),
    }
}

#[test]
fn test_parse_addition() {
    let result = parse_expr("2 + 3");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Add),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_subtraction() {
    let result = parse_expr("10 - 5");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Subtract),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_multiplication() {
    let result = parse_expr("4 * 6");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Multiply),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_division() {
    let result = parse_expr("20 / 4");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Divide),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_modulo() {
    let result = parse_expr("10 % 3");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Modulo),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_equality() {
    let result = parse_expr("x == y");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Equal),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_not_equal() {
    let result = parse_expr("x != y");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::NotEqual),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_less_than() {
    let result = parse_expr("a < b");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Less),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_greater_than() {
    let result = parse_expr("a > b");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Greater),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_less_equal() {
    let result = parse_expr("a <= b");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::LessEqual),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_greater_equal() {
    let result = parse_expr("a >= b");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::GreaterEqual),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_logical_and() {
    let result = parse_expr("true && false");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::And),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_logical_or() {
    let result = parse_expr("true || false");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Or),
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_unary_negate() {
    let result = parse_expr("-42");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Unary { op, .. } => assert_eq!(op, UnaryOp::Negate),
        _ => panic!("Expected unary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_unary_not() {
    let result = parse_expr("!true");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Unary { op, .. } => assert_eq!(op, UnaryOp::Not),
        _ => panic!("Expected unary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_parenthesized_expression() {
    let result = parse_expr("(42)");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Paren(inner, _) => match *inner {
            Expr::Literal(Literal::Integer(n), _) => assert_eq!(n, 42),
            _ => panic!("Expected integer literal inside parens"),
        },
        _ => panic!("Expected parenthesized expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_complex_parenthesized() {
    let result = parse_expr("(2 + 3) * 4");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => {
            assert_eq!(op, BinaryOp::Multiply);
            assert!(matches!(*left, Expr::Paren(_, _)));
            assert!(matches!(*right, Expr::Literal(Literal::Integer(4), _)));
        }
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_parse_function_call_no_args() {
    let result = parse_expr("foo()");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Call { func, args, .. } => {
            assert!(matches!(*func, Expr::Identifier(_)));
            assert_eq!(args.len(), 0);
        }
        _ => panic!("Expected function call, got {:?}", expr),
    }
}

#[test]
fn test_parse_function_call_one_arg() {
    let result = parse_expr("foo(42)");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Call { func, args, .. } => {
            assert!(matches!(*func, Expr::Identifier(_)));
            assert_eq!(args.len(), 1);
        }
        _ => panic!("Expected function call, got {:?}", expr),
    }
}

#[test]
fn test_parse_function_call_multiple_args() {
    let result = parse_expr("add(1, 2, 3)");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Call { func, args, .. } => {
            assert!(matches!(*func, Expr::Identifier(_)));
            assert_eq!(args.len(), 3);
        }
        _ => panic!("Expected function call, got {:?}", expr),
    }
}

#[test]
fn test_parse_nested_function_calls() {
    let result = parse_expr("outer(inner(42))");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Call { args, .. } => {
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0], Expr::Call { .. }));
        }
        _ => panic!("Expected function call, got {:?}", expr),
    }
}

#[test]
fn test_operator_precedence_mul_over_add() {
    let result = parse_expr("2 + 3 * 4");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => {
            assert_eq!(op, BinaryOp::Add);
            assert!(matches!(*left, Expr::Literal(Literal::Integer(2), _)));
            match *right {
                Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Multiply),
                _ => panic!("Expected multiplication on right side"),
            }
        }
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_operator_precedence_div_over_sub() {
    let result = parse_expr("10 - 6 / 2");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => {
            assert_eq!(op, BinaryOp::Subtract);
            assert!(matches!(*left, Expr::Literal(Literal::Integer(10), _)));
            match *right {
                Expr::Binary { op, .. } => assert_eq!(op, BinaryOp::Divide),
                _ => panic!("Expected division on right side"),
            }
        }
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_operator_precedence_comparison_over_logical() {
    let result = parse_expr("a < b && c > d");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => {
            assert_eq!(op, BinaryOp::And);
            assert!(matches!(*left, Expr::Binary { .. }));
            assert!(matches!(*right, Expr::Binary { .. }));
        }
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_operator_associativity_left() {
    let result = parse_expr("1 + 2 + 3");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Binary {
            op, left, right, ..
        } => {
            assert_eq!(op, BinaryOp::Add);
            assert!(matches!(*left, Expr::Binary { .. }));
            assert!(matches!(*right, Expr::Literal(Literal::Integer(3), _)));
        }
        _ => panic!("Expected binary expression, got {:?}", expr),
    }
}

#[test]
fn test_complex_expression() {
    let result = parse_expr("(-a + b * c) / (d - e)");
    assert!(result.is_ok());
}

#[test]
fn test_expression_with_newlines() {
    let result = parse_expr("1 +\n2 *\n3");
    assert!(result.is_ok());
}

#[test]
fn test_function_call_with_complex_args() {
    let result = parse_expr("foo(a + b, c * d, e)");
    assert!(result.is_ok());
    let expr = result.unwrap();
    match expr {
        Expr::Call { args, .. } => assert_eq!(args.len(), 3),
        _ => panic!("Expected function call"),
    }
}
