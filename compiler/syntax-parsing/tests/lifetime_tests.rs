// Explicit lifetime annotation parsing tests

use syntax_parsing::{Item, Type, parse};

#[test]
fn test_parse_function_lifetime_param() {
    // `<'a>` is a lifetime parameter; it lands in `lifetimes`, not `generics`.
    let source = "func longest<'a>(a: &'a string, b: &'a string) -> &'a string { a }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => {
            assert!(
                func.generics.is_empty(),
                "lifetimes must not become generics"
            );
            assert_eq!(func.lifetimes.len(), 1);
            assert_eq!(func.lifetimes[0].name, "a");
        }
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_parse_reference_type_lifetime_annotation() {
    let source = "func f<'a>(a: &'a string) -> i32 { 0 }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => match &func.params[0].ty {
            Type::Reference {
                lifetime, mutable, ..
            } => {
                assert!(!mutable);
                let lt = lifetime.as_ref().expect("lifetime annotation present");
                assert_eq!(lt.name, "a");
            }
            other => panic!("expected a reference type, got {other:?}"),
        },
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_parse_mut_reference_with_lifetime() {
    // Order after `&`: lifetime then `mut`.
    let source = "func f<'a>(a: &'a mut i32) -> i32 { 0 }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => match &func.params[0].ty {
            Type::Reference {
                lifetime, mutable, ..
            } => {
                assert!(mutable);
                assert_eq!(lifetime.as_ref().expect("lifetime").name, "a");
            }
            other => panic!("expected a reference type, got {other:?}"),
        },
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_parse_lifetimes_mixed_with_type_params() {
    let source = "func f<'a, T>(x: T, r: &'a string) -> T { x }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.lifetimes.len(), 1);
            assert_eq!(func.lifetimes[0].name, "a");
            assert_eq!(func.generics.len(), 1);
            assert_eq!(func.generics[0].name.name, "T");
        }
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_duplicate_lifetime_is_rejected() {
    let source = "func f<'a, 'a>(a: &'a string) -> i32 { 0 }";
    let err = parse(source)
        .expect_err("duplicate lifetime must be rejected")
        .to_string();
    assert!(err.contains("duplicate parameter name ''a'"), "{err}");
}

#[test]
fn test_struct_and_impl_lifetime_params_parse() {
    let source = "impl<'a> Wrapper { func get(&self) -> i32 { 0 } }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Impl(def) => {
            assert_eq!(def.lifetimes.len(), 1);
            assert_eq!(def.lifetimes[0].name, "a");
            assert!(def.generics.is_empty());
        }
        other => panic!("expected impl item, got {other:?}"),
    }
}

#[test]
fn regression_lim_205_amp_amp_is_two_borrows_in_a_type() {
    // `&&` lexes as logical-and; in a type it can only mean `& &`, and the second
    // `&` takes the lifetime and `mut`.
    let source = "func f<'a>(a: &&'a mut i32) -> i32 { 0 }";
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected function item");
    };
    let Type::Reference {
        inner,
        mutable: false,
        lifetime: None,
        span,
    } = &func.params[0].ty
    else {
        panic!(
            "expected an outer shared borrow, got {:?}",
            func.params[0].ty
        );
    };
    assert_eq!(&source[span.start..span.end], "&&'a mut i32");
    let Type::Reference {
        mutable: true,
        lifetime: Some(lt),
        span: inner_span,
        ..
    } = inner.as_ref()
    else {
        panic!("expected an inner `&'a mut`, got {inner:?}");
    };
    assert_eq!(lt.name, "a");
    assert_eq!(&source[inner_span.start..inner_span.end], "&'a mut i32");
}

#[test]
fn regression_lim_205_amp_amp_is_two_borrows_in_prefix_position() {
    use syntax_parsing::{Expr, parse_expr};
    let Expr::Reference {
        operand,
        mutable: false,
        ..
    } = parse_expr("&&mut x").expect("should parse")
    else {
        panic!("expected an outer shared borrow");
    };
    assert!(matches!(*operand, Expr::Reference { mutable: true, .. }));
    // Infix `&&` is still logical-and.
    assert!(matches!(
        parse_expr("a && b").expect("should parse"),
        Expr::Binary { .. }
    ));
}

fn param_type(source: &str, index: usize) -> Type {
    let items = parse(source).expect("should parse");
    let Some(Item::Function(func)) = items.into_iter().last() else {
        panic!("expected the last item to be a function");
    };
    func.params[index].ty.clone()
}

#[test]
fn lifetime_arguments_are_checked_then_dropped() {
    let decls = "struct H<'a> { r: &'a string }\nstruct P<'a, T> { r: &'a T }\n";
    // Lifetimes alone leave a plain named type.
    let only = param_type(&format!("{decls}func f<'a>(h: H<'a>) -> i32 {{ 0 }}"), 0);
    assert!(
        matches!(&only, Type::Named(id) if id.name == "H"),
        "{only:?}"
    );
    // Mixed with a type argument, the type argument alone stays.
    let mixed = param_type(
        &format!("{decls}func f<'a>(p: P<'a, i32>) -> i32 {{ 0 }}"),
        0,
    );
    let Type::Generic { args, .. } = &mixed else {
        panic!("expected a generic application, got {mixed:?}");
    };
    assert_eq!(args.len(), 1);
    // The anonymous lifetime needs no declaration.
    let anonymous = param_type(&format!("{decls}func f(h: &H<'_>) -> i32 {{ 0 }}"), 0);
    assert!(matches!(anonymous, Type::Reference { .. }));
}

#[test]
fn lifetime_argument_resolves_through_impl_and_method_scopes() {
    let source = "struct H<'a> { r: &'a string }
impl<'a> H<'a> {
    func pick<'b>(&self, other: H<'b>) -> H<'a> { H { r: self.r } }
}";
    let items = parse(source).expect("should parse");
    let Item::Impl(def) = &items[1] else {
        panic!("expected an impl item");
    };
    assert!(def.type_args.is_empty(), "{:?}", def.type_args);
}

#[test]
fn undeclared_lifetime_argument_is_rejected() {
    let err = parse("struct H<'a> { r: &'a string }\nfunc f(h: H<'z>) -> i32 { 0 }")
        .expect_err("an undeclared lifetime argument must be rejected")
        .to_string();
    assert!(err.contains("undeclared lifetime `'z`"), "{err}");
    // A lifetime a finished item declared is out of scope in the next one.
    let err = parse("func g<'a>(s: &'a string) -> i32 { 0 }\nstruct H<'a> { r: &'a string }\nfunc f(h: H<'a>) -> i32 { 0 }")
        .expect_err("a lifetime from an earlier item must be out of scope")
        .to_string();
    assert!(err.contains("undeclared lifetime `'a`"), "{err}");
}
