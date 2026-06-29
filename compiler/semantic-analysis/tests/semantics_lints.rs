// Integration tests: Lints

use semantic_analysis::type_check;

#[test]
fn lint_while_true_emits_prefer_loop_warning() {
    use semantic_analysis::WarningCode;

    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        while true {
            if i == 3 {
                break
            }
            i = i + 1
        }
        return i
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert_eq!(
        warnings.len(),
        1,
        "expected one lint warning, got {:?}",
        warnings
    );
    assert_eq!(warnings[0].code, WarningCode::PreferLoopOverWhileTrue);
}

#[test]
fn lint_allow_attribute_suppresses_while_true() {
    let source = r#"
        @allow(prefer_loop_over_while_true)
        func test() -> i32 {
            mut i: i32 = 0
            while true {
                if i == 3 {
                    break
                }
                i = i + 1
            }
            return i
        }
    "#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert!(
        warnings.is_empty(),
        "@allow should suppress the lint, got {:?}",
        warnings
    );
}

#[test]
fn lint_parenthesised_while_true_not_flagged() {
    let source = r#"func test() -> i32 {
        mut i: i32 = 0
        while (true) {
            if i == 3 {
                break
            }
            i = i + 1
        }
        return i
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert!(
        warnings.is_empty(),
        "parenthesised condition is the explicit escape hatch; got {:?}",
        warnings
    );
}

#[test]
fn lint_while_false_not_flagged() {
    let source = r#"func test() -> i32 {
        while false {
            return 1
        }
        return 0
    }"#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert!(warnings.is_empty(), "while false should not lint");
}

#[test]
fn lint_while_true_inside_method_is_flagged() {
    let source = r#"
        struct Counter { value: i32 }

        impl Counter {
            func tick(&self) -> i32 {
                mut i: i32 = 0
                while true {
                    if i == 1 { break }
                    i = i + 1
                }
                i
            }
        }
    "#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert_eq!(warnings.len(), 1);
}

#[test]
fn lint_allow_on_method_suppresses_while_true() {
    let source = r#"
        struct Counter { value: i32 }

        impl Counter {
            @allow(prefer_loop_over_while_true)
            func tick(&self) -> i32 {
                mut i: i32 = 0
                while true {
                    if i == 1 { break }
                    i = i + 1
                }
                i
            }
        }
    "#;

    let items = syntax_parsing::parse(source).unwrap();
    let warnings = type_check(&items).expect("expected successful type check");
    assert!(warnings.is_empty());
}
