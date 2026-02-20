// NEURO Programming Language - Syntax Parsing
// Feature slice for AST generation and syntax analysis
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained parsing functionality
// - Minimal dependencies (only infrastructure and lexical-analysis)
// - Clear module boundaries with pub(crate) for internals
// - Public API limited to parse() and parse_expr() entry points

mod ast;
mod errors;
mod parser;
mod precedence;

// Public exports
pub use ast::{BinaryOp, Expr, FunctionDef, Item, Parameter, Stmt, Type, UnaryOp};
pub use errors::{ParseError, ParseResult};

use lexical_analysis::tokenize;
use parser::Parser;
use precedence::Precedence;

/// Parse NEURO source code into AST
///
/// This function takes NEURO source code and produces an Abstract Syntax Tree (AST)
/// representing the program structure. It performs lexical analysis (tokenization)
/// followed by syntax analysis (parsing).
///
/// # Phase 1 Support
///
/// Currently supports:
/// - Function definitions with parameters and return types
/// - Variable declarations (`val` and `mut`)
/// - Expressions: literals, identifiers, binary/unary operators, function calls
/// - Statements: variable declarations, if/else, return, expression statements
/// - Type annotations for primitive types
///
/// # Arguments
///
/// * `source` - The NEURO source code as a string
///
/// # Returns
///
/// * `Ok(Vec<Item>)` - Successfully parsed AST items (currently only functions)
/// * `Err(ParseError)` - Syntax error or lexical error
///
/// # Examples
///
/// ```ignore
/// use syntax_parsing::parse;
///
/// let source = r#"
///     func add(a: i32, b: i32) -> i32 {
///         return a + b
///     }
/// "#;
///
/// match parse(source) {
///     Ok(items) => println!("Parsed {} items", items.len()),
///     Err(e) => eprintln!("Parse error: {}", e),
/// }
/// ```
///
/// # Error Handling
///
/// Parse errors include precise source location information (spans) for error reporting.
/// Errors can be either lexical (invalid tokens) or syntactic (invalid grammar).
pub fn parse(source: &str) -> ParseResult<Vec<Item>> {
    // Tokenize the source code
    let tokens = tokenize(source)?;

    // Create parser and parse program
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

/// Parse a NEURO expression from source code
///
/// This is a convenience function for parsing standalone expressions,
/// useful for testing and REPL environments.
///
/// # Arguments
///
/// * `source` - The NEURO expression source code as a string
///
/// # Returns
///
/// * `Ok(Expr)` - Successfully parsed expression AST
/// * `Err(ParseError)` - Syntax error or lexical error
///
/// # Examples
///
/// ```ignore
/// use syntax_parsing::parse_expr;
///
/// let source = "2 + 3 * 4";
/// match parse_expr(source) {
///     Ok(expr) => println!("Parsed expression: {:?}", expr),
///     Err(e) => eprintln!("Parse error: {}", e),
/// }
/// ```
pub fn parse_expr(source: &str) -> ParseResult<Expr> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_expr(Precedence::Lowest)
}
