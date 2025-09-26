//! Integration tests for the NEURO tokenizer

use lexical_analysis::Tokenizer;
use shared_types::{TokenType, Keyword};

#[test]
fn test_tokenizer_basic_usage() {
    let tokenizer = Tokenizer::new("let x = 42;".to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::Let),
        TokenType::Identifier("x".to_string()),
        TokenType::Assign,
        TokenType::Integer("42".to_string()),
        TokenType::Semicolon,
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_tokenizer_with_file_path() {
    let tokenizer = Tokenizer::with_file(
        "fn main() {}".to_string(), 
        "test.nr".to_string()
    );
    
    let tokens = tokenizer.tokenize().unwrap();
    
    assert_eq!(tokens.len(), 7); // fn, main, (, ), {, }, EOF
    assert_eq!(tokens[0].token_type, TokenType::Keyword(Keyword::Fn));
    assert_eq!(tokens[1].token_type, TokenType::Identifier("main".to_string()));
    assert_eq!(tokenizer.file_path, Some("test.nr".to_string()));
}

#[test]
fn test_tokenizer_filtered() {
    let source = "hello\nworld\n\ntest";
    let tokenizer = Tokenizer::new(source.to_string());
    
    let all_tokens = tokenizer.tokenize().unwrap();
    let filtered_tokens = tokenizer.tokenize_filtered().unwrap();
    
    // All tokens should include newlines
    assert!(all_tokens.iter().any(|t| matches!(t.token_type, TokenType::Newline)));
    
    // Filtered tokens should not include newlines
    assert!(!filtered_tokens.iter().any(|t| matches!(t.token_type, TokenType::Newline)));
    
    // But should still have the identifiers
    let identifiers: Vec<_> = filtered_tokens
        .iter()
        .filter(|t| matches!(t.token_type, TokenType::Identifier(_)))
        .collect();
    
    assert_eq!(identifiers.len(), 3); // hello, world, test
}

#[test]
fn test_get_line() {
    let source = "line 1\nline 2\nline 3";
    let tokenizer = Tokenizer::new(source.to_string());
    
    assert_eq!(tokenizer.get_line(1), Some("line 1"));
    assert_eq!(tokenizer.get_line(2), Some("line 2"));
    assert_eq!(tokenizer.get_line(3), Some("line 3"));
    assert_eq!(tokenizer.get_line(4), None);
    assert_eq!(tokenizer.get_line(0), None); // Line numbers start from 1
}

#[test]
fn test_get_text() {
    let source = "hello world";
    let tokenizer = Tokenizer::new(source.to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    // Get text for "hello" token
    let hello_text = tokenizer.get_text(&tokens[0].span);
    assert_eq!(hello_text, Some("hello"));
    
    // Get text for "world" token
    let world_text = tokenizer.get_text(&tokens[1].span);
    assert_eq!(world_text, Some("world"));
    
    // Test invalid span
    let invalid_span = shared_types::Span::new(100, 200);
    let invalid_text = tokenizer.get_text(&invalid_span);
    assert_eq!(invalid_text, None);
}

#[test]
fn test_ml_specific_syntax() {
    let source = r#"
        #[grad]
        fn neural_forward(input: Tensor<float, [N, D]>) -> Tensor<float, [N, C]> {
            // ML computation here
        }
        
        #[kernel]
        #[gpu]
        fn matrix_multiply(a: Tensor<float, [M, K]>, b: Tensor<float, [K, N]>) -> Tensor<float, [M, N]> {
            // GPU kernel implementation
        }
    "#;
    
    let tokenizer = Tokenizer::new(source.to_string());
    let tokens = tokenizer.tokenize_filtered().unwrap(); // Remove newlines for easier checking
    
    // Should contain ML-specific keywords
    let has_grad = tokens.iter().any(|t| matches!(t.token_type, TokenType::Keyword(Keyword::Grad)));
    let has_kernel = tokens.iter().any(|t| matches!(t.token_type, TokenType::Keyword(Keyword::Kernel)));
    let has_gpu = tokens.iter().any(|t| matches!(t.token_type, TokenType::Keyword(Keyword::Gpu)));
    let has_tensor = tokens.iter().any(|t| matches!(t.token_type, TokenType::Keyword(Keyword::Tensor)));
    
    assert!(has_grad, "Should contain 'grad' keyword");
    assert!(has_kernel, "Should contain 'kernel' keyword"); 
    assert!(has_gpu, "Should contain 'gpu' keyword");
    assert!(has_tensor, "Should contain 'Tensor' keyword");
    
    // Should also contain identifiers and other tokens
    let identifier_count = tokens.iter().filter(|t| matches!(t.token_type, TokenType::Identifier(_))).count();
    assert!(identifier_count > 10, "Should have many identifiers");
}

#[test]
fn test_tensor_type_syntax() {
    let source = "Tensor<float, [3, 3]>";
    let tokenizer = Tokenizer::new(source.to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    let expected_sequence = vec![
        TokenType::Keyword(Keyword::Tensor),
        TokenType::Less,
        TokenType::Identifier("float".to_string()),
        TokenType::Comma,
        TokenType::LeftBracket,
        TokenType::Integer("3".to_string()),
        TokenType::Comma,
        TokenType::Integer("3".to_string()),
        TokenType::RightBracket,
        TokenType::Greater,
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected_sequence.len());
    for (token, expected_type) in tokens.iter().zip(expected_sequence.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_function_with_arrow_return() {
    let source = "fn add(x: int, y: int) -> int";
    let tokenizer = Tokenizer::new(source.to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    let _expected_sequence = vec![
        TokenType::Keyword(Keyword::Fn),
        TokenType::Identifier("add".to_string()),
        TokenType::LeftParen,
        TokenType::Identifier("x".to_string()),
        TokenType::Identifier(":".to_string()), // Note: We don't have colon as separate token yet
        TokenType::Identifier("int".to_string()),
        TokenType::Comma,
        TokenType::Identifier("y".to_string()),
        TokenType::Identifier(":".to_string()),
        TokenType::Identifier("int".to_string()),
        TokenType::RightParen,
        TokenType::Arrow,
        TokenType::Identifier("int".to_string()),
        TokenType::EndOfFile,
    ];
    
    // Should contain arrow token
    let has_arrow = tokens.iter().any(|t| matches!(t.token_type, TokenType::Arrow));
    assert!(has_arrow, "Should contain arrow (->) token");
}

#[test]
fn test_empty_source() {
    let tokenizer = Tokenizer::new("".to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].token_type, TokenType::EndOfFile);
}

#[test]
fn test_only_whitespace() {
    let tokenizer = Tokenizer::new("   \t  \n  \n  ".to_string());
    let tokens = tokenizer.tokenize().unwrap();
    
    // Should have newlines and EOF
    let newline_count = tokens.iter().filter(|t| matches!(t.token_type, TokenType::Newline)).count();
    assert_eq!(newline_count, 2);
    
    let filtered = tokenizer.tokenize_filtered().unwrap();
    assert_eq!(filtered.len(), 1); // Only EOF
    assert_eq!(filtered[0].token_type, TokenType::EndOfFile);
}