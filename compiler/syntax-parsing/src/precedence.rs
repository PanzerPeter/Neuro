// Operator precedence definitions for Pratt parsing

/// Operator precedence for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Precedence {
    Lowest,
    Pipeline,     // |> (Appendix B row 17: looser than every other binary operator)
    Range,        // .. ..= (Appendix B row 15: looser than ??, tighter than |>)
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
    MatMul,       // @ (Appendix B row 4: tighter than `*`, looser than `as`)
    Cast,         // as
    Unary,        // - ! ~
    Call,         // function calls
    FieldAccess,  // . (member access, binds tighter than call)
}
