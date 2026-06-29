// Feature slice for AST generation and syntax analysis.
// Public API: the `parse()` and `parse_expr()` entry points.

mod ast;
mod errors;
mod parser;
mod precedence;

pub use ast::{
    Attribute, BinaryOp, Expr, FieldDef, FieldInit, FunctionDef, ImplDef, Item, MethodDef,
    Parameter, SelfParam, Stmt, StructDef, Type, UnaryOp,
};
pub use errors::{ParseError, ParseResult};

use lexical_analysis::tokenize;
use parser::Parser;
use precedence::Precedence;

/// Parse Neuro source into AST items, tokenizing first.
///
/// Errors carry precise source spans and may be lexical (invalid tokens) or
/// syntactic (invalid grammar).
///
/// # Examples
///
/// ```
/// use syntax_parsing::parse;
///
/// fn main() {
///     let source = r#"
///         func add(a: i32, b: i32) -> i32 {
///             return a + b
///         }
///     "#;
///
///     match parse(source) {
///         Ok(items) => println!("Parsed {} items", items.len()),
///         Err(e) => eprintln!("Parse error: {}", e),
///     }
/// }
/// ```
pub fn parse(source: &str) -> ParseResult<Vec<Item>> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

/// Parse a standalone Neuro expression — a convenience for tests and REPLs.
///
/// # Examples
///
/// ```
/// use syntax_parsing::parse_expr;
///
/// fn main() {
///     let source = "2 + 3 * 4";
///     match parse_expr(source) {
///         Ok(expr) => println!("Parsed expression: {:?}", expr),
///         Err(e) => eprintln!("Parse error: {}", e),
///     }
/// }
/// ```
pub fn parse_expr(source: &str) -> ParseResult<Expr> {
    let tokens = tokenize(source)?;
    let mut parser = Parser::new(tokens);
    parser.parse_expr(Precedence::Lowest)
}
