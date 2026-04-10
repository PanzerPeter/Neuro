// NEURO Programming Language - Syntax Parsing
// Operator precedence definitions for Pratt parsing

/// Operator precedence for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Precedence {
    Lowest,
    LogicalOr,   // ||
    LogicalAnd,  // &&
    Equality,    // == !=
    Comparison,  // < > <= >=
    Sum,         // + -
    Product,     // * / %
    Cast,        // as
    Unary,       // - !
    Call,        // function calls
    FieldAccess, // . (member access, binds tighter than call)
}
