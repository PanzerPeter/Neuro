// Generic struct / impl / type-application parsing tests (§3.8)

use syntax_parsing::{parse, Item, Type};

#[test]
fn test_parse_generic_struct_definition() {
    let source = "struct Pair<T, U> { first: T, second: U }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Struct(def) => {
            assert_eq!(def.name.name, "Pair");
            assert_eq!(def.generics.len(), 2);
            assert_eq!(def.generics[0].name.name, "T");
            assert_eq!(def.generics[1].name.name, "U");
            assert_eq!(def.fields.len(), 2);
        }
        other => panic!("expected struct item, got {other:?}"),
    }
}

#[test]
fn test_parse_generic_type_application_annotation() {
    // `Pair<i32, f64>` as a parameter annotation is a generic type application.
    let source = "func f(p: Pair<i32, f64>) -> i32 { 0 }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => match &func.params[0].ty {
            Type::Generic { name, args, .. } => {
                assert_eq!(name.name, "Pair");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected a generic type application, got {other:?}"),
        },
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_parse_generic_impl_with_type_args() {
    let source = "impl<T> Wrapper<T> { func get(&self) -> T { self.value } }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Impl(def) => {
            assert_eq!(def.type_name.name, "Wrapper");
            assert_eq!(def.generics.len(), 1);
            assert_eq!(def.generics[0].name.name, "T");
            assert_eq!(def.type_args.len(), 1);
            assert!(matches!(&def.type_args[0], Type::Named(id) if id.name == "T"));
            assert_eq!(def.methods.len(), 1);
        }
        other => panic!("expected impl item, got {other:?}"),
    }
}

#[test]
fn test_non_generic_struct_has_empty_generics() {
    let items = parse("struct Point { x: f64, y: f64 }").expect("should parse");
    match &items[0] {
        Item::Struct(def) => assert!(def.generics.is_empty()),
        other => panic!("expected struct item, got {other:?}"),
    }
}
