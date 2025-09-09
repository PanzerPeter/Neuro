//! Tests for parser edge cases and regression tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;
    use shared_types::{TokenType, Keyword, Token, Span};
    use lexical_analysis::Lexer;

    /// Test parsing function with if statement followed by return statement
    /// This is a regression test for the discriminant bug
    #[test]
    fn test_if_followed_by_return_statement() {
        let source = r#"
fn test() -> int {
    if n <= 1 {
        return false;
    }
    return true;
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 1, "Should have exactly one function");
        
        // Verify it's a function
        if let shared_types::Item::Function(func) = &program.items[0] {
            assert_eq!(func.name, "test");
            assert_eq!(func.body.statements.len(), 2, "Function should have 2 statements");
            
            // First statement should be if statement
            assert!(matches!(func.body.statements[0], shared_types::Statement::If(_)));
            
            // Second statement should be return statement
            assert!(matches!(func.body.statements[1], shared_types::Statement::Return(_)));
        } else {
            panic!("First item should be a function");
        }
    }
    
    /// Test parsing complex expression with logical operators
    #[test]
    fn test_complex_logical_expressions() {
        let source = r#"
fn test() -> bool {
    let result = x > 0 && y > 0 || z == 0;
    return !result && (a != b);
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 1, "Should have exactly one function");
    }
    
    /// Test parsing nested if statements
    #[test]
    fn test_nested_if_statements() {
        let source = r#"
fn test() -> int {
    if x > 0 {
        if y > 0 {
            return 1;
        }
        return 2;
    }
    return 0;
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 1, "Should have exactly one function");
    }
    
    /// Test parsing function calls in expressions
    #[test]
    fn test_function_calls_in_expressions() {
        let source = r#"
fn test() -> int {
    return fibonacci(n - 1) + fibonacci(n - 2);
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 1, "Should have exactly one function");
    }
    
    /// Test parsing while loops
    #[test]
    fn test_while_loop_parsing() {
        let source = r#"
fn test() -> int {
    let mut i = 0;
    while i < 10 {
        i = i + 1;
    }
    return i;
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 1, "Should have exactly one function");
    }
    
    /// Test the exact complex program that was originally failing
    #[test]
    fn test_original_complex_program() {
        let source = r#"
// Complex NEURO program to test full compilation pipeline
fn is_prime(n: int) -> bool {
    if n <= 1 {
        return false;
    }
    if n <= 3 {
        return true;
    }
    if n % 2 == 0 || n % 3 == 0 {
        return false;
    }
    
    let mut i = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i = i + 6;
    }
    return true;
}

fn fibonacci(n: int) -> int {
    if n <= 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

fn logical_test(x: int, y: int) -> bool {
    let is_positive = x > 0 && y > 0;
    let has_zero = x == 0 || y == 0;
    let is_negative = !is_positive && !has_zero;
    
    return is_positive || has_zero || is_negative;
}

fn main() -> int {
    let num = 17;
    let prime_result = is_prime(num);
    
    let fib_result = fibonacci(8);
    
    let logic_result = logical_test(-5, 10);
    
    if prime_result && fib_result > 20 {
        return 1;
    }
    
    if !logic_result {
        return -1;
    }
    
    return 0;
}
        "#;
        
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexical analysis should succeed");
        
        let mut parser = Parser::new(tokens);
        let program = parser.parse().expect("Parsing should succeed");
        
        assert_eq!(program.items.len(), 4, "Should have exactly 4 functions");
        
        // Verify all functions are parsed correctly
        let function_names: Vec<String> = program.items.iter()
            .filter_map(|item| {
                if let shared_types::Item::Function(func) = item {
                    Some(func.name.clone())
                } else {
                    None
                }
            })
            .collect();
        
        assert_eq!(function_names, vec!["is_prime", "fibonacci", "logical_test", "main"]);
    }
}