// Explicit lifetime annotation parsing tests

use syntax_parsing::{parse, Item, Type};

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
    assert!(
        parse(source).is_err(),
        "duplicate lifetime must be rejected"
    );
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
