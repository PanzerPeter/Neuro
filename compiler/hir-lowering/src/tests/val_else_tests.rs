//! `val-else` lowering: the resolved success test, the enclosing-scope bindings, and
//! the type-directed `else` binding.

use super::*;
use neuro_hir::{HirBindingSource, HirMatchTest, HirType};

/// The two fallible enums, declared exactly as the prelude declares them.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

/// The sole `val-else` statement in `body`.
fn val_else(body: &[HirStmt]) -> &HirStmt {
    body.iter()
        .find(|stmt| matches!(stmt, HirStmt::ValElse { .. }))
        .expect("expected a lowered val-else")
}

#[test]
fn lowers_to_a_tag_test_with_payload_bindings() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
func parse(n: i32) -> Result<i32, i32> {{ Result::Ok(n) }}

func main() -> i32 {{
    val Result::Ok(data) = parse(1) else |err| {{ return err }}
    data
}}"
    ));

    let HirStmt::ValElse {
        test,
        bindings,
        else_binding,
        else_block,
        ..
    } = val_else(function_body(&program, "main"))
    else {
        panic!("expected a val-else");
    };

    // `Ok` is variant 0 of `Result`.
    assert_eq!(*test, HirMatchTest::Tag { tag: 0 });

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].name, "data");
    assert_eq!(bindings[0].ty, HirType::I32);
    assert_eq!(
        bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    );

    // `Result` binds the `Err` payload, which is also slot 0 — of the other variant.
    let binding = else_binding.as_ref().expect("expected an else binding");
    assert_eq!(binding.name, "err");
    assert_eq!(binding.ty, HirType::I32);
    assert_eq!(binding.source, HirBindingSource::EnumPayload { slot: 0 });

    assert_eq!(else_block.len(), 1);
    assert!(matches!(else_block[0], HirStmt::Return { .. }));
}

#[test]
fn an_option_else_binding_lowers_to_nothing() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
func lookup(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    val Option::Some(v) = lookup(1) else |_| {{ return 0 }}
    v
}}"
    ));
    let HirStmt::ValElse { else_binding, .. } = val_else(function_body(&program, "main")) else {
        panic!("expected a val-else");
    };
    assert!(
        else_binding.is_none(),
        "`Option::None` carries no payload to bind"
    );
}

#[test]
fn a_plain_enum_else_binding_lowers_to_the_whole_scrutinee() {
    let program = lower(
        "enum Shape { Circle { radius: i32 }, Square(i32), Empty }

func main() -> i32 {
    val Shape::Circle { radius } = Shape::Square(3) else |s| {
        match s {
            Shape::Square(side) => { return side },
            _ => { return 0 }
        }
    }
    radius
}",
    );
    let HirStmt::ValElse {
        test, else_binding, ..
    } = val_else(function_body(&program, "main"))
    else {
        panic!("expected a val-else");
    };
    assert_eq!(*test, HirMatchTest::Tag { tag: 0 });

    let binding = else_binding.as_ref().expect("expected an else binding");
    assert_eq!(binding.name, "s");
    assert_eq!(binding.ty, HirType::Enum("Shape".to_string()));
    assert_eq!(binding.source, HirBindingSource::Scrutinee);
}

#[test]
fn the_bindings_type_later_statements_in_the_block() {
    // The binding is registered in the enclosing scope, so the `val` after it takes
    // its type from the payload rather than falling back to a default width.
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
func parse(n: i32) -> Result<i64, i32> {{ Result::Ok(1i64) }}

func main() -> i32 {{
    val Result::Ok(data) = parse(1) else |err| {{ return err }}
    val echoed = data
    0
}}"
    ));
    let body = function_body(&program, "main");
    let init = binding_init(body, "echoed");
    assert_eq!(init.ty, HirType::I64);
}
