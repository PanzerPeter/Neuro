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

/// An arm that binds an owner by value takes it out of the scrutinee, so a second
/// `match` on the same binding reads a moved value. It used to free the payload twice.
#[test]
fn regression_a_match_binding_an_owner_moves_the_scrutinee() {
    let errors = semantic_errors(
        r#"
enum Bag { Items(Vec<i32>), Empty }
func main() -> i32 {
    val b = Bag::Empty
    val x = match b { Bag::Items(v) => v.len(), Bag::Empty => 0 }
    val y = match b { Bag::Items(v) => v.len(), Bag::Empty => 0 }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "got {errors:?}"
    );
}

/// A name an arm declares shadows an outer binding only inside the arm, so an arm's
/// tail naming it does not move the outer one.
#[test]
fn an_arm_local_tail_does_not_move_an_outer_binding_of_the_same_name() {
    let errors = semantic_errors(
        r#"
enum Bag { Items(Vec<i32>), Empty }
func main() -> i32 {
    val v: Vec<i32> = Vec::new()
    val b = Bag::Empty
    val w = match b { Bag::Items(v) => v, Bag::Empty => Vec::new() }
    val z = {
        val v: Vec<i32> = Vec::new()
        v
    }
    return (v.len() + w.len() + z.len()) as i32
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the outer `v` is untouched; got {errors:?}"
    );
}
