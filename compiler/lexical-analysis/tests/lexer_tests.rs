//! Comprehensive tests for the NEURO lexer

use lexical_analysis::{Lexer, LexError};
use shared_types::{TokenType, Keyword, Span};

#[test]
fn test_basic_tokens() {
    let mut lexer = Lexer::new("+ - * / % ( ) { } [ ] ; , .");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Plus,
        TokenType::Minus,
        TokenType::Star,
        TokenType::Slash,
        TokenType::Percent,
        TokenType::LeftParen,
        TokenType::RightParen,
        TokenType::LeftBrace,
        TokenType::RightBrace,
        TokenType::LeftBracket,
        TokenType::RightBracket,
        TokenType::Semicolon,
        TokenType::Comma,
        TokenType::Dot,
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_comparison_operators() {
    let mut lexer = Lexer::new("== != < <= > >=");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Equal,
        TokenType::NotEqual,
        TokenType::Less,
        TokenType::LessEqual,
        TokenType::Greater,
        TokenType::GreaterEqual,
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_arrow_operator() {
    let mut lexer = Lexer::new("-> -");
    let tokens = lexer.tokenize().unwrap();
    
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].token_type, TokenType::Arrow);
    assert_eq!(tokens[1].token_type, TokenType::Minus);
    assert_eq!(tokens[2].token_type, TokenType::EndOfFile);
}

#[test]
fn test_integers() {
    let mut lexer = Lexer::new("123 0 456789");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Integer("123".to_string()),
        TokenType::Integer("0".to_string()),
        TokenType::Integer("456789".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_floats() {
    let mut lexer = Lexer::new("123.45 0.0 3.14159");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Float("123.45".to_string()),
        TokenType::Float("0.0".to_string()),
        TokenType::Float("3.14159".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_strings() {
    let mut lexer = Lexer::new(r#""hello" "world with spaces" "empty" "#);
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::String("hello".to_string()),
        TokenType::String("world with spaces".to_string()),
        TokenType::String("empty".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_string_escapes() {
    let mut lexer = Lexer::new(r#""hello\nworld" "tab\there" "quote\"here""#);
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::String("hello\nworld".to_string()),
        TokenType::String("tab\there".to_string()),
        TokenType::String("quote\"here".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_control_flow_keywords() {
    let mut lexer = Lexer::new("if else while for break continue return");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::If),
        TokenType::Keyword(Keyword::Else),
        TokenType::Keyword(Keyword::While),
        TokenType::Keyword(Keyword::For),
        TokenType::Keyword(Keyword::Break),
        TokenType::Keyword(Keyword::Continue),
        TokenType::Keyword(Keyword::Return),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_type_keywords() {
    let mut lexer = Lexer::new("fn let mut const type struct enum");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::Fn),
        TokenType::Keyword(Keyword::Let),
        TokenType::Keyword(Keyword::Mut),
        TokenType::Keyword(Keyword::Const),
        TokenType::Keyword(Keyword::Type),
        TokenType::Keyword(Keyword::Struct),
        TokenType::Keyword(Keyword::Enum),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_ml_keywords() {
    let mut lexer = Lexer::new("Tensor grad kernel gpu");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::Tensor),
        TokenType::Keyword(Keyword::Grad),
        TokenType::Keyword(Keyword::Kernel),
        TokenType::Keyword(Keyword::Gpu),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_module_keywords() {
    let mut lexer = Lexer::new("import export module");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::Import),
        TokenType::Keyword(Keyword::Export),
        TokenType::Keyword(Keyword::Module),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_memory_keywords() {
    let mut lexer = Lexer::new("Arc Pool");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Keyword(Keyword::Arc),
        TokenType::Keyword(Keyword::Pool),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_booleans() {
    let mut lexer = Lexer::new("true false");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Boolean(true),
        TokenType::Boolean(false),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_identifiers() {
    let mut lexer = Lexer::new("variable_name _private myFunction");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Identifier("variable_name".to_string()),
        TokenType::Identifier("_private".to_string()),
        TokenType::Identifier("myFunction".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_line_comments() {
    let mut lexer = Lexer::new("hello // this is a comment\nworld");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Identifier("hello".to_string()),
        TokenType::Newline,
        TokenType::Identifier("world".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_block_comments() {
    let mut lexer = Lexer::new("hello /* this is a block comment */ world");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Identifier("hello".to_string()),
        TokenType::Identifier("world".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_newlines() {
    let mut lexer = Lexer::new("line1\nline2\n\nline4");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Identifier("line1".to_string()),
        TokenType::Newline,
        TokenType::Identifier("line2".to_string()),
        TokenType::Newline,
        TokenType::Newline,
        TokenType::Identifier("line4".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

#[test]
fn test_whitespace_handling() {
    let mut lexer = Lexer::new("   token1    token2\t\ttoken3   ");
    let tokens = lexer.tokenize().unwrap();
    
    let expected = vec![
        TokenType::Identifier("token1".to_string()),
        TokenType::Identifier("token2".to_string()),
        TokenType::Identifier("token3".to_string()),
        TokenType::EndOfFile,
    ];
    
    assert_eq!(tokens.len(), expected.len());
    for (token, expected_type) in tokens.iter().zip(expected.iter()) {
        assert_eq!(&token.token_type, expected_type);
    }
}

// Error cases

#[test]
fn test_unterminated_string() {
    let mut lexer = Lexer::new(r#""unterminated string"#);
    let result = lexer.tokenize();
    
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::UnterminatedString { .. } => {} // Expected
        other => panic!("Expected UnterminatedString error, got: {:?}", other),
    }
}

#[test]
fn test_invalid_escape_sequence() {
    let mut lexer = Lexer::new(r#""invalid \x escape""#);
    let result = lexer.tokenize();
    
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::InvalidEscapeSequence { .. } => {} // Expected
        other => panic!("Expected InvalidEscapeSequence error, got: {:?}", other),
    }
}

#[test]
fn test_unterminated_block_comment() {
    let mut lexer = Lexer::new("/* unterminated comment");
    let result = lexer.tokenize();
    
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::UnterminatedBlockComment { .. } => {} // Expected
        other => panic!("Expected UnterminatedBlockComment error, got: {:?}", other),
    }
}

#[test]
fn test_unexpected_character() {
    let mut lexer = Lexer::new("!");
    let result = lexer.tokenize();
    
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::UnexpectedCharacter { char: '!', .. } => {} // Expected
        other => panic!("Expected UnexpectedCharacter error for '!', got: {:?}", other),
    }
}

#[test]
fn test_span_information() {
    let mut lexer = Lexer::new("hello world");
    let tokens = lexer.tokenize().unwrap();
    
    // Check that spans are correct
    assert_eq!(tokens[0].span, Span::new(0, 5)); // "hello"
    assert_eq!(tokens[1].span, Span::new(6, 11)); // "world"
}

#[test]
fn test_complex_program() {
    let source = r#"
        fn add(x: int, y: int) -> int {
            return x + y;
        }
        
        let result = add(5, 10);
        let tensor = Tensor<float, [3, 3]> { 
            1.0, 2.0, 3.0,
            4.0, 5.0, 6.0, 
            7.0, 8.0, 9.0 
        };
    "#;
    
    let mut lexer = Lexer::new(source);
    let result = lexer.tokenize();
    
    assert!(result.is_ok(), "Complex program should tokenize successfully: {:?}", result.err());
    let tokens = result.unwrap();
    
    // Should have more than 30 tokens (rough check)
    assert!(tokens.len() > 30, "Should have many tokens for complex program");
    
    // Check that it ends with EOF
    assert_eq!(tokens.last().unwrap().token_type, TokenType::EndOfFile);
}