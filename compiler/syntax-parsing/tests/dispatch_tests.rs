// Parsing of the two dispatch forms: `impl Trait` static-dispatch sugar and
// `dyn Trait` trait objects.

use syntax_parsing::{parse, GenericParamKind, Item, Type};

/// Argument-position `impl Trait` is anonymous-generic sugar, so the parser rewrites it
/// into a fresh trait-bounded type parameter and leaves an ordinary named annotation.
#[test]
fn impl_trait_argument_desugars_to_a_bounded_generic() {
    let source = r#"
func measure(s: &impl Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };

    assert_eq!(func.generics.len(), 1, "one anonymous type parameter");
    let param = &func.generics[0];
    assert!(matches!(param.kind, GenericParamKind::Type));
    assert_eq!(param.bounds.len(), 1);
    assert_eq!(param.bounds[0].name, "Shape");

    // The parameter annotation now names that generated parameter, behind the `&`.
    let Type::Reference { inner, .. } = &func.params[0].ty else {
        panic!("expected a reference parameter");
    };
    let Type::Named(ident) = inner.as_ref() else {
        panic!("expected the reference to name the generated parameter");
    };
    assert_eq!(ident.name, param.name.name);
}

/// Two `impl Trait` parameters are two independent anonymous parameters — not one shared
/// `<T>`, which is the whole point of the sugar.
#[test]
fn each_impl_trait_parameter_gets_its_own_type_parameter() {
    let source = r#"
func total(a: &impl Shape, b: &impl Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert_eq!(func.generics.len(), 2);
    assert_ne!(func.generics[0].name.name, func.generics[1].name.name);
}

/// An `impl Trait` parameter composes with explicitly declared generics rather than
/// replacing them.
#[test]
fn impl_trait_appends_to_explicit_generics() {
    let source = r#"
func mix<T>(a: T, b: &impl Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert_eq!(func.generics.len(), 2);
    assert_eq!(func.generics[0].name.name, "T");
    assert!(func.generics[0].bounds.is_empty());
    assert_eq!(func.generics[1].bounds[0].name, "Shape");
}

/// Return-position `impl Trait` is NOT desugared: it names one concrete type chosen by
/// the body, which only type resolution can determine, so the node survives parsing.
#[test]
fn impl_trait_return_is_preserved_and_adds_no_generic() {
    let source = r#"
func make() -> impl Shape { Square { side: 1 } }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert!(
        func.generics.is_empty(),
        "a return-position `impl Trait` is not a caller-inferred parameter"
    );
    let Some(Type::ImplTrait { trait_name, .. }) = &func.return_type else {
        panic!("expected the return type to stay an `impl Trait` node");
    };
    assert_eq!(trait_name.name, "Shape");
}

#[test]
fn dyn_trait_parses_behind_shared_and_mutable_references() {
    let source = r#"
func read(s: &dyn Shape) -> i32 { 0 }
func write(s: &mut dyn Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");

    let Item::Function(read) = &items[0] else {
        panic!("expected a function item");
    };
    let Type::Reference { inner, mutable, .. } = &read.params[0].ty else {
        panic!("expected a reference parameter");
    };
    assert!(!mutable);
    let Type::DynTrait { trait_name, .. } = inner.as_ref() else {
        panic!("expected a `dyn Trait` referent");
    };
    assert_eq!(trait_name.name, "Shape");

    let Item::Function(write) = &items[1] else {
        panic!("expected a function item");
    };
    let Type::Reference { inner, mutable, .. } = &write.params[0].ty else {
        panic!("expected a reference parameter");
    };
    assert!(mutable);
    assert!(matches!(inner.as_ref(), Type::DynTrait { .. }));
}

/// `dyn` produces no generic parameter — dynamic dispatch is one runtime type, not a
/// monomorphized family.
#[test]
fn dyn_trait_introduces_no_generic_parameter() {
    let source = r#"
func read(s: &dyn Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert!(func.generics.is_empty());
}

/// The parser accepts a bare `dyn Trait`; rejecting it as unsized is a type-resolution
/// rule, so the diagnostic can name the type rather than a syntax position.
#[test]
fn bare_dyn_trait_parses_and_defers_the_unsized_check() {
    let source = r#"
func read(s: dyn Shape) -> i32 { 0 }
"#;
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function item");
    };
    assert!(matches!(func.params[0].ty, Type::DynTrait { .. }));
}
