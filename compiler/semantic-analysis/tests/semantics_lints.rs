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

fn float_cast_warnings(source: &str) -> Vec<semantic_analysis::Warning> {
    let items = syntax_parsing::parse(source).unwrap();
    type_check(&items)
        .expect("expected successful type check")
        .into_iter()
        .filter(|w| w.code == semantic_analysis::WarningCode::FloatCastOutOfRange)
        .collect()
}

#[test]
fn lint_constant_float_cast_out_of_range_warns() {
    for cast in [
        "1e20 as i32",
        "-1.5 as u8",
        "(-(129.5)) as i8",
        "256.0 as u8",
        "BIG as i32",
        "(BIG / 2.0) as i32",
        "(0.0 / 0.0) as i64",
        "4294967295.0f32 as u32",
        "9223372036854775807.0 as i64",
    ] {
        let source = format!(
            "const BIG: f64 = 1e10 * 10.0\nfunc test() -> i64 {{\n    val v = {cast}\n    0\n}}"
        );
        let warnings = float_cast_warnings(&source);
        assert_eq!(
            warnings.len(),
            1,
            "`{cast}` should warn once, got {warnings:?}"
        );
    }
}

#[test]
fn lint_in_range_or_run_time_float_cast_is_silent() {
    for cast in [
        "255.9 as u8",
        "-128.9 as i8",
        "-0.5 as u8",
        "SMALL as i32",
        "x as i8",
        "(x * 1e20) as i32",
        "4294967040.0f32 as u32",
    ] {
        let source = format!(
            "const SMALL: f64 = 3.0\nfunc test(x: f64) -> i64 {{\n    val v = {cast}\n    0\n}}"
        );
        let warnings = float_cast_warnings(&source);
        assert!(
            warnings.is_empty(),
            "`{cast}` should not warn, got {warnings:?}"
        );
    }
}

#[test]
fn lint_local_shadowing_a_const_is_not_a_constant() {
    let source = r#"
        const BIG: f64 = 1e20
        func test(x: f64) -> i32 {
            val BIG = x
            BIG as i32
        }
    "#;
    assert!(float_cast_warnings(source).is_empty());
}

#[test]
fn lint_allow_attribute_suppresses_float_cast_out_of_range() {
    let source = r#"
        @allow(float_cast_out_of_range)
        func quiet() -> i32 {
            1e20 as i32
        }

        struct S { v: i32 }
        impl S {
            @allow(float_cast_out_of_range)
            func quiet(&self) -> i32 {
                1e20 as i32
            }
            func loud(&self) -> i32 {
                1e20 as i32
            }
        }
    "#;
    assert_eq!(float_cast_warnings(source).len(), 1);
}
