// `export` visibility markers on items and on struct fields.

use syntax_parsing::{parse, Item};

fn items(source: &str) -> Vec<Item> {
    parse(source).expect("source should parse")
}

fn parse_error(source: &str) -> String {
    parse(source)
        .expect_err("source should be rejected")
        .to_string()
}

#[test]
fn an_item_is_private_unless_it_carries_export() {
    let items = items("func hidden() -> i32 { 1 }\nexport func shown() -> i32 { 2 }\n");
    let exported: Vec<(&str, bool)> = items
        .iter()
        .filter_map(|item| match item {
            Item::Function(def) => Some((def.name.name.as_str(), def.exported)),
            _ => None,
        })
        .collect();
    assert_eq!(exported, vec![("hidden", false), ("shown", true)]);
}

#[test]
fn every_named_item_kind_takes_export() {
    let source = "export struct S { a: i32 }\n\
                  export enum E { A }\n\
                  export trait T { func run(&self) -> i32 }\n\
                  export const C: i32 = 1\n\
                  export newtype N = i32\n";
    let flags: Vec<bool> = items(source)
        .iter()
        .filter_map(|item| match item {
            Item::Struct(def) => Some(def.exported),
            Item::Enum(def) => Some(def.exported),
            Item::Trait(def) => Some(def.exported),
            Item::Const(def) => Some(def.exported),
            Item::Newtype(def) => Some(def.exported),
            _ => None,
        })
        .collect();
    assert_eq!(flags, vec![true, true, true, true, true]);
}

#[test]
fn a_struct_field_carries_its_own_visibility() {
    let items = items("export struct Config { export host: i32, timeout: i32 }\n");
    let Item::Struct(def) = &items[0] else {
        panic!("expected a struct item");
    };
    // An exported struct may still keep a field to itself, which is the whole point of
    // per-field visibility.
    assert!(def.exported);
    let fields: Vec<(&str, bool)> = def
        .fields
        .iter()
        .map(|f| (f.name.name.as_str(), f.exported))
        .collect();
    assert_eq!(fields, vec![("host", true), ("timeout", false)]);
}

#[test]
fn export_follows_an_attribute_rather_than_preceding_it() {
    let items = items("@derive(Copy, Clone)\nexport struct P { export x: i32 }\n");
    let Item::Struct(def) = &items[0] else {
        panic!("expected a struct item");
    };
    assert!(def.exported);
    assert_eq!(def.attributes.len(), 1);
}

#[test]
fn an_enum_struct_variant_field_follows_its_enum() {
    let items = items("export enum Shape { Circle { radius: i32 } }\n");
    let Item::Enum(def) = &items[0] else {
        panic!("expected an enum item");
    };
    let syntax_parsing::VariantPayload::Struct(fields) = &def.variants[0].payload else {
        panic!("expected a struct variant");
    };
    // A variant is reached through a pattern that names the enum, so its fields have no
    // visibility of their own.
    assert!(fields[0].exported);
}

#[test]
fn export_is_rejected_on_an_impl_block() {
    let message =
        parse_error("struct S { a: i32 }\nexport impl S { func f(&self) -> i32 { 1 } }\n");
    assert!(
        message.contains("`export` cannot be applied to"),
        "{}",
        message
    );
    assert!(message.contains("impl"), "{}", message);
}

#[test]
fn export_is_rejected_on_a_type_alias() {
    let message = parse_error("export type Count = i32\n");
    assert!(
        message.contains("`export` cannot be applied to"),
        "{}",
        message
    );
}

#[test]
fn export_is_rejected_on_an_import() {
    let message = parse_error("export import math\n");
    assert!(
        message.contains("`export` cannot be applied to"),
        "{}",
        message
    );
}
