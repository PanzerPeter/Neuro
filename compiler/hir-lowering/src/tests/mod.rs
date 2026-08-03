mod closure_tests;
mod coalesce_tests;
mod enum_tests;
mod expr_tests;
mod generic_tests;
mod match_tests;
mod newtype_tests;
mod trait_tests;
mod try_tests;
mod val_else_tests;

use crate::lower_program;
use neuro_hir::{HirExpr, HirItem, HirProgram, HirStmt};

/// Parse and lower `src`, expecting success.
pub(super) fn lower(src: &str) -> HirProgram {
    let ast = syntax_parsing::parse(src).expect("source should parse");
    lower_program(&ast).expect("well-typed program should lower")
}

/// The body of the first function named `name`.
pub(super) fn function_body<'a>(program: &'a HirProgram, name: &str) -> &'a [HirStmt] {
    for item in &program.items {
        if let HirItem::Function(f) = item {
            if f.name == name {
                return &f.body;
            }
        }
    }
    panic!("function '{}' not found", name);
}

/// The initializer expression of the first `val`/`mut` named `name` in `body`.
pub(super) fn binding_init<'a>(body: &'a [HirStmt], name: &str) -> &'a HirExpr {
    for stmt in body {
        if let HirStmt::VarDecl { name: n, init, .. } = stmt {
            if n == name {
                return init.as_ref().expect("binding should have an initializer");
            }
        }
    }
    panic!("binding '{}' not found", name);
}

/// Names of every free function in the lowered program (monomorphized instances
/// included; generic templates are erased).
pub(super) fn function_names(program: &HirProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect()
}

/// Names of every struct in the lowered program (monomorphized instances included;
/// generic templates are erased).
pub(super) fn struct_names(program: &HirProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Struct(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect()
}

/// The method names of the first impl on `type_name` in the lowered program.
pub(super) fn impl_method_names(program: &HirProgram, type_name: &str) -> Vec<String> {
    for item in &program.items {
        if let HirItem::Impl(imp) = item {
            if imp.type_name == type_name {
                return imp.methods.iter().map(|m| m.name.clone()).collect();
            }
        }
    }
    panic!("impl for '{}' not found", type_name);
}

/// Every enum item in the program, by name.
pub(super) fn enum_names(program: &HirProgram) -> Vec<String> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Enum(e) => Some(e.name.clone()),
            _ => None,
        })
        .collect()
}
