//! Field visibility: the rule the parser cannot state and module resolution cannot
//! check, because it needs the receiver's type.
//!
//! The fixtures are built by hand rather than parsed: a single parsed file is one
//! module, so only a hand-built program can put a struct and its reader in different
//! ones — which is exactly the case being tested.

use super::*;

use ast_types::{FieldDef, FieldInit, Item, ModuleId, StructDef};

/// A struct whose fields are `(name, type, exported)`, declared in `module`.
fn struct_in_module(name: &str, module: ModuleId, fields: &[(&str, &str, bool)]) -> Item {
    Item::Struct(StructDef {
        name: make_ident(name),
        exported: true,
        module,
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        fields: fields
            .iter()
            .map(|(fname, fty, exported)| FieldDef {
                name: make_ident(fname),
                exported: *exported,
                ty: make_type(fty),
                span: Span::new(0, 0),
            })
            .collect(),
        attributes: Vec::new(),
        span: Span::new(0, 0),
    })
}

/// A function in `module` whose body binds `val v = <expr>`.
fn reader_in_module(module: ModuleId, expr: Expr) -> Item {
    let mut def = make_function("read", Vec::new(), None, Vec::new());
    def.module = module;
    def.body.push(Stmt::VarDecl {
        name: make_ident("v"),
        ty: None,
        init: Some(expr),
        mutable: false,
        span: Span::new(0, 0),
    });
    Item::Function(def)
}

fn field_access(struct_name: &str, field: &str) -> Expr {
    Expr::FieldAccess {
        object: Box::new(Expr::StructLiteral {
            name: make_ident(struct_name),
            fields: vec![FieldInit {
                name: make_ident("open"),
                value: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 0))),
                span: Span::new(0, 0),
            }],
            base: None,
            span: Span::new(0, 0),
        }),
        field: make_ident(field),
        span: Span::new(0, 0),
    }
}

fn errors(items: &[Item]) -> Vec<TypeError> {
    let mut checker = TypeChecker::new();
    let _ = checker.check_program(items);
    checker.into_errors()
}

fn is_private_field(error: &TypeError, field: &str) -> bool {
    matches!(error, TypeError::PrivateField { field_name, .. } if field_name == field)
}

const FIELDS: &[(&str, &str, bool)] = &[("open", "i32", true), ("closed", "i32", false)];

#[test]
fn a_private_field_read_from_another_module_is_rejected() {
    let items = [
        struct_in_module("S", 1, FIELDS),
        reader_in_module(0, field_access("S", "closed")),
    ];
    // The struct literal omits `closed`, so a missing-field error rides along; only the
    // visibility verdict is under test here.
    assert!(errors(&items).iter().any(|e| is_private_field(e, "closed")));
}

#[test]
fn a_private_field_read_from_its_own_module_is_accepted() {
    let items = [
        struct_in_module("S", 1, FIELDS),
        reader_in_module(1, field_access("S", "closed")),
    ];
    assert!(!errors(&items).iter().any(|e| is_private_field(e, "closed")));
}

#[test]
fn an_exported_field_crosses_the_boundary() {
    let items = [
        struct_in_module("S", 1, FIELDS),
        reader_in_module(0, field_access("S", "open")),
    ];
    assert!(!errors(&items).iter().any(|e| is_private_field(e, "open")));
}

#[test]
fn a_struct_update_from_another_module_may_not_copy_a_private_field() {
    let base = Expr::Identifier(make_ident("source"));
    let update = Expr::StructLiteral {
        name: make_ident("S"),
        fields: vec![FieldInit {
            name: make_ident("open"),
            value: Box::new(Expr::Literal(Literal::Integer(1, None), Span::new(0, 0))),
            span: Span::new(0, 0),
        }],
        base: Some(Box::new(base)),
        span: Span::new(0, 0),
    };
    let items = [
        struct_in_module("S", 1, FIELDS),
        reader_in_module(0, update),
    ];
    // `..base` supplies every unlisted field, so it reaches `closed` without naming it.
    assert!(errors(&items).iter().any(|e| is_private_field(e, "closed")));
}
