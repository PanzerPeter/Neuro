// NEURO Programming Language - Semantic Analysis
// Feature slice for type checking and semantic validation
//
// This slice follows Vertical Slice Architecture (VSA) principles:
// - Self-contained type checking functionality
// - Minimal dependencies (only infrastructure and syntax-parsing)
// - Clear module boundaries with pub(crate) for internals
// - Public API limited to type_check() entry point

mod errors;
mod symbol_table;
mod type_checker;
mod types;

// Public exports
pub use errors::TypeError;
pub use types::Type;

use ast_types::Item;
use type_checker::TypeChecker;

/// Type check a NEURO program.
///
/// This function performs semantic analysis on a parsed NEURO program, validating:
/// - Type correctness of expressions and statements
/// - Variable and function declarations
/// - Function signatures and call sites
/// - Control flow (if/else) conditions
/// - Return type matching
///
/// # Phase 1 Support
///
/// Currently supports:
/// - Primitive types: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `bool`
/// - Function definitions with parameters and return types
/// - Variable declarations (`val` and `mut`)
/// - Binary operators (arithmetic, comparison, logical)
/// - Unary operators (negation, logical not)
/// - Function calls
/// - If/else statements
/// - While loops and range-for loops with `break` and `continue` validation
/// - Return statements
/// - Lexical scoping
///
/// # Arguments
///
/// * `items` - A slice of AST items (functions) from the parser
///
/// # Returns
///
/// * `Ok(())` - Program is type-correct
/// * `Err(Vec<TypeError>)` - One or more type errors were found
///
/// # Examples
///
/// ```ignore
/// use syntax_parsing::parse;
/// use semantic_analysis::type_check;
///
/// let source = r#"
///     func add(a: i32, b: i32) -> i32 {
///         return a + b
///     }
/// "#;
///
/// let ast = parse(source).unwrap();
/// match type_check(&ast) {
///     Ok(()) => println!("Program is type-correct"),
///     Err(errors) => {
///         for error in errors {
///             eprintln!("Type error: {}", error);
///         }
///     }
/// }
/// ```
///
/// # Error Handling
///
/// This function collects multiple errors in a single pass (fail-slow approach)
/// to provide comprehensive feedback to the user. All errors include source
/// location information (spans) for precise error reporting.
pub fn type_check(items: &[Item]) -> Result<(), Vec<TypeError>> {
    let mut checker = TypeChecker::new();
    if checker.check_program(items).is_err() {
        Err(checker.into_errors())
    } else {
        Ok(())
    }
}
