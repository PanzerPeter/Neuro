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

#[test]
fn test_parse_const_generic_parameter() {
    // `const CAP: u32` is a const (value) parameter, distinguished by its kind.
    use syntax_parsing::GenericParamKind;
    let source = "struct Ring<T, const CAP: u32> { data: [T; CAP] }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Struct(def) => {
            assert_eq!(def.generics.len(), 2);
            assert!(matches!(def.generics[0].kind, GenericParamKind::Type));
            assert!(matches!(def.generics[1].kind, GenericParamKind::Const(_)));
            assert_eq!(def.generics[1].name.name, "CAP");
        }
        other => panic!("expected struct item, got {other:?}"),
    }
}

#[test]
fn test_parse_const_generic_array_size() {
    // The array length `CAP` parses as a const-parameter size, not a literal.
    use syntax_parsing::ArraySize;
    let source = "struct Ring<const CAP: u32> { data: [i32; CAP] }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Struct(def) => match &def.fields[0].ty {
            Type::Array { size, .. } => match size {
                ArraySize::Const(id) => assert_eq!(id.name, "CAP"),
                other => panic!("expected a const array size, got {other:?}"),
            },
            other => panic!("expected an array type, got {other:?}"),
        },
        other => panic!("expected struct item, got {other:?}"),
    }
}

#[test]
fn test_parse_where_clause_value_predicate() {
    let source = "func f<const N: u32>(a: [i32; N]) -> i32 where N > 0 { a[0] }";
    let items = parse(source).expect("should parse");
    match &items[0] {
        Item::Function(func) => {
            assert_eq!(func.where_predicates.len(), 1);
        }
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn test_parse_turbofish_call() {
    // `identity::<i32>(x)` attaches an explicit type argument to the call.
    use syntax_parsing::{Expr, GenericArg, Stmt};
    let source = "func main() -> i32 { identity::<i32>(5) }";
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected function")
    };
    let Some(Stmt::Expr(Expr::Call { type_args, .. })) = func.body.last() else {
        panic!("expected a trailing call")
    };
    assert_eq!(type_args.len(), 1);
    assert!(matches!(type_args[0], GenericArg::Type(_)));
}

#[test]
fn test_parse_turbofish_const_argument() {
    // A turbofish may carry a const (integer) argument.
    use syntax_parsing::{Expr, GenericArg, Stmt};
    let source = "func main() -> i32 { zeros::<4>() }";
    let items = parse(source).expect("should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected function")
    };
    let Some(Stmt::Expr(Expr::Call { type_args, .. })) = func.body.last() else {
        panic!("expected a trailing call")
    };
    assert_eq!(type_args.len(), 1);
    assert!(matches!(type_args[0], GenericArg::Const { value: 4, .. }));
}
