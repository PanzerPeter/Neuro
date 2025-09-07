//! Tests for expression-only parsing functionality

use syntax_parsing::Parser;
use lexical_analysis::Tokenizer;
use shared_types::{Expression, Value, ast::Literal};

#[test]
fn test_parse_simple_arithmetic() {
    let source = "2 + 3 * 4".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse arithmetic expression");
}

#[test]
fn test_parse_parenthesized_expression() {
    let source = "(2 + 3) * 4".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse parenthesized expression");
}

#[test]
fn test_parse_comparison() {
    let source = "42 == 42".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse comparison expression");
}

#[test]
fn test_parse_string_concatenation() {
    let source = "\"Hello\" + \" World\"".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse string concatenation");
}

#[test]
fn test_parse_with_semicolon() {
    let source = "2 + 3;".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse expression with semicolon");
}

#[test]
fn test_parse_with_newlines() {
    let source = "\n  2 + 3  \n".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse expression with newlines");
}

#[test]
fn test_parse_expression_extra_tokens() {
    let source = "2 + 3 let x = 5".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_err(), "Should fail with extra tokens");
}

#[test]
fn test_parse_integer_literal() {
    let source = "42".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse integer literal");
}

#[test]
fn test_parse_float_literal() {
    let source = "3.14159".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse float literal");
}

#[test]
fn test_parse_boolean_literal() {
    let source = "true".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse boolean literal");
}

#[test]
fn test_parse_unary_minus() {
    let source = "-42".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse unary minus");
}

#[test]
#[ignore = "Uses unary operators and logical operators not yet implemented"]
fn test_parse_complex_expression() {
    let source = "-(2 + 3) * 4 > 10 && true".to_string();
    let tokenizer = Tokenizer::new(source);
    let tokens = tokenizer.tokenize_filtered().unwrap();
    let mut parser = Parser::new(tokens);
    
    let result = parser.parse_expression_only();
    assert!(result.is_ok(), "Should parse complex expression");
}