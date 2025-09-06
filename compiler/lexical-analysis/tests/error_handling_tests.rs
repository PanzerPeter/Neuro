//! Tests for lexical analysis error handling

use lexical_analysis::Tokenizer;
use shared_types::TokenType;

#[test]
fn test_unterminated_string() {
    let source = "\"unterminated string".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_err(), "Should fail on unterminated string");
}

#[test]
fn test_unterminated_block_comment() {
    let source = "/* unterminated comment".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_err(), "Should fail on unterminated block comment");
}

#[test]
fn test_invalid_escape_sequence() {
    let source = "\"invalid \\q escape\"".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_err(), "Should fail on invalid escape sequence");
}

#[test]
fn test_unexpected_character() {
    let source = "let x = @invalid".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should tokenize with Unknown token for unexpected character");
    
    let tokens = result.unwrap();
    // Should have an Unknown token for the @ character
    assert!(tokens.iter().any(|t| matches!(t.token_type, TokenType::Unknown(_))));
}

#[test]
fn test_very_long_identifier() {
    let source = "a".repeat(1000);
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle very long identifiers");
    
    let tokens = result.unwrap();
    assert_eq!(tokens.len(), 2); // identifier + EOF
    match &tokens[0].token_type {
        TokenType::Identifier(name) => assert_eq!(name.len(), 1000),
        _ => panic!("Expected identifier token"),
    }
}

#[test]
fn test_very_long_string() {
    let long_content = "x".repeat(1000);
    let source = format!("\"{}\"", long_content);
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle very long strings");
}

#[test]
fn test_empty_source() {
    let source = "".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle empty source");
    
    let tokens = result.unwrap();
    assert_eq!(tokens.len(), 1); // Just EOF
    assert_eq!(tokens[0].token_type, TokenType::EndOfFile);
}

#[test]
fn test_only_whitespace() {
    let source = "   \t\n   \r\n   ".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle whitespace-only source");
}

#[test]
fn test_only_comments() {
    let source = "// comment 1\n/* comment 2 */\n// comment 3".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle comment-only source");
}

#[test]
fn test_mixed_line_endings() {
    let source = "let\r\nx\n=\r\n42".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle mixed line endings");
}

#[test]
fn test_unicode_in_string() {
    let source = "\"Hello 世界 🌍\"".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle unicode in strings");
}

#[test]
fn test_unicode_in_identifier() {
    let source = "let variable_名前 = 42".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle unicode in identifiers");
}

#[test]
fn test_nested_block_comments() {
    let source = "/* outer /* inner */ outer */".to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle nested block comments");
}

#[test]
fn test_string_with_common_escapes() {
    let source = r#""String with \n\t\r\\\" escapes""#.to_string();
    let tokenizer = Tokenizer::new(source);
    let result = tokenizer.tokenize();
    assert!(result.is_ok(), "Should handle common escape sequences");
}

#[test]
fn test_numbers_edge_cases() {
    let test_cases = vec![
        "0",
        "00000",
        "123456789",
        "0.0",
        "0.123",
        "123.456",
        ".5", // Should this be valid?
        "5.",
    ];
    
    for case in test_cases {
        let tokenizer = Tokenizer::new(case.to_string());
        let result = tokenizer.tokenize();
        assert!(result.is_ok() || case == ".5", "Should handle number: {}", case);
    }
}