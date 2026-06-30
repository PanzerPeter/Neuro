// Enum declaration and struct-variant literal parsing (§3.5).

use syntax_parsing::{parse, Expr, Item, Stmt, VariantPayload};

#[test]
fn parses_all_three_variant_payload_shapes() {
    let source = r#"
enum Msg {
    Quit,
    Move(i32, i32),
    Write { text: bool, count: u32 }
}
"#;
    let items = parse(source).expect("enum should parse");
    let Item::Enum(def) = &items[0] else {
        panic!("expected an enum item");
    };
    assert_eq!(def.name.name, "Msg");
    assert_eq!(def.variants.len(), 3);

    assert_eq!(def.variants[0].name.name, "Quit");
    assert!(matches!(def.variants[0].payload, VariantPayload::Unit));

    assert_eq!(def.variants[1].name.name, "Move");
    let VariantPayload::Tuple(tys) = &def.variants[1].payload else {
        panic!("Move should be a tuple variant");
    };
    assert_eq!(tys.len(), 2);

    assert_eq!(def.variants[2].name.name, "Write");
    let VariantPayload::Struct(fields) = &def.variants[2].payload else {
        panic!("Write should be a struct variant");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name.name, "text");
    assert_eq!(fields[1].name.name, "count");
}

#[test]
fn parses_struct_variant_literal() {
    // `Enum::Variant { field: value }` parses to the dedicated EnumStructLiteral node.
    let source = r#"
func main() -> i32 {
    val s = Shape::Circle { radius: 5.0 }
    0
}
"#;
    let items = parse(source).expect("program should parse");
    let Item::Function(func) = &items[0] else {
        panic!("expected a function");
    };
    let Stmt::VarDecl {
        init: Some(init), ..
    } = &func.body[0]
    else {
        panic!("expected a val binding");
    };
    let Expr::EnumStructLiteral {
        enum_name,
        variant,
        fields,
        ..
    } = init
    else {
        panic!("expected an EnumStructLiteral, got {init:?}");
    };
    assert_eq!(enum_name.name, "Shape");
    assert_eq!(variant.name, "Circle");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name.name, "radius");
}
