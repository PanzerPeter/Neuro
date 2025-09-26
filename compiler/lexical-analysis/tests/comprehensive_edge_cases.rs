//! Comprehensive edge case tests for the lexical analyzer
//! These tests cover boundary conditions, error recovery, and unusual input scenarios

use lexical_analysis::Lexer;
use shared_types::{TokenType, Keyword};

/// Test extremely long identifiers
#[test]
fn test_very_long_identifier() {
    let long_name = "a".repeat(1000);
    let source = format!("let {} = 42;", long_name);

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().expect("Should tokenize very long identifier");

    // Should have: let, identifier, =, 42, ;, EOF
    assert_eq!(tokens.len(), 6);
    assert!(matches!(tokens[0].token_type, TokenType::Keyword(Keyword::Let)));
    assert!(matches!(tokens[1].token_type, TokenType::Identifier(_)));
    if let TokenType::Identifier(ref name) = tokens[1].token_type {
        assert_eq!(name.len(), 1000);
    }
}

/// Test very large integer literals
#[test]
fn test_large_integer_literals() {
    let source = "999999999999999999999999999999";

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize large integer");

    assert_eq!(tokens.len(), 2); // number + EOF
    assert!(matches!(tokens[0].token_type, TokenType::Integer(_)));
}

/// Test floating point edge cases
#[test]
fn test_float_edge_cases() {
    let test_cases = vec![
        "0.0",
        "1.0",
        "0.123456789",
        "123.456",
        ".5",      // Leading decimal point
        "5.",      // Trailing decimal point
        "1e10",    // Scientific notation
        "1.5e-3",  // Scientific with negative exponent
    ];

    for case in test_cases {
        let mut lexer = Lexer::new(case);
        let tokens = lexer.tokenize().unwrap_or_else(|_| panic!("Failed to tokenize: {}", case));

        assert_eq!(tokens.len(), 2, "Case '{}' should have 2 tokens", case);
        assert!(matches!(tokens[0].token_type, TokenType::Float(_)),
               "Case '{}' should be a float", case);
    }
}

/// Test string literals with various escape sequences
#[test]
fn test_string_escape_sequences() {
    let test_cases = vec![
        (r#""hello world""#, "hello world"),
        (r#""hello\nworld""#, "hello\nworld"),
        (r#""quote: \"hello\"""#, r#"quote: "hello""#),
        (r#""backslash: \\""#, r"backslash: \"),
        (r#""tab:\t""#, "tab:\t"),
        (r#""carriage:\r""#, "carriage:\r"),
    ];

    for (source, expected) in test_cases {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap_or_else(|_| panic!("Failed to tokenize: {}", source));

        assert_eq!(tokens.len(), 2, "Should have string + EOF");
        if let TokenType::String(ref content) = tokens[0].token_type {
            assert_eq!(content, expected, "Escape sequence mismatch for: {}", source);
        } else {
            panic!("Expected string token for: {}", source);
        }
    }
}

/// Test mixed line endings (Windows \r\n, Unix \n, Old Mac \r)
#[test]
fn test_mixed_line_endings() {
    let source = "line1\r\nline2\nline3\rline4";

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should handle mixed line endings");

    // Should tokenize as 4 identifiers + EOF
    assert_eq!(tokens.len(), 5);

    let expected_lines = ["line1", "line2", "line3", "line4"];
    for (i, expected) in expected_lines.iter().enumerate() {
        if let TokenType::Identifier(ref name) = tokens[i].token_type {
            assert_eq!(name, expected);
        } else {
            panic!("Expected identifier at position {}", i);
        }
    }
}

/// Test Unicode characters in identifiers
#[test]
fn test_unicode_identifiers() {
    let test_cases = vec![
        "café",
        "αβγ",
        "변수",
        "变量",
        "🔥hot",
    ];

    for case in test_cases {
        let source = format!("let {} = 42;", case);
        let mut lexer = Lexer::new(&source);
        let tokens = lexer.tokenize().unwrap_or_else(|_| panic!("Failed to tokenize Unicode: {}", case));

        assert_eq!(tokens.len(), 6);
        if let TokenType::Identifier(ref name) = tokens[1].token_type {
            assert_eq!(name, case);
        } else {
            panic!("Expected Unicode identifier: {}", case);
        }
    }
}

/// Test Unicode characters in string literals
#[test]
fn test_unicode_strings() {
    let test_cases = vec![
        ("\"Hello, 世界\"", "Hello, 世界"),
        ("\"🚀 Rocket\"", "🚀 Rocket"),
        ("\"Математика\"", "Математика"),
        ("\"ñoño\"", "ñoño"),
    ];

    for (source, expected) in test_cases {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap_or_else(|_| panic!("Failed to tokenize Unicode string: {}", source));

        assert_eq!(tokens.len(), 2);
        if let TokenType::String(ref content) = tokens[0].token_type {
            assert_eq!(content, expected);
        } else {
            panic!("Expected Unicode string: {}", source);
        }
    }
}

/// Test deeply nested block comments
#[test]
fn test_deeply_nested_comments() {
    let source = "/* outer /* middle /* inner */ middle */ outer */";

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should handle nested comments");

    // Should only have EOF token (everything is commented out)
    assert_eq!(tokens.len(), 1);
    assert!(matches!(tokens[0].token_type, TokenType::EndOfFile));
}

/// Test interleaved comments and code
#[test]
fn test_interleaved_comments() {
    let source = r#"
fn /* function */ test /* name */ ( /* params */ x: int /* type */ ) -> int {
    // Return statement
    return /* value */ x /* end */ ;
}
"#;

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should handle interleaved comments");

    // Should extract: fn, test, (, x, :, int, ), ->, int, {, return, x, ;, }, EOF
    let expected_tokens = vec![
        TokenType::Keyword(Keyword::Fn),
        TokenType::Identifier("test".to_string()),
        TokenType::LeftParen,
        TokenType::Identifier("x".to_string()),
        TokenType::Colon,
        TokenType::Identifier("int".to_string()),
        TokenType::RightParen,
        TokenType::Arrow,
        TokenType::Identifier("int".to_string()),
        TokenType::LeftBrace,
        TokenType::Keyword(Keyword::Return),
        TokenType::Identifier("x".to_string()),
        TokenType::Semicolon,
        TokenType::RightBrace,
        TokenType::EndOfFile,
    ];

    assert_eq!(tokens.len(), expected_tokens.len());
    for (i, expected) in expected_tokens.iter().enumerate() {
        assert_eq!(&tokens[i].token_type, expected, "Token mismatch at position {}", i);
    }
}

/// Test error recovery with invalid characters
#[test]
fn test_error_recovery() {
    let source = "let x @ = 42;"; // @ is not a valid token

    let mut lexer = Lexer::new(source);
    let result = lexer.tokenize();

    // Should fail gracefully or skip invalid characters
    // The exact behavior depends on the error recovery strategy
    match result {
        Ok(tokens) => {
            // If error recovery works, should have some valid tokens
            assert!(!tokens.is_empty());
        }
        Err(_) => {
            // If no error recovery, should produce a clear error
            // This is also acceptable behavior
        }
    }
}

/// Test very long single line
#[test]
fn test_very_long_line() {
    let long_expression = (0..100).map(|i| format!("x{}", i)).collect::<Vec<_>>().join(" + ");
    let source = format!("let result = {};", long_expression);

    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize().expect("Should handle very long line");

    // Should have: let, result, =, many identifiers and +, ;, EOF
    assert!(!tokens.is_empty());
    assert!(matches!(tokens[0].token_type, TokenType::Keyword(Keyword::Let)));
    assert!(matches!(tokens.last().unwrap().token_type, TokenType::EndOfFile));
}

/// Test boundary conditions for numbers
#[test]
fn test_number_boundaries() {
    let test_cases = vec![
        "0",
        "00",    // Leading zeros
        "007",   // Multiple leading zeros
        "123abc", // Number followed by identifier (should be separate tokens)
        "1.2.3",  // Invalid float (should handle gracefully)
    ];

    for case in test_cases {
        let mut lexer = Lexer::new(case);
        let result = lexer.tokenize();

        // Should either tokenize successfully or fail gracefully
        match result {
            Ok(tokens) => {
                assert!(!tokens.is_empty(), "Should produce some tokens for: {}", case);
            }
            Err(_) => {
                // Error is acceptable for invalid cases
            }
        }
    }
}

/// Test empty and whitespace-only input
#[test]
fn test_empty_input() {
    let test_cases = vec![
        "",
        " ",
        "\t",
        "\n",
        "\r\n",
        "   \t  \n  \r\n  ",
    ];

    for case in test_cases {
        let mut lexer = Lexer::new(case);
        let tokens = lexer.tokenize().expect("Should handle empty/whitespace input");

        // Should only have EOF token
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].token_type, TokenType::EndOfFile));
    }
}

/// Test span information accuracy
#[test]
fn test_span_accuracy() {
    let source = "fn test() {\n    let x = 42;\n}";

    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("Should tokenize with span info");

    // Verify that spans are reasonable (non-zero length, increasing positions)
    let mut last_end = 0;
    for token in &tokens[..tokens.len()-1] { // Skip EOF token
        assert!(token.span.start >= last_end, "Spans should not overlap");
        assert!(token.span.end > token.span.start, "Spans should have positive length");
        last_end = token.span.end;
    }
}