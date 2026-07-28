#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
use neuro_hir::{HirExprKind, HirStmt, HirType};

#[test]
fn match_lowers_to_resolved_tests_and_bindings() {
    use neuro_hir::{HirBindingSource, HirMatchTest};

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
    assert!(matches!(
        arms[0].bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    ));
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
