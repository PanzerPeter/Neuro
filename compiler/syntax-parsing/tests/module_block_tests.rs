// Inline `module Name { ... }` block parsing.

use syntax_parsing::{parse, Item, ModuleDef};

fn block(source: &str) -> ModuleDef {
    let items = parse(source).expect("module block should parse");
    match &items[0] {
        Item::Module(def) => def.clone(),
        other => panic!("expected a module item, got {:?}", other),
    }
}

fn parse_error(source: &str) -> String {
    parse(source)
        .expect_err("source should be rejected")
        .to_string()
}

fn item_names(items: &[Item]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Function(def) => Some(def.name.name.as_str()),
            Item::Struct(def) => Some(def.name.name.as_str()),
            Item::Module(def) => Some(def.name.name.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn parses_a_block_and_its_items() {
    let def = block(
        "module geometry {\n\
             export struct Circle { export radius: f64 }\n\
             export func area(c: &Circle) -> f64 { c.radius }\n\
             func validate(r: f64) -> bool { r > 0.0 }\n\
         }\n",
    );
    assert_eq!(def.name.name, "geometry");
    assert_eq!(item_names(&def.items), vec!["Circle", "area", "validate"]);
}

#[test]
fn a_block_item_carries_its_own_export_marker() {
    let def =
        block("module m {\nexport func shown() -> i32 { 1 }\nfunc hidden() -> i32 { 2 }\n}\n");
    let exported: Vec<(&str, bool)> = def
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Function(f) => Some((f.name.name.as_str(), f.exported)),
            _ => None,
        })
        .collect();
    assert_eq!(exported, vec![("shown", true), ("hidden", false)]);
}

#[test]
fn blocks_nest() {
    let def = block("module outer {\nmodule inner {\nexport func deep() -> i32 { 1 }\n}\n}\n");
    assert_eq!(item_names(&def.items), vec!["inner"]);
    let Item::Module(inner) = &def.items[0] else {
        panic!("expected a nested module")
    };
    assert_eq!(item_names(&inner.items), vec!["deep"]);
}

#[test]
fn an_empty_block_parses() {
    let def = block("module m { }\n");
    assert!(def.items.is_empty());
}

#[test]
fn a_type_alias_declared_beside_a_block_reads_inside_it() {
    // Aliases are expanded at parse time, so they stay file-scoped rather than stopping
    // at the block's brace.
    let def = block("module m {\nexport func f(v: Count) -> i32 { v }\n}\ntype Count = i32\n");
    let Item::Function(f) = &def.items[0] else {
        panic!("expected a function")
    };
    let syntax_parsing::Type::Named(ty) = &f.params[0].ty else {
        panic!("expected a named type, got {:?}", f.params[0].ty)
    };
    assert_eq!(ty.name, "i32", "alias should have been expanded");
}

#[test]
fn an_unterminated_block_is_rejected() {
    let message = parse_error("module m {\nexport func f() -> i32 { 1 }\n");
    assert!(
        message.contains("'}' to close the module block"),
        "{}",
        message
    );
}

#[test]
fn a_block_without_a_name_is_rejected() {
    let message = parse_error("module { }\n");
    assert!(
        message.contains("module name after 'module'"),
        "{}",
        message
    );
}
