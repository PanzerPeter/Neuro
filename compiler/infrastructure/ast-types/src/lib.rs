//! NEURO Programming Language - AST Types
//!
//! Infrastructure component providing Abstract Syntax Tree (AST) type definitions
//! used across compiler slices. This crate contains pure data structures with no
//! business logic.
//!
//! # VSA 4.0 Compliance
//!
//! This is a pure infrastructure crate following VSA 4.0 principles. AST types
//! were extracted from syntax-parsing slice to eliminate cross-slice dependencies
//! and maintain slice independence.
//!
//! ## Consumers
//! - **syntax-parsing**: Constructs AST during parsing
//! - **semantic-analysis**: Type-checks and validates AST
//! - **llvm-backend**: Generates code from AST
//!
//! ## Design Rationale
//! AST types are shared data structures (not business logic), making them
//! appropriate for infrastructure. This allows feature slices to remain
//! independent while sharing the common AST representation.

pub mod expressions;
pub mod items;
pub mod statements;
pub mod types;

// Re-export all AST types for convenient access
pub use expressions::{BinaryOp, Expr, UnaryOp};
pub use items::{FunctionDef, Item, Parameter};
pub use statements::Stmt;
pub use types::Type;
