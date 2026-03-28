// NEURO Programming Language - AST Types
// Top-level item AST nodes

use shared_types::{Identifier, Span};

use super::statements::Stmt;
use super::types::Type;

/// Function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    pub name: Identifier,
    pub params: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

/// Function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// A single field in a struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: Identifier,
    pub ty: Type,
    pub span: Span,
}

/// Struct definition
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: Identifier,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

/// Top-level AST item
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDef),
    Struct(StructDef),
}
