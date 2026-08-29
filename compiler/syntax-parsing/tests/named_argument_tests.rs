// Parsing the two halves of named arguments: the external-label parameter forms
// at a declaration, and `label: expr` at a call site.
//
// The parser records what was written and nothing more — matching a label to a parameter
// needs the callee and happens in `argument-binding` — so these tests only assert on the
// shape the parser produces.

use syntax_parsing::{parse, parse_expr, Expr, Item, ParamLabel, Parameter, Stmt};

/// The parameters of the first function declared in `source`.
fn params_of(source: &str) -> Vec<Parameter> {
    let items = parse(source).expect("parse should succeed");
    let Some(Item::Function(def)) = items.into_iter().next() else {
        panic!("expected a function item");
    };
    def.params
}

#[test]
fn a_plain_parameter_carries_an_implicit_label() {
    let params = params_of("func f(value: i32) -> i32 { return value }");
    assert_eq!(params[0].name.name, "value");
    assert_eq!(params[0].label, ParamLabel::Implicit);
}

#[test]
fn a_second_identifier_makes_the_first_an_external_label() {
    let params = params_of("func f(min lo: i32) -> i32 { return lo }");
    assert_eq!(params[0].name.name, "lo");
    let ParamLabel::External(label) = &params[0].label else {
        panic!("expected an external label, got {:?}", params[0].label);
    };
    assert_eq!(label.name, "min");
}

#[test]
fn an_underscore_label_suppresses_the_call_site_name() {
    let params = params_of("func f(_ value: i32) -> i32 { return value }");
    assert_eq!(params[0].name.name, "value");
    assert_eq!(params[0].label, ParamLabel::Suppressed);
}

#[test]
fn the_three_parameter_forms_mix_in_one_signature() {
    let params = params_of(
        "func clamp(_ value: f32, min lo: f32, hi: f32) -> f32 { return value + lo + hi }",
    );
    assert_eq!(params.len(), 3);
    assert_eq!(params[0].label, ParamLabel::Suppressed);
    assert!(matches!(params[1].label, ParamLabel::External(_)));
    assert_eq!(params[2].label, ParamLabel::Implicit);
}

#[test]
fn a_method_parameter_may_carry_a_label() {
    let items = parse(
        "struct P { x: i32 }\n\
         impl P { func shift(&self, by delta: i32) -> i32 { return self.x + delta } }",
    )
    .expect("parse should succeed");
    let Some(Item::Impl(block)) = items.into_iter().nth(1) else {
        panic!("expected an impl item");
    };
    let params = &block.methods[0].params;
    assert_eq!(params[0].name.name, "delta");
    assert!(matches!(params[0].label, ParamLabel::External(_)));
}

#[test]
fn a_trait_method_parameter_may_carry_a_label() {
    let items =
        parse("trait T { func shift(&self, by delta: i32) -> i32 }").expect("parse should succeed");
    let Some(Item::Trait(def)) = items.into_iter().next() else {
        panic!("expected a trait item");
    };
    let params = &def.methods[0].params;
    assert_eq!(params[0].name.name, "delta");
    assert!(matches!(params[0].label, ParamLabel::External(_)));
}

#[test]
fn two_parameters_sharing_a_call_site_name_are_rejected() {
    let error = parse("func f(size a: i32, size b: i32) -> i32 { return a }")
        .expect_err("expected a parse error");
    assert!(
        error.to_string().contains("share the call-site name"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_call_records_the_label_of_each_named_argument() {
    let expr = parse_expr("f(1, port: 2, host: 3)").expect("parse should succeed");
    let Expr::Call {
        args, arg_labels, ..
    } = expr
    else {
        panic!("expected a call expression");
    };
    assert_eq!(args.len(), 3);
    let written: Vec<Option<&str>> = arg_labels
        .iter()
        .map(|l| l.as_ref().map(|i| i.name.as_str()))
        .collect();
    assert_eq!(written, vec![None, Some("port"), Some("host")]);
}

#[test]
fn a_call_that_names_nothing_carries_no_label_list() {
    let expr = parse_expr("f(1, 2, 3)").expect("parse should succeed");
    let Expr::Call { arg_labels, .. } = expr else {
        panic!("expected a call expression");
    };
    assert!(arg_labels.is_empty());
}

#[test]
fn a_qualified_path_argument_is_not_read_as_a_label() {
    // `::` is one token, so `Shape::Circle` can never be mistaken for `Shape:` plus a name.
    let expr = parse_expr("f(Shape::Circle)").expect("parse should succeed");
    let Expr::Call { arg_labels, .. } = expr else {
        panic!("expected a call expression");
    };
    assert!(arg_labels.is_empty());
}

#[test]
fn a_named_argument_may_be_an_arbitrary_expression() {
    let expr = parse_expr("f(scale: a * 2 + g(1))").expect("parse should succeed");
    let Expr::Call {
        args, arg_labels, ..
    } = expr
    else {
        panic!("expected a call expression");
    };
    assert_eq!(arg_labels.len(), 1);
    assert!(matches!(args[0], Expr::Binary { .. }));
}

#[test]
fn a_labelled_parameter_is_bound_by_its_internal_name_in_the_body() {
    // The body sees `lo`, never `min` — the external label is call-site-only.
    let items = parse(
        "func f(min lo: i32) -> i32 { return lo }\n\
         func main() -> i32 { return f(min: 1) }",
    )
    .expect("parse should succeed");
    let Some(Item::Function(def)) = items.into_iter().next() else {
        panic!("expected a function item");
    };
    let Some(Stmt::Return { value: Some(e), .. }) = def.body.first() else {
        panic!("expected a return statement");
    };
    assert!(matches!(e, Expr::Identifier(id) if id.name == "lo"));
}
