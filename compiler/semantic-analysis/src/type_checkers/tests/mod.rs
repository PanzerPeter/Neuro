mod array_tests;
mod borrow_tests;
mod builtin_tests;
mod coalesce_tests;
mod comparison_tests;
mod decl_tests;
mod derive_tests;
mod drop_tests;
mod enum_expr_tests;
mod enum_tests;
mod generic_tests;
mod intrinsic_tests;
mod iteration_tests;
mod literal_tests;
mod loop_adapter_tests;
mod loop_tests;
mod match_tests;
mod newtype_tests;
mod slice_tests;
mod string_tests;
mod tensor_tests;
mod trait_tests;
mod try_tests;
mod val_else_tests;
mod visibility_tests;

use super::TypeChecker;
use crate::errors::TypeError;
use ast_types::{BinaryOp, Expr, FunctionDef, Parameter, Stmt};
use shared_types::{Identifier, Literal, Span};

pub(super) fn make_ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: Span::new(0, 0),
    }
}

pub(super) fn make_type(name: &str) -> ast_types::Type {
    ast_types::Type::Named(make_ident(name))
}

/// Helper to create a simple function for testing
pub(super) fn make_function(
    name: &str,
    params: Vec<(String, String)>,
    return_type: Option<String>,
    body: Vec<Stmt>,
) -> FunctionDef {
    FunctionDef {
        name: make_ident(name),
        exported: false,
        module: 0,
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        params: params
            .into_iter()
            .map(|(pname, pty)| Parameter {
                label: ast_types::ParamLabel::Implicit,
                name: make_ident(&pname),
                ty: make_type(&pty),
                span: Span::new(0, 0),
            })
            .collect(),
        return_type: return_type.map(|rt| make_type(&rt)),
        body,
        attributes: Vec::new(),
        span: Span::new(0, 0),
    }
}

/// Type-check `source` end to end and return its errors (empty on success).
/// Used by the borrow-exclusivity tests below, which exercise multi-statement
/// programs more naturally through the parser than via hand-built AST.
pub(super) fn semantic_errors(source: &str) -> Vec<TypeError> {
    let items = syntax_parsing::parse(source).expect("source should parse");
    match crate::type_check(&items) {
        Ok(_) => Vec::new(),
        Err(errors) => errors,
    }
}
