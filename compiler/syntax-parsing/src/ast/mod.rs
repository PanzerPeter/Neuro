// NEURO Programming Language - Syntax Parsing
// AST module - Re-exports from ast-types infrastructure
//
// NOTE (VSA 4.0): AST types have been extracted to infrastructure/ast-types
// to maintain slice independence. This module now serves as a re-export layer
// for backward compatibility within the slice.

// Re-export all AST types from infrastructure
pub use ast_types::{BinaryOp, Expr, FunctionDef, Item, Parameter, Stmt, Type, UnaryOp};
