// `import` declaration and unqualified-variant pattern parsing.

use syntax_parsing::{Expr, ImportSelection, Item, Pattern, Stmt, parse};

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
fn parses_a_parent_relative_path() {
    let def = import("import ../../shared::config\n");
    assert!(def.relative);
    assert_eq!(def.parents, 2);
    let segments: Vec<&str> = def.path.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(segments, vec!["shared", "config"]);
}

#[test]
fn a_root_path_climbs_nothing() {
    let def = import("import ./utils\n");
    assert_eq!(def.parents, 0);
    assert!(!import("import utils\n").relative);
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
    let err = parse("import math::{}\n")
        .expect_err("should be rejected")
        .to_string();
    assert!(
        err.contains("expected at least one name inside an import list"),
        "{err}"
    );
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

#[test]
fn a_qualified_enum_pattern_folds_its_module_path_into_the_enum() {
    let items = parse("func f(s: i32) -> i32 {\n    match s {\n        shapes::Shape::Circle(r) => r,\n        _ => 0,\n    }\n}\n")
        .expect("a qualified pattern parses");
    let Item::Function(def) = &items[0] else {
        panic!("expected a function");
    };
    let pattern = def.body.iter().find_map(|stmt| match stmt {
        Stmt::Expr(Expr::Match { arms, .. }) => arms[0].patterns.first().cloned(),
        _ => None,
    });
    match pattern {
        Some(Pattern::Enum {
            enum_name, variant, ..
        }) => {
            assert_eq!(enum_name.name, "shapes::Shape");
            assert_eq!(variant.name, "Circle");
        }
        other => panic!("expected an enum pattern, got {:?}", other),
    }
}

#[test]
fn qualified_trait_names_parse_in_impl_dyn_and_bound_positions() {
    let source = "impl geo::Drawable for geo::Dot { }\nfunc f(d: &dyn geo::Drawable) { }\nfunc g<T: geo::Drawable>(t: T) -> impl geo::Drawable { t }\n";
    let items = parse(source).expect("qualified trait names parse");
    match &items[0] {
        Item::Impl(def) => {
            assert_eq!(
                def.trait_name.as_ref().map(|t| t.name.as_str()),
                Some("geo::Drawable")
            );
            assert_eq!(def.type_name.name, "geo::Dot");
        }
        other => panic!("expected an impl, got {:?}", other),
    }
    match &items[2] {
        Item::Function(def) => {
            assert_eq!(def.generics[0].bounds[0].trait_name.name, "geo::Drawable")
        }
        other => panic!("expected a function, got {:?}", other),
    }
}

#[test]
fn a_qualified_struct_literal_takes_a_base() {
    let items = parse("func f() {\n    val p = geo::Point { x: 1, ..geo::origin() }\n}\n")
        .expect("a qualified struct literal with a base parses");
    let Item::Function(def) = &items[0] else {
        panic!("expected a function");
    };
    match &def.body[0] {
        Stmt::VarDecl {
            init: Some(Expr::StructLiteral { name, base, .. }),
            ..
        } => {
            assert_eq!(name.name, "geo::Point");
            assert!(base.is_some());
        }
        other => panic!("expected a struct literal, got {:?}", other),
    }
}
