// `import` declaration and unqualified-variant pattern parsing.

use syntax_parsing::{parse, Expr, ImportSelection, Item, Pattern, Stmt};

fn import(source: &str) -> syntax_parsing::ImportDef {
    let items = parse(source).expect("import should parse");
    match &items[0] {
        Item::Import(def) => def.clone(),
        other => panic!("expected an import item, got {:?}", other),
    }
}

#[test]
fn parses_a_bare_module_import() {
    let def = import("import math\n");
    assert!(!def.relative);
    assert_eq!(def.path.len(), 1);
    assert_eq!(def.path[0].name, "math");
    assert!(matches!(def.selection, ImportSelection::Module));
}

#[test]
fn parses_a_nested_relative_path() {
    let def = import("import ./utils::io\n");
    assert!(def.relative);
    let segments: Vec<&str> = def.path.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(segments, vec!["utils", "io"]);
}

#[test]
fn parses_a_brace_list_with_a_renamed_entry() {
    let def = import("import math::{sqrt, sin as sine}\n");
    let ImportSelection::List(names) = &def.selection else {
        panic!("expected a list selection");
    };
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].name.name, "sqrt");
    assert!(names[0].alias.is_none());
    assert_eq!(names[1].name.name, "sin");
    assert_eq!(
        names[1].alias.as_ref().map(|a| a.name.as_str()),
        Some("sine")
    );
}

#[test]
fn parses_a_path_alias() {
    let def = import("import math::matrix as mat\n");
    let ImportSelection::Alias(alias) = &def.selection else {
        panic!("expected an alias selection");
    };
    assert_eq!(alias.name, "mat");
    let segments: Vec<&str> = def.path.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(segments, vec!["math", "matrix"]);
}

#[test]
fn parses_a_variant_import() {
    let def = import("import Option::{Some, None}\n");
    let ImportSelection::List(names) = &def.selection else {
        panic!("expected a list selection");
    };
    let bound: Vec<&str> = names.iter().map(|n| n.name.name.as_str()).collect();
    assert_eq!(bound, vec!["Some", "None"]);
}

#[test]
fn an_empty_import_list_is_rejected() {
    assert!(parse("import math::{}\n").is_err());
}

#[test]
fn a_cast_is_still_read_as_a_cast_after_an_import() {
    // `as` marks a rename in an import and a cast in an expression; only a following
    // name makes it a rename, so the two never compete.
    let source = r#"
import math

func main() -> i64 {
    val n = 1
    n as i64
}
"#;
    let items = parse(source).expect("program should parse");
    assert_eq!(items.len(), 2);
}

#[test]
fn a_payload_carrying_variant_pattern_parses_without_its_enum() {
    let source = r#"
func main() -> i32 {
    match value {
        Some(n) => n,
        _ => 0
    }
}
"#;
    let items = parse(source).expect("program should parse");
    let Item::Function(def) = &items[0] else {
        panic!("expected a function");
    };
    let Some(Stmt::Expr(Expr::Match { arms, .. })) = def.body.first() else {
        panic!("expected a match expression");
    };
    match &arms[0].patterns[0] {
        Pattern::UnqualifiedEnum { variant, .. } => assert_eq!(variant.name, "Some"),
        other => panic!("expected an unqualified variant pattern, got {:?}", other),
    }
}
