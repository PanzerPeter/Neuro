// NEURO Programming Language - Syntax Parsing
// Operator precedence definitions for Pratt parsing

/// Operator precedence for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Precedence {
    Lowest,
    NullCoalesce, // ?? (Appendix B row 14: looser than ||, tighter than range)
    LogicalOr,    // ||
    LogicalAnd,   // &&
    BitwiseOr,    // |
    BitwiseXor,   // ^
    BitwiseAnd,   // &
    Equality,     // == !=
    Comparison,   // < > <= >=
    Shift,        // <<
    Sum,          // + -
    Product,      // * / %
    Cast,         // as
    Unary,        // - ! ~
    Call,         // function calls
    FieldAccess,  // . (member access, binds tighter than call)
}
