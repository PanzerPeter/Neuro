//! Tests for lexical analysis edge cases

#[cfg(test)]
mod tests {
    use crate::Lexer;
    use shared_types::{TokenType, Keyword};

    /// Test tokenization of logical operators
    #[test]
    fn test_logical_operators() {
        let source = "! && ||";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 4); // 3 operators + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::LogicalNot);
        assert_eq!(tokens[1].token_type, TokenType::LogicalAnd);
        assert_eq!(tokens[2].token_type, TokenType::LogicalOr);
        assert_eq!(tokens[3].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization of comparison operators
    #[test]
    fn test_comparison_operators() {
        let source = "< <= > >= == !=";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 7); // 6 operators + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::Less);
        assert_eq!(tokens[1].token_type, TokenType::LessEqual);
        assert_eq!(tokens[2].token_type, TokenType::Greater);
        assert_eq!(tokens[3].token_type, TokenType::GreaterEqual);
        assert_eq!(tokens[4].token_type, TokenType::Equal);
        assert_eq!(tokens[5].token_type, TokenType::NotEqual);
    }
    
    /// Test that != is tokenized as NotEqual, not LogicalNot followed by Assign
    #[test]
    fn test_not_equal_not_separate() {
        let source = "!=";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 2); // != + EOF
        assert_eq!(tokens[0].token_type, TokenType::NotEqual);
        assert_eq!(tokens[1].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization of keywords vs identifiers
    #[test]
    fn test_keywords_vs_identifiers() {
        let source = "if return fn let mut identifier";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 7); // 6 tokens + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::Keyword(Keyword::If));
        assert_eq!(tokens[1].token_type, TokenType::Keyword(Keyword::Return));
        assert_eq!(tokens[2].token_type, TokenType::Keyword(Keyword::Fn));
        assert_eq!(tokens[3].token_type, TokenType::Keyword(Keyword::Let));
        assert_eq!(tokens[4].token_type, TokenType::Keyword(Keyword::Mut));
        assert_eq!(tokens[5].token_type, TokenType::Identifier("identifier".to_string()));
        assert_eq!(tokens[6].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization of boolean literals
    #[test]
    fn test_boolean_literals() {
        let source = "true false";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 3); // true + false + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::Boolean(true));
        assert_eq!(tokens[1].token_type, TokenType::Boolean(false));
        assert_eq!(tokens[2].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization of numeric literals
    #[test]
    fn test_numeric_literals() {
        let source = "42 3.14 0 123";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 5); // 4 numbers + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::Integer("42".to_string()));
        assert_eq!(tokens[1].token_type, TokenType::Float("3.14".to_string()));
        assert_eq!(tokens[2].token_type, TokenType::Integer("0".to_string()));
        assert_eq!(tokens[3].token_type, TokenType::Integer("123".to_string()));
        assert_eq!(tokens[4].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization with mixed whitespace and newlines
    #[test]
    fn test_whitespace_handling() {
        let source = "if\n  x\t<=\r\n 1";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        // Should have: if, newline, x, <=, 1, newline, EOF
        assert!(tokens.len() >= 6);
        
        assert_eq!(tokens[0].token_type, TokenType::Keyword(Keyword::If));
        assert_eq!(tokens[1].token_type, TokenType::Newline);
        
        // Find the x token (may have more newlines/whitespace)
        let x_pos = tokens.iter().position(|t| matches!(t.token_type, TokenType::Identifier(ref s) if s == "x")).unwrap();
        let le_pos = tokens.iter().position(|t| matches!(t.token_type, TokenType::LessEqual)).unwrap();
        let one_pos = tokens.iter().position(|t| matches!(t.token_type, TokenType::Integer(ref s) if s == "1")).unwrap();
        
        assert!(x_pos < le_pos);
        assert!(le_pos < one_pos);
    }
    
    /// Test string literal tokenization
    #[test] 
    fn test_string_literals() {
        let source = r#""hello world" "test""#;
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 3); // 2 strings + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::String("hello world".to_string()));
        assert_eq!(tokens[1].token_type, TokenType::String("test".to_string()));
        assert_eq!(tokens[2].token_type, TokenType::EndOfFile);
    }
    
    /// Test tokenization of arrow operator
    #[test]
    fn test_arrow_operator() {
        let source = "-> ->";
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Should tokenize successfully");
        
        assert_eq!(tokens.len(), 3); // -> + -> + EOF
        
        assert_eq!(tokens[0].token_type, TokenType::Arrow);
        assert_eq!(tokens[1].token_type, TokenType::Arrow);
        assert_eq!(tokens[2].token_type, TokenType::EndOfFile);
    }
}