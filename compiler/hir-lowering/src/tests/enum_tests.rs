#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use crate::{lower_program, LoweringError};
use neuro_hir::{HirExprKind, HirItem, HirType};

#[test]
fn enum_construction_lowers_to_enum_construct() {
    // Each surface form normalizes to a single `EnumConstruct` node carrying
    // the variant's discriminant tag and a payload in declared field order. The
    // struct-variant form reorders provided fields into declaration order.
    let program = lower(
        "enum Shape { Circle { radius: f64 }, Rectangle { width: f64, height: f64 } }\n\
         enum Msg { Quit, Move(i32, i32) }\n\
         func main() -> i32 {\n\
            val a = Msg::Quit\n\
            val b = Msg::Move(1, 2)\n\
            val c = Shape::Rectangle { height: 3.0, width: 2.0 }\n\
            0\n\
         }",
    );
    let body = function_body(&program, "main");

    let a = binding_init(body, "a");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &a.kind else {
        panic!("unit variant should lower to EnumConstruct");
    };
    assert_eq!(*tag, 0);
    assert!(payload.is_empty());
    assert_eq!(a.ty, HirType::Enum("Msg".to_string()));

    let b = binding_init(body, "b");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &b.kind else {
        panic!("tuple variant should lower to EnumConstruct");
    };
    assert_eq!(*tag, 1);
    assert_eq!(payload.len(), 2);
    assert_eq!(payload[0].ty, HirType::I32);

    let c = binding_init(body, "c");
    let HirExprKind::EnumConstruct { tag, payload, .. } = &c.kind else {
        panic!("struct variant should lower to EnumConstruct");
    };
    // Rectangle is the second variant of Shape.
    assert_eq!(*tag, 1);
    // Fields are reordered into declaration order: width (2.0) then height (3.0).
    assert_eq!(payload.len(), 2);
    assert_eq!(c.ty, HirType::Enum("Shape".to_string()));
}

#[test]
fn generic_enum_monomorphizes_per_type_argument() {
    // Two distinct instantiations produce two ordinary enum items with concrete
    // payloads; the template itself is never emitted, so the backend sees no generics.
    let program = lower(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val a = Opt::Some(1)
    val b = Opt::Some(2.5)
    0
}
"#,
    );
    let mut names = enum_names(&program);
    names.sort();
    assert_eq!(
        names,
        vec!["Opt_g_f64".to_string(), "Opt_g_i32".to_string()]
    );
    assert!(
        !names.contains(&"Opt".to_string()),
        "the generic template must not be emitted"
    );

    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "a").ty,
        HirType::Enum("Opt_g_i32".to_string())
    );
    assert_eq!(
        binding_init(body, "b").ty,
        HirType::Enum("Opt_g_f64".to_string())
    );
}

#[test]
fn generic_enum_instance_carries_concrete_payload_types() {
    let program = lower(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val a: Opt<i64> = Opt::None
    0
}
"#,
    );
    let instance = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Enum(e) if e.name == "Opt_g_i64" => Some(e),
            _ => None,
        })
        .expect("the i64 instance should be emitted");
    assert_eq!(instance.variants.len(), 2);
    assert_eq!(instance.variants[0].fields[0].ty, HirType::I64);
    assert!(instance.variants[1].fields.is_empty());
}

#[test]
fn generic_enum_match_binds_the_instance_payload_type() {
    // The pattern names the base `Opt`, but the binding takes its type from the
    // scrutinee's monomorphized instance.
    let program = lower(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val o: Opt<i64> = Opt::Some(4i64)
    val n = match o {
        Opt::Some(v) => v,
        Opt::None => 0i64
    }
    0
}
"#,
    );
    let body = function_body(&program, "main");
    let matched = binding_init(body, "n");
    assert_eq!(matched.ty, HirType::I64);
    let HirExprKind::Match { arms, .. } = &matched.kind else {
        panic!("expected a match expression, got {:?}", matched.kind);
    };
    assert_eq!(arms[0].bindings[0].ty, HirType::I64);
}

#[test]
fn generic_enum_without_inferable_arguments_is_a_lowering_error() {
    // The checker rejects this program; lowering must surface it as an error rather
    // than panic, per the well-typedness contract.
    let ast = syntax_parsing::parse(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val x = Opt::None
    0
}
"#,
    )
    .expect("source should parse");
    assert!(matches!(
        lower_program(&ast),
        Err(LoweringError::UnresolvedType { .. })
    ));
}

#[test]
fn checked_intrinsic_lowers_to_an_option_instance() {
    // `checked_*` is the only builtin intrinsic whose result is a monomorphized enum,
    // so lowering must materialize the instance the backend then emits.
    let program = lower(
        r#"
enum Option<T> { Some(T), None }
func main() -> i32 {
    val a: u8 = 200
    val r = a.checked_add(100u8)
    0
}
"#,
    );
    let body = function_body(&program, "main");
    assert_eq!(
        binding_init(body, "r").ty,
        HirType::Enum("Option_g_u8".to_string())
    );
    assert!(
        program
            .items
            .iter()
            .any(|item| matches!(item, HirItem::Enum(e) if e.name == "Option_g_u8")),
        "the Option<u8> instance should be emitted for the backend"
    );
}
