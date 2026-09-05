use ast_types::{
    Expr, FunctionDef, ImplDef, Item, MethodDef, ParamLabel, Parameter, SelfParam, Stmt, Type,
};
use shared_types::{Identifier, Literal, Span};

use crate::{bind_arguments, ArgumentError};

fn span() -> Span {
    Span::new(0, 0)
}

fn ident(name: &str) -> Identifier {
    Identifier {
        name: name.to_string(),
        span: span(),
    }
}

/// `name: i32` — an ordinary parameter, nameable but not required to be named.
fn implicit(name: &str) -> Parameter {
    Parameter {
        label: ParamLabel::Implicit,
        name: ident(name),
        ty: Type::Named(ident("i32")),
        span: span(),
    }
}

/// `external internal: i32` — the caller must write `external:`.
fn external(label: &str, name: &str) -> Parameter {
    Parameter {
        label: ParamLabel::External(ident(label)),
        name: ident(name),
        ty: Type::Named(ident("i32")),
        span: span(),
    }
}

/// `_ name: i32` — the caller must pass positionally.
fn suppressed(name: &str) -> Parameter {
    Parameter {
        label: ParamLabel::Suppressed,
        name: ident(name),
        ty: Type::Named(ident("i32")),
        span: span(),
    }
}

fn int(value: i128) -> Expr {
    Expr::Literal(Literal::Integer(value, None), span())
}

/// One argument as written: its optional call-site name and its value.
type Arg = (Option<Identifier>, Expr);

fn positional(value: i128) -> Arg {
    (None, int(value))
}

fn named(label: &str, value: i128) -> Arg {
    (Some(ident(label)), int(value))
}

fn call(callee: Expr, written: Vec<Arg>) -> Expr {
    let mut args = Vec::new();
    let mut arg_labels = Vec::new();
    for (label, value) in written {
        arg_labels.push(label);
        args.push(value);
    }
    if arg_labels.iter().all(Option::is_none) {
        arg_labels.clear();
    }
    Expr::Call {
        func: Box::new(callee),
        type_args: Vec::new(),
        args,
        arg_labels,
        span: span(),
    }
}

fn func(name: &str, params: Vec<Parameter>, body: Vec<Stmt>) -> Item {
    Item::Function(FunctionDef {
        name: ident(name),
        exported: false,
        module: 0,
        generics: Vec::new(),
        lifetimes: Vec::new(),
        where_predicates: Vec::new(),
        params,
        return_type: None,
        body,
        attributes: Vec::new(),
        span: span(),
    })
}

fn method(name: &str, self_param: Option<SelfParam>, params: Vec<Parameter>) -> MethodDef {
    MethodDef {
        name: ident(name),
        self_param,
        params,
        return_type: None,
        body: Vec::new(),
        attributes: Vec::new(),
        span: span(),
    }
}

fn impl_block(type_name: &str, methods: Vec<MethodDef>) -> Item {
    Item::Impl(ImplDef {
        module: 0,
        trait_name: None,
        type_name: ident(type_name),
        generics: Vec::new(),
        lifetimes: Vec::new(),
        type_args: Vec::new(),
        where_predicates: Vec::new(),
        assoc_types: Vec::new(),
        methods,
        span: span(),
    })
}

/// The single call in `main`'s body after binding, as the integer literals it holds.
fn bound_values(items: &[Item]) -> Vec<i128> {
    let Some(Item::Function(main)) = items.last() else {
        panic!("expected a trailing function item");
    };
    let Some(Stmt::Expr(Expr::Call {
        args, arg_labels, ..
    })) = main.body.first()
    else {
        panic!("expected a call statement");
    };
    assert!(arg_labels.is_empty(), "a label survived binding");
    args.iter()
        .map(|arg| match arg {
            Expr::Literal(Literal::Integer(v, _), _) => *v,
            other => panic!("expected an integer argument, found {other:?}"),
        })
        .collect()
}

/// A program declaring `target` and calling it once from `main`.
fn program(params: Vec<Parameter>, args: Vec<Arg>) -> Vec<Item> {
    vec![
        func("target", params, Vec::new()),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(Expr::Identifier(ident("target")), args))],
        ),
    ]
}

#[test]
fn a_positional_call_is_left_untouched() {
    let mut items = program(
        vec![implicit("a"), implicit("b")],
        vec![positional(1), positional(2)],
    );
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 2]);
}

#[test]
fn named_arguments_bind_by_name_not_position() {
    let mut items = program(
        vec![implicit("a"), implicit("b")],
        vec![named("b", 2), named("a", 1)],
    );
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 2]);
}

#[test]
fn a_positional_prefix_may_precede_named_arguments() {
    let mut items = program(
        vec![implicit("a"), implicit("b"), implicit("c")],
        vec![positional(1), named("c", 3), named("b", 2)],
    );
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 2, 3]);
}

#[test]
fn a_positional_argument_may_not_follow_a_named_one() {
    let mut items = program(
        vec![implicit("a"), implicit("b")],
        vec![named("b", 2), positional(1)],
    );
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::PositionalAfterNamed { .. }]
    ));
}

#[test]
fn an_unknown_label_is_rejected() {
    let mut items = program(vec![implicit("a")], vec![named("nope", 1)]);
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::UnknownArgumentLabel { label, .. }] if label == "nope"
    ));
}

#[test]
fn one_parameter_may_not_be_named_twice() {
    let mut items = program(
        vec![implicit("a"), implicit("b")],
        vec![named("a", 1), named("a", 2)],
    );
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::DuplicateArgumentLabel { .. }]
    ));
}

#[test]
fn an_external_label_is_required_at_the_call_site() {
    let mut items = program(
        vec![suppressed("value"), external("min", "lo")],
        vec![positional(5), positional(0)],
    );
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::MissingArgumentLabel { label, .. }] if label == "min"
    ));
}

#[test]
fn an_external_label_binds_by_its_external_name() {
    let mut items = program(
        vec![
            suppressed("value"),
            external("min", "lo"),
            external("max", "hi"),
        ],
        vec![positional(5), named("max", 9), named("min", 0)],
    );
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![5, 0, 9]);
}

#[test]
fn a_suppressed_label_is_not_a_call_site_name() {
    let mut items = program(vec![suppressed("value")], vec![named("value", 1)]);
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::SuppressedLabel { label, .. }] if label == "value"
    ));
}

#[test]
fn a_wrong_arity_named_call_reports_the_count() {
    let mut items = program(vec![implicit("a"), implicit("b")], vec![named("a", 1)]);
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::ArgumentCountMismatch {
            expected: 2,
            found: 1,
            ..
        }]
    ));
}

#[test]
fn a_callee_with_no_declared_names_rejects_a_label() {
    let mut items = vec![func(
        "main",
        Vec::new(),
        vec![Stmt::Expr(call(
            Expr::Identifier(ident("some_closure")),
            vec![named("a", 1)],
        ))],
    )];
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::LabelsUnsupported { .. }]
    ));
}

#[test]
fn an_associated_function_binds_by_name() {
    let mut items = vec![
        impl_block(
            "Point",
            vec![method("build", None, vec![implicit("x"), implicit("y")])],
        ),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(
                Expr::Path {
                    type_name: ident("Point"),
                    member: ident("build"),
                    span: span(),
                },
                vec![named("y", 2), named("x", 1)],
            ))],
        ),
    ];
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 2]);
}

#[test]
fn an_instance_method_binds_by_name() {
    let mut items = vec![
        impl_block(
            "Counter",
            vec![method(
                "step",
                Some(SelfParam::RefMut),
                vec![implicit("by"), implicit("cap")],
            )],
        ),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(
                Expr::FieldAccess {
                    object: Box::new(Expr::Identifier(ident("c"))),
                    field: ident("step"),
                    span: span(),
                },
                vec![named("cap", 9), named("by", 1)],
            ))],
        ),
    ];
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 9]);
}

#[test]
fn two_methods_of_one_name_with_different_parameters_reject_a_label() {
    let mut items = vec![
        impl_block(
            "A",
            vec![method("step", Some(SelfParam::Ref), vec![implicit("by")])],
        ),
        impl_block(
            "B",
            vec![method(
                "step",
                Some(SelfParam::Ref),
                vec![implicit("amount")],
            )],
        ),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(
                Expr::FieldAccess {
                    object: Box::new(Expr::Identifier(ident("b"))),
                    field: ident("step"),
                    span: span(),
                },
                vec![named("amount", 1)],
            ))],
        ),
    ];
    let errors = bind_arguments(&mut items).expect_err("expected a rejection");
    assert!(matches!(
        errors.as_slice(),
        [ArgumentError::AmbiguousMethodLabels { .. }]
    ));
}

#[test]
fn a_call_nested_in_an_argument_is_bound_too() {
    let mut items = vec![
        func("inner", vec![implicit("p"), implicit("q")], Vec::new()),
        func("target", vec![implicit("a")], Vec::new()),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(
                Expr::Identifier(ident("target")),
                vec![(
                    None,
                    call(
                        Expr::Identifier(ident("inner")),
                        vec![named("q", 2), named("p", 1)],
                    ),
                )],
            ))],
        ),
    ];
    bind_arguments(&mut items).expect("binding failed");
    let Some(Item::Function(main)) = items.last() else {
        panic!("expected a trailing function item");
    };
    let Some(Stmt::Expr(Expr::Call { args, .. })) = main.body.first() else {
        panic!("expected a call statement");
    };
    let Expr::Call {
        args: inner,
        arg_labels,
        ..
    } = &args[0]
    else {
        panic!("expected a nested call");
    };
    assert!(arg_labels.is_empty(), "a label survived binding");
    let values: Vec<i128> = inner
        .iter()
        .map(|a| match a {
            Expr::Literal(Literal::Integer(v, _), _) => *v,
            other => panic!("expected an integer, found {other:?}"),
        })
        .collect();
    assert_eq!(values, vec![1, 2]);
}

/// A call to `target` whose arguments are the given expressions under the given labels.
fn program_with_args(params: Vec<Parameter>, args: Vec<(Option<Identifier>, Expr)>) -> Vec<Item> {
    vec![
        func("target", params, Vec::new()),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(Expr::Identifier(ident("target")), args))],
        ),
    ]
}

/// `effect()` — a call, so an argument that carries one.
fn effect() -> Expr {
    Expr::Call {
        func: Box::new(Expr::Identifier(ident("effect"))),
        type_args: Vec::new(),
        args: Vec::new(),
        arg_labels: Vec::new(),
        span: span(),
    }
}

/// The statements of `main`'s single hoisting block, or a panic when it was not rewritten.
fn hoisted_block(items: &[Item]) -> Vec<Stmt> {
    let Some(Item::Function(main)) = items.last() else {
        panic!("expected a trailing function item");
    };
    match main.body.first() {
        Some(Stmt::Expr(Expr::Block { stmts, .. })) => stmts.clone(),
        other => panic!("expected a hoisting block, found {other:?}"),
    }
}

#[test]
fn a_reordered_call_with_effects_evaluates_its_arguments_in_source_order() {
    // `target(b: effect(), a: effect())` must run the argument written first, first —
    // permuting the two into declaration order would run them the other way round.
    let mut items = program_with_args(
        vec![implicit("a"), implicit("b")],
        vec![
            (Some(ident("b")), effect()),
            (Some(ident("a")), Expr::Identifier(ident("x"))),
        ],
    );
    bind_arguments(&mut items).expect("binding failed");

    let stmts = hoisted_block(&items);
    assert_eq!(stmts.len(), 3, "two temporaries and the call: {stmts:?}");
    let names: Vec<String> = stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::VarDecl { name, .. } => Some(name.name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        names,
        vec!["__narg0".to_string(), "__narg1".to_string()],
        "temporaries are declared in the order the arguments were written"
    );

    let Some(Stmt::Expr(Expr::Call {
        args, arg_labels, ..
    })) = stmts.last()
    else {
        panic!("the block must end in the bound call: {stmts:?}");
    };
    assert!(arg_labels.is_empty(), "a label survived binding");
    let passed: Vec<String> = args
        .iter()
        .map(|a| match a {
            Expr::Identifier(id) => id.name.clone(),
            other => panic!("expected a temporary, found {other:?}"),
        })
        .collect();
    assert_eq!(
        passed,
        vec!["__narg1".to_string(), "__narg0".to_string()],
        "the temporaries are passed in the callee's declaration order"
    );
}

#[test]
fn a_reordered_call_of_plain_values_is_permuted_in_place() {
    // Nothing can observe when a literal is evaluated, so the call keeps the shape — and
    // the identical IR — it has always had.
    let mut items = program_with_args(
        vec![implicit("a"), implicit("b")],
        vec![named("b", 2), named("a", 1)],
    );
    bind_arguments(&mut items).expect("binding failed");
    assert_eq!(bound_values(&items), vec![1, 2]);
}

#[test]
fn a_call_whose_arguments_keep_their_order_is_not_hoisted() {
    // `target(1, b: effect())` binds `b` to the argument already written second, so the
    // permutation moves nothing and there is nothing to hoist.
    let mut items = program_with_args(
        vec![implicit("a"), implicit("b")],
        vec![(None, int(1)), (Some(ident("b")), effect())],
    );
    bind_arguments(&mut items).expect("binding failed");
    let Some(Item::Function(main)) = items.last() else {
        panic!("expected a trailing function item");
    };
    assert!(
        matches!(main.body.first(), Some(Stmt::Expr(Expr::Call { .. }))),
        "the call must be left in place: {:?}",
        main.body.first()
    );
}

#[test]
fn a_hoisted_temporary_carries_its_parameter_type() {
    let mut items = program_with_args(
        vec![implicit("a"), implicit("b")],
        vec![
            (Some(ident("b")), effect()),
            (Some(ident("a")), Expr::Identifier(ident("x"))),
        ],
    );
    bind_arguments(&mut items).expect("binding failed");
    let stmts = hoisted_block(&items);
    let Some(Stmt::VarDecl { ty, .. }) = stmts.first() else {
        panic!("expected a temporary: {stmts:?}");
    };
    assert_eq!(
        ty.as_ref(),
        Some(&Type::Named(ident("i32"))),
        "the temporary is typed by the parameter it binds to, not by its initializer"
    );
}

#[test]
fn a_call_nested_in_a_hoisted_argument_is_bound_too() {
    // The walk must keep reaching into the temporaries it just created.
    let inner = call(
        Expr::Identifier(ident("inner")),
        vec![named("q", 2), named("p", 1)],
    );
    let mut items = vec![
        func("inner", vec![implicit("p"), implicit("q")], Vec::new()),
        func("target", vec![implicit("a"), implicit("b")], Vec::new()),
        func(
            "main",
            Vec::new(),
            vec![Stmt::Expr(call(
                Expr::Identifier(ident("target")),
                vec![
                    (Some(ident("b")), inner),
                    (Some(ident("a")), Expr::Identifier(ident("x"))),
                ],
            ))],
        ),
    ];
    bind_arguments(&mut items).expect("binding failed");
    let stmts = hoisted_block(&items);
    let Some(Stmt::VarDecl {
        init: Some(Expr::Call {
            args, arg_labels, ..
        }),
        ..
    }) = stmts.first()
    else {
        panic!("expected the nested call in the first temporary: {stmts:?}");
    };
    assert!(arg_labels.is_empty(), "a label survived binding");
    let values: Vec<i128> = args
        .iter()
        .map(|a| match a {
            Expr::Literal(Literal::Integer(v, _), _) => *v,
            other => panic!("expected an integer, found {other:?}"),
        })
        .collect();
    assert_eq!(values, vec![1, 2]);
}
