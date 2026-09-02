// Trait declaration parsing and default-method injection.

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

#[test]
fn parses_operator_impl_with_associated_output_type() {
    // An operator-trait impl declares `type Output = T` alongside its method.
    let source = r#"
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }
impl Add for Vec2 {
    type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x + rhs.x, y: self.y + rhs.y } }
}
"#;
    let items = parse(source).expect("operator impl should parse");
    let imp = items
        .iter()
        .find_map(|item| match item {
            Item::Impl(def) if def.trait_name.as_ref().map(|t| t.name.as_str()) == Some("Add") => {
                Some(def)
            }
            _ => None,
        })
        .expect("Add impl present");
    assert_eq!(imp.assoc_types.len(), 1);
    assert_eq!(imp.assoc_types[0].0.name, "Output");
    assert_eq!(imp.methods.len(), 1);
    assert_eq!(imp.methods[0].name.name, "add");
}

#[test]
fn parses_an_associated_type_declaration_and_its_self_path() {
    let source = r#"
trait Iterator {
    type Item

    func next(&mut self) -> Option<Self::Item>
}
"#;
    let items = parse(source).expect("trait with an associated type should parse");
    let Item::Trait(def) = &items[0] else {
        panic!("expected a trait item");
    };
    assert_eq!(def.assoc_types.len(), 1);
    assert_eq!(def.assoc_types[0].name, "Item");
    assert_eq!(def.methods.len(), 1);
    let Some(syntax_parsing::Type::Generic { name, args, .. }) = &def.methods[0].return_type else {
        panic!("expected `Option<...>` as the return type");
    };
    assert_eq!(name.name, "Option");
    let [syntax_parsing::GenericArg::Type(syntax_parsing::Type::Named(item))] = &args[..] else {
        panic!("expected one type argument");
    };
    assert_eq!(item.name, "Self::Item");
}

#[test]
fn a_trait_may_not_bind_its_own_associated_type() {
    let source = r#"
trait Iterator {
    type Item = i32
}
"#;
    assert!(
        parse(source).is_err(),
        "`type Item = i32` is the impl's binding, not a trait declaration"
    );
}

#[test]
fn bare_self_is_not_a_type_annotation() {
    let source = r#"
struct Point { x: i32 }
impl Point {
    func me(&self) -> Self { Point { x: 1 } }
}
"#;
    assert!(parse(source).is_err(), "bare `Self` should be rejected");
}

#[test]
fn parses_an_associated_type_binding_in_a_bound() {
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

func head<T: Source<Item = i32>>(src: &T) -> i32 { 0 }
"#;
    let items = parse(source).expect("a constrained bound should parse");
    let func = items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) => Some(f),
            _ => None,
        })
        .expect("expected a function item");
    let bound = &func.generics[0].bounds[0];
    assert_eq!(bound.trait_name.name, "Source");
    let [(name, syntax_parsing::Type::Named(ty))] = &bound.assoc_bindings[..] else {
        panic!("expected one associated-type binding");
    };
    assert_eq!(name.name, "Item");
    assert_eq!(ty.name, "i32");
}

#[test]
fn a_where_clause_bound_takes_the_same_constraint() {
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

func head<T>(src: &T) -> i32 where T: Source<Item = i32> { 0 }
"#;
    let items = parse(source).expect("a constrained where bound should parse");
    let func = items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) => Some(f),
            _ => None,
        })
        .expect("expected a function item");
    let bound = &func.generics[0].bounds[0];
    assert_eq!(bound.trait_name.name, "Source");
    assert_eq!(bound.assoc_bindings.len(), 1);
    assert_eq!(bound.assoc_bindings[0].0.name, "Item");
}

#[test]
fn argument_position_impl_trait_carries_its_constraint_into_the_bound() {
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

func head(src: &impl Source<Item = i32>) -> i32 { 0 }
"#;
    let items = parse(source).expect("a constrained `impl Trait` argument should parse");
    let func = items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) => Some(f),
            _ => None,
        })
        .expect("expected a function item");
    // The desugar appends one anonymous parameter per `impl Trait` occurrence, and the
    // constraint has to survive it or the bound would say less than what was written.
    let bound = &func.generics[0].bounds[0];
    assert_eq!(bound.trait_name.name, "Source");
    assert_eq!(bound.assoc_bindings.len(), 1);
    assert_eq!(bound.assoc_bindings[0].0.name, "Item");
}

#[test]
fn a_bound_takes_no_positional_type_argument() {
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

func head<T: Source<i32>>(src: &T) -> i32 { 0 }
"#;
    assert!(
        parse(source).is_err(),
        "only `Assoc = T` entries constrain a bound; a positional argument names nothing"
    );
}
