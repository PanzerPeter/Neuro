// Operator precedence definitions for Pratt parsing

/// Operator precedence for Pratt parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Precedence {
    Lowest,
    Pipeline,     // |> (Appendix B row 17: looser than every other binary operator)
    Compose,      // >> (Appendix B row 16: looser than range, tighter than |>)
    Range,        // .. ..= (Appendix B row 15: looser than ??, tighter than >>)
    NullCoalesce, // ?? (Appendix B row 14: looser than ||, tighter than range)
    LogicalOr,    // ||
    LogicalAnd,   // &&
    Comparison,   // < > <= >= == != (Appendix B row 11: one row, no chaining)
    BitwiseOr,    // | (Appendix B rows 8-10: bitwise binds tighter than comparison)
    BitwiseXor,   // ^
    BitwiseAnd,   // &
    Shift,        // <<
    Sum,          // + -
    Product,      // * / %
    MatMul,       // @ (Appendix B row 4: tighter than `*`, looser than `as`)
    Cast,         // as
    Unary,        // - ! ~
    Call,         // function calls
    FieldAccess,  // . (member access, binds tighter than call)
}
