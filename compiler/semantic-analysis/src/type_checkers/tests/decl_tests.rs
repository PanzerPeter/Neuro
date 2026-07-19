use super::*;

use ast_types::{FieldDef, Item, MethodDef, StructDef};

fn struct_item(name: &str, fields: &[(&str, &str)]) -> Item {
    Item::Struct(StructDef {
        name: make_ident(name),
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        fields: fields
            .iter()
            .map(|(fname, fty)| FieldDef {
                name: make_ident(fname),
                ty: make_type(fty),
                span: Span::new(0, 0),
            })
            .collect(),
        attributes: Vec::new(),
        span: Span::new(0, 0),
    })
}

fn impl_item(type_name: &str, method_names: &[&str]) -> Item {
    Item::Impl(ast_types::ImplDef {
        trait_name: None,
        type_name: make_ident(type_name),
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        type_args: Vec::new(),
        assoc_types: Vec::new(),
        methods: method_names
            .iter()
            .map(|m| MethodDef {
                name: make_ident(m),
                self_param: Some(ast_types::SelfParam::Ref),
                params: Vec::new(),
                return_type: None,
                body: Vec::new(),
                attributes: Vec::new(),
                span: Span::new(0, 0),
            })
            .collect(),
        span: Span::new(0, 0),
    })
}

fn reserved_names(errors: &[TypeError]) -> Vec<String> {
    errors
        .iter()
        .filter_map(|e| match e {
            TypeError::ReservedNameSeparator { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn single_underscore_names_are_accepted() {
    // The reservation covers `__` only — ordinary snake_case must stay legal.
    let mut checker = TypeChecker::new();
    let items = vec![
        struct_item("my_point", &[("x_pos", "i32")]),
        impl_item("my_point", &["get_x"]),
    ];

    let _ = checker.check_program(&items);

    assert!(reserved_names(&checker.into_errors()).is_empty());
}

#[test]
fn double_underscore_in_declared_names_is_rejected() {
    // Each of these would mint a symbol indistinguishable from a compiler-generated
    // `Receiver__method`, so all four are reported.
    let mut checker = TypeChecker::new();
    let items = vec![
        struct_item("My__Point", &[("x__pos", "i32")]),
        impl_item("My__Point", &["get__x"]),
        Item::Function(make_function("do__work", vec![], None, vec![])),
    ];

    let _ = checker.check_program(&items);

    let mut reported = reserved_names(&checker.into_errors());
    reported.sort();
    assert_eq!(reported, vec!["My__Point", "do__work", "get__x", "x__pos"]);
}

#[test]
fn a_user_method_cannot_forge_a_generic_instance_symbol() {
    // `Pair_g_i32` is the mangled name of `Pair<i32>`; a method `push` on it lowers to
    // `Pair_g_i32__push`. A user method named `g_i32__push` on a struct `Pair` would
    // lower to the identical symbol — rejecting `__` in the method name closes it.
    let mut checker = TypeChecker::new();
    let items = vec![
        struct_item("Pair", &[("first", "i32")]),
        impl_item("Pair", &["g_i32__push"]),
    ];

    let _ = checker.check_program(&items);

    assert_eq!(reserved_names(&checker.into_errors()), vec!["g_i32__push"]);
}
