// Feature slice for type checking and semantic validation.
// Public API: the `type_check()` entry point.

mod errors;
mod extents;
mod symbol_table;
pub(crate) mod type_checkers;
mod types;
mod warnings;

pub use errors::TypeError;
pub use types::Type;
pub use warnings::{Warning, WarningCode};

use ast_types::Item;
use type_checkers::TypeChecker;

/// Type check a Neuro program: expression/statement types, declarations,
/// call sites, control-flow conditions, and return-type matching.
///
/// Returns the collected non-fatal lint [`Warning`]s on success, or the
/// [`TypeError`]s otherwise.
///
/// # Examples
///
/// ```
/// use syntax_parsing::parse;
/// use semantic_analysis::type_check;
///
/// fn main() {
///     let source = r#"
///         func add(a: i32, b: i32) -> i32 {
///             return a + b
///         }
///     "#;
///
///     let ast = parse(source).unwrap();
///     match type_check(&ast) {
///         Ok(warnings) => {
///             for warning in warnings {
///                 eprintln!("Warning: {}", warning);
///             }
///         }
///         Err(errors) => {
///             for error in errors {
///                 eprintln!("Type error: {}", error);
///             }
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
pub fn type_check(items: &[Item]) -> Result<Vec<Warning>, Vec<TypeError>> {
    type_check_program(items, &[])
}

/// [`type_check`] for a program whose modules nest: `enclosing[id]` is the module an inline
/// block with id `id` is written in, `None` (or no entry) for a file. A block may read and
/// build the private fields of every module enclosing it, as it may name their private
/// items.
pub fn type_check_program(
    items: &[Item],
    enclosing: &[Option<ast_types::ModuleId>],
) -> Result<Vec<Warning>, Vec<TypeError>> {
    types::set_template_params(Default::default());
    let mut checker = TypeChecker::new();
    checker.set_module_parents(enclosing);
    let outcome = checker.check_program(items);
    types::set_template_params(checker.template_params());
    if outcome.is_err() {
        Err(checker.into_errors())
    } else {
        Ok(checker.into_warnings())
    }
}
