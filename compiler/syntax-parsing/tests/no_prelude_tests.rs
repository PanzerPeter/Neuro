// Parser tests for the `@no_prelude` file marker: where it is accepted, and the
// diagnostic for every position that is not the top of a file.

use syntax_parsing::{parse, Item, ParseError};

#[test]
fn the_marker_parses_at_the_top_of_a_file() {
    let items = parse("@no_prelude\n\nfunc main() -> i32 { 0 }\n").expect("program parses");

    assert!(matches!(items.first(), Some(Item::NoPrelude(_))));
    assert!(matches!(items.get(1), Some(Item::Function(_))));
}

#[test]
fn the_marker_is_rejected_after_a_declaration() {
    let error = parse("func main() -> i32 { 0 }\n@no_prelude\n").expect_err("misplaced marker");

    assert!(matches!(error, ParseError::MisplacedNoPrelude { .. }));
}

#[test]
fn the_marker_is_rejected_inside_an_inline_module_block() {
    let error =
        parse("module inner {\n@no_prelude\nfunc value() -> i32 { 1 }\n}\n").expect_err("rejected");

    assert!(matches!(error, ParseError::MisplacedNoPrelude { .. }));
}

#[test]
fn the_marker_may_not_be_written_twice() {
    let error = parse("@no_prelude\n@no_prelude\nfunc main() -> i32 { 0 }\n")
        .expect_err("the second marker is misplaced");

    assert!(matches!(error, ParseError::MisplacedNoPrelude { .. }));
}

#[test]
fn an_ordinary_attribute_still_attaches_to_its_declaration() {
    let items = parse("@derive(Copy, Clone)\nstruct Point { x: i32 }\n").expect("program parses");

    match items.first() {
        Some(Item::Struct(def)) => assert_eq!(def.attributes.len(), 1),
        other => panic!("expected a struct with its attribute, got {:?}", other),
    }
}
