use super::{assert_payload_slot, function_body, lower};
use neuro_hir::{HirExprKind, HirStmt, HirType};

#[test]
fn match_lowers_to_resolved_tests_and_bindings() {
    use neuro_hir::HirMatchTest;

    let program = lower(
        r#"
enum Shape { Circle(i32), Rect { w: i32, h: i32 }, Unit }
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => r,
        Shape::Rect { w, h } => w + h,
        Shape::Unit => 0
    }
}
func classify(n: i32) -> i32 {
    match n {
        1 | 2 => 10,
        3..=9 => 20,
        _ => 0
    }
}
func main() -> i32 { area(Shape::Unit) + classify(1) }
"#,
    );

    let area = function_body(&program, "area");
    let HirStmt::Expr(m) = &area[0] else {
        panic!("area body should be a match expression statement");
    };
    let HirExprKind::Match { arms, .. } = &m.kind else {
        panic!("expected a match expression");
    };
    assert_eq!(arms.len(), 3);
    assert_eq!(m.ty, HirType::I32);

    // Circle(r): tag 0, one payload-slot binding of type i32.
    assert!(matches!(arms[0].tests[0], HirMatchTest::Tag { tag: 0 }));
    assert_eq!(arms[0].bindings.len(), 1);
    assert_eq!(arms[0].bindings[0].name, "r");
    assert_payload_slot(&arms[0].bindings[0], 0);
    // Rect { w, h }: tag 1, two payload-slot bindings.
    assert!(matches!(arms[1].tests[0], HirMatchTest::Tag { tag: 1 }));
    assert_eq!(arms[1].bindings.len(), 2);

    let classify = function_body(&program, "classify");
    let HirStmt::Expr(m) = &classify[0] else {
        panic!("classify body should be a match expression statement");
    };
    let HirExprKind::Match { arms, .. } = &m.kind else {
        panic!("expected a match expression");
    };
    // Or-pattern: two IntEq tests, no bindings.
    assert_eq!(arms[0].tests.len(), 2);
    assert!(matches!(arms[0].tests[0], HirMatchTest::IntEq { value: 1 }));
    assert!(matches!(arms[0].tests[1], HirMatchTest::IntEq { value: 2 }));
    // Inclusive range 3..=9.
    assert!(matches!(
        arms[1].tests[0],
        HirMatchTest::IntRange { lo: 3, hi: 9 }
    ));
    // Wildcard catch-all.
    assert!(matches!(arms[2].tests[0], HirMatchTest::Wildcard));
}

#[test]
fn an_exclusive_range_ending_past_i64_max_keeps_its_bits() {
    use neuro_hir::HirMatchTest;

    // `2^63` truncates to `i64::MIN`; the inclusive end one below it is `i64::MAX`, the
    // same bits a `u64` scrutinee compares against `2^63 - 1`.
    let program = lower(
        "func classify(x: u64) -> i32 { return match x { 0..9223372036854775808 => 1, _ => 2, } }
         func main() -> i32 { classify(5) }",
    );
    let HirStmt::Return { value: Some(m), .. } = &function_body(&program, "classify")[0] else {
        panic!("classify should return its match");
    };
    let HirExprKind::Match { arms, .. } = &m.kind else {
        panic!("expected a match expression");
    };
    assert!(matches!(
        arms[0].tests[0],
        HirMatchTest::IntRange {
            lo: 0,
            hi: i64::MAX
        }
    ));
}

#[test]
fn nested_payload_tests_follow_their_tag_and_binding_paths_reach_through() {
    use neuro_hir::{HirMatchTest, HirPatternStep};

    let program = lower(
        r#"
enum Inner { Num(i32), Blank }
enum Outer { Hold(Inner, i32), Bare }
func f(o: Outer) -> i32 {
    match o {
        Outer::Hold(Inner::Num(n), 7) => n,
        _ => 0
    }
}
func main() -> i32 { f(Outer::Bare) }
"#,
    );
    let body = function_body(&program, "f");
    let HirStmt::Expr(expr) = &body[0] else {
        panic!("expected a tail expression");
    };
    let HirExprKind::Match { arms, .. } = &expr.kind else {
        panic!("expected a match");
    };

    let HirMatchTest::All(tests) = &arms[0].tests[0] else {
        panic!("expected a conjunction: {:?}", arms[0].tests[0]);
    };
    assert_eq!(tests[0], HirMatchTest::Tag { tag: 0 });
    let HirMatchTest::Part { step, test } = &tests[1] else {
        panic!("expected a test of slot 0: {:?}", tests[1]);
    };
    assert!(matches!(step, HirPatternStep::EnumPayload { slot: 0, .. }));
    assert_eq!(**test, HirMatchTest::Tag { tag: 0 });
    assert!(matches!(
        &tests[2],
        HirMatchTest::Part { step: HirPatternStep::EnumPayload { slot: 1, .. }, test }
            if **test == HirMatchTest::IntEq { value: 7 }
    ));

    let [path] = arms[0].bindings[0].paths.as_slice() else {
        panic!("expected one alternative");
    };
    assert!(matches!(
        path.as_slice(),
        [
            HirPatternStep::EnumPayload { slot: 0, .. },
            HirPatternStep::EnumPayload { slot: 0, .. }
        ]
    ));
}

#[test]
fn a_binding_has_one_path_per_alternative_in_the_first_alternatives_order() {
    use neuro_hir::HirPatternStep;

    let program = lower(
        r#"
enum Two { A(i32, i32), B(i32, i32), C }
func f(t: Two) -> i32 {
    match t {
        Two::A(x, y) | Two::B(y, x) => x - y,
        Two::C => 0
    }
}
func main() -> i32 { f(Two::C) }
"#,
    );
    let body = function_body(&program, "f");
    let HirStmt::Expr(expr) = &body[0] else {
        panic!("expected a tail expression");
    };
    let HirExprKind::Match { arms, .. } = &expr.kind else {
        panic!("expected a match");
    };
    let slots = |name: &str| -> Vec<usize> {
        let binding = arms[0].bindings.iter().find(|b| b.name == name).unwrap();
        binding
            .paths
            .iter()
            .map(|path| match path.as_slice() {
                [HirPatternStep::EnumPayload { slot, .. }] => *slot,
                other => panic!("unexpected path {other:?}"),
            })
            .collect()
    };
    assert_eq!(arms[0].tests.len(), 2);
    assert_eq!(slots("x"), vec![0, 1]);
    assert_eq!(slots("y"), vec![1, 0]);
}

#[test]
fn a_slice_pattern_tests_length_before_its_elements() {
    use neuro_hir::{HirMatchTest, HirPatternStep};

    let program = lower(
        r#"
func f(s: &[i32]) -> i32 {
    match s {
        [1, ..rest] => rest.len() as i32,
        _ => 0
    }
}
func main() -> i32 { 0 }
"#,
    );
    let body = function_body(&program, "f");
    let HirStmt::Expr(expr) = &body[0] else {
        panic!("expected a tail expression");
    };
    let HirExprKind::Match { arms, .. } = &expr.kind else {
        panic!("expected a match");
    };
    let HirMatchTest::All(tests) = &arms[0].tests[0] else {
        panic!("expected a conjunction: {:?}", arms[0].tests[0]);
    };
    assert_eq!(
        tests[0],
        HirMatchTest::SliceLen {
            len: 1,
            exact: false
        }
    );
    assert!(matches!(
        &tests[1],
        HirMatchTest::Part {
            step: HirPatternStep::Element { index: 0, .. },
            ..
        }
    ));
    assert!(matches!(
        arms[0].bindings[0].paths[0].as_slice(),
        [HirPatternStep::Rest { start: 1, .. }]
    ));
}
