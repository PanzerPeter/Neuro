// Trait declaration parsing and default-method injection (§3.9).

use syntax_parsing::{parse, Item};

#[test]
fn parses_required_and_default_trait_methods() {
    let source = r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}
"#;
    let items = parse(source).expect("trait should parse");
    let Item::Trait(def) = &items[0] else {
        panic!("expected a trait item");
    };
    assert_eq!(def.name.name, "Describable");
    assert_eq!(def.methods.len(), 2);
    // The first method is required (no body); the second is a default (has a body).
    assert!(def.methods[0].default_body.is_none());
    assert!(def.methods[1].default_body.is_some());
}

#[test]
fn injects_default_methods_into_conforming_impls() {
    let source = r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
}
"#;
    let items = parse(source).expect("program should parse");
    let imp = items
        .iter()
        .find_map(|item| match item {
            Item::Impl(def) if def.type_name.name == "Widget" => Some(def),
            _ => None,
        })
        .expect("Widget impl present");
    // The impl wrote only `value`; the omitted default `doubled` must be injected.
    let names: Vec<&str> = imp.methods.iter().map(|m| m.name.name.as_str()).collect();
    assert!(names.contains(&"value"), "explicit method kept: {names:?}");
    assert!(
        names.contains(&"doubled"),
        "default method injected: {names:?}"
    );
}

#[test]
fn explicit_override_is_not_replaced_by_default() {
    let source = r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
    func doubled(&self) -> i32 { self.id }
}
"#;
    let items = parse(source).expect("program should parse");
    let imp = items
        .iter()
        .find_map(|item| match item {
            Item::Impl(def) if def.type_name.name == "Widget" => Some(def),
            _ => None,
        })
        .expect("Widget impl present");
    // Injection must not duplicate a method the implementor wrote explicitly.
    let doubled_count = imp
        .methods
        .iter()
        .filter(|m| m.name.name == "doubled")
        .count();
    assert_eq!(doubled_count, 1, "no duplicate `doubled` method injected");
}
