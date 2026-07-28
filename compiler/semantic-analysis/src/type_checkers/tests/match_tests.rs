use super::super::*;
use super::*;

#[test]
fn match_all_pattern_forms_type_check() {
    // Enum unit/tuple/struct variants, literal, or-pattern, range, guard,
    // and wildcard patterns all type-check in one exhaustive match.
    let errors = semantic_errors(
        r#"
enum Shape { Circle(i32), Rect { w: i32, h: i32 }, Unit }
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => r * r,
        Shape::Rect { w, h } => w * h,
        Shape::Unit => 0
    }
}
func classify(n: i32) -> i32 {
    match n {
        0 => 1,
        1 | 2 => 2,
        3..=9 => 3,
        n if n < 0 => 4,
        _ => 9
    }
}
func main() -> i32 { area(Shape::Unit) + classify(5) }
"#,
    );
    assert!(errors.is_empty(), "valid match program; got {errors:?}");
}

#[test]
fn non_exhaustive_enum_match_is_rejected() {
    let errors = semantic_errors(
        r#"
enum E { A, B, C }
func f(e: E) -> i32 {
    match e {
        E::A => 1,
        E::B => 2
    }
}
func main() -> i32 { f(E::A) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonExhaustiveMatch { .. })),
        "a match missing variant C must be rejected; got {errors:?}"
    );
}

#[test]
fn integer_match_without_wildcard_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 => 1,
        1 => 2
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonExhaustiveMatch { .. })),
        "an integer match needs a `_` arm; got {errors:?}"
    );
}

#[test]
fn match_arm_type_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 => 1,
        _ => true
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MatchArmTypeMismatch { .. })),
        "arms with incompatible body types must be rejected; got {errors:?}"
    );
}

#[test]
fn or_pattern_binding_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(n: i32) -> i32 {
    match n {
        0 | x => x,
        _ => 0
    }
}
func main() -> i32 { f(0) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::OrPatternBinding { .. })),
        "a binding in an or-pattern must be rejected; got {errors:?}"
    );
}

#[test]
fn match_on_unsupported_scrutinee_is_rejected() {
    let errors = semantic_errors(
        r#"
func f(s: string) -> i32 {
    match s {
        _ => 0
    }
}
func main() -> i32 { f("x") }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnsupportedMatchScrutinee { .. })),
        "matching on a string must be rejected in phase 1E; got {errors:?}"
    );
}
