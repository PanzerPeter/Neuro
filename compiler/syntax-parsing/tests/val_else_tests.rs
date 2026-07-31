// Parsing for the `val PATTERN = value else |binding| { ... }` statement.

use syntax_parsing::{parse, EnumPatternPayload, Item, Pattern, Stmt};

/// The statements of the first function body in `source`.
fn first_fn_body(source: &str) -> Vec<Stmt> {
    let items = parse(source).expect("source should parse");
    for item in &items {
        if let Item::Function(func) = item {
            return func.body.clone();
        }
    }
    panic!("no function found");
}

/// The sole `val-else` in the first function body.
fn only_val_else(source: &str) -> Stmt {
    let body = first_fn_body(source);
    body.into_iter()
        .find(|stmt| matches!(stmt, Stmt::ValElse { .. }))
        .expect("expected a val-else statement")
}

#[test]
fn parses_a_tuple_variant_pattern_with_an_else_binding() {
    let stmt = only_val_else(
        r#"
        func test() {
            val Result::Ok(data) = parse(raw) else |err| { return err }
        }
    "#,
    );

    let Stmt::ValElse {
        pattern,
        else_binding,
        else_block,
        ..
    } = stmt
    else {
        panic!("expected a val-else");
    };

    let Pattern::Enum {
        enum_name,
        variant,
        payload,
        ..
    } = pattern
    else {
        panic!("expected an enum pattern");
    };
    assert_eq!(enum_name.name, "Result");
    assert_eq!(variant.name, "Ok");
    assert!(matches!(payload, EnumPatternPayload::Tuple(subs) if subs.len() == 1));

    assert_eq!(
        else_binding.map(|b| b.name),
        Some("err".to_string()),
        "the `|err|` after `else` is an else-binding, not a closure"
    );
    assert_eq!(else_block.len(), 1);
    assert!(matches!(else_block[0], Stmt::Return { .. }));
}

#[test]
fn parses_without_an_else_binding() {
    let stmt = only_val_else(
        r#"
        func test() {
            val Option::Some(config) = load() else { return 0 }
        }
    "#,
    );
    let Stmt::ValElse { else_binding, .. } = stmt else {
        panic!("expected a val-else");
    };
    assert!(else_binding.is_none());
}

#[test]
fn parses_a_wildcard_else_binding() {
    // `|_|` is a written binding that names nothing — distinct from omitting the
    // form entirely, which is what lets the Option rule reject `|name|` but accept this.
    let stmt = only_val_else(
        r#"
        func test() {
            val Option::Some(v) = load() else |_| { return 0 }
        }
    "#,
    );
    let Stmt::ValElse { else_binding, .. } = stmt else {
        panic!("expected a val-else");
    };
    assert_eq!(else_binding.map(|b| b.name), Some("_".to_string()));
}

#[test]
fn parses_a_struct_variant_pattern() {
    let stmt = only_val_else(
        r#"
        func test() {
            val Shape::Circle { radius } = shape else |s| { return 0 }
        }
    "#,
    );
    let Stmt::ValElse { pattern, .. } = stmt else {
        panic!("expected a val-else");
    };
    let Pattern::Enum { payload, .. } = pattern else {
        panic!("expected an enum pattern");
    };
    let EnumPatternPayload::Struct(fields) = payload else {
        panic!("expected a struct-variant payload");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field.name, "radius");
}

#[test]
fn an_ordinary_val_is_still_a_var_decl() {
    // The `Name::` head is the only marker; a plain binding must not be diverted.
    let body = first_fn_body(
        r#"
        func test() {
            val x = 1
            val y: i32 = 2
        }
    "#,
    );
    assert_eq!(body.len(), 2);
    assert!(body.iter().all(|s| matches!(s, Stmt::VarDecl { .. })));
}

#[test]
fn a_struct_destructure_is_still_a_destructure() {
    // `val Point { x, y } = p` shares the brace shape with a struct-variant pattern
    // but has no `::`, so it must keep desugaring to a temp plus one bind per field.
    let body = first_fn_body(
        r#"
        func test() {
            val Point { x, y } = p
        }
    "#,
    );
    assert!(body.iter().all(|s| !matches!(s, Stmt::ValElse { .. })));
    assert_eq!(body.len(), 3, "temp + x + y");
}

#[test]
fn rejects_a_val_else_without_an_else_branch() {
    let err = parse(
        r#"
        func test() {
            val Option::Some(v) = load()
        }
    "#,
    );
    assert!(err.is_err(), "a `val-else` pattern requires an `else`");
}
