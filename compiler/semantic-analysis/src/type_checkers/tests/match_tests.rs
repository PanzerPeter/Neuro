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
            .any(|e| matches!(e, TypeError::OrPatternBindingMissing { .. })),
        "a name one alternative leaves out must be rejected; got {errors:?}"
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
        "matching on a string must be rejected; got {errors:?}"
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

/// The errors `body` raises, with the declarations these tests share in scope.
fn pattern_errors(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!(
        r#"
enum Opt<T> {{ Some(T), None }}
enum Tok {{ Num(i32), Neg(i32), Text(string), End }}
enum Flags {{ Two(bool, bool), Off }}
{body}
func main() -> i32 {{ 0 }}
"#
    ))
}

fn non_exhaustive_reason(body: &str) -> String {
    let errors = pattern_errors(body);
    let [TypeError::NonExhaustiveMatch { reason, .. }] = errors.as_slice() else {
        panic!("expected one non-exhaustive error; got {errors:?}");
    };
    reason.clone()
}

#[test]
fn alternatives_bind_the_same_names_at_the_same_types() {
    let ok = pattern_errors(
        "func f(t: Tok) -> i32 { match t { Tok::Num(x) | Tok::Neg(x) => x, _ => 0 } }",
    );
    assert!(ok.is_empty(), "got {ok:?}");

    let missing = pattern_errors(
        "func f(t: Tok) -> i32 { match t { Tok::Num(x) | Tok::Neg(y) => 1, _ => 0 } }",
    );
    assert!(
        missing
            .iter()
            .any(|e| matches!(e, TypeError::OrPatternBindingMissing { .. })),
        "got {missing:?}"
    );

    let typed = pattern_errors(
        "func f(t: Tok) -> i32 { match t { Tok::Num(x) | Tok::Text(x) => 1, _ => 0 } }",
    );
    assert!(
        typed
            .iter()
            .any(|e| matches!(e, TypeError::OrPatternBindingType { .. })),
        "got {typed:?}"
    );
}

#[test]
fn exhaustiveness_looks_inside_payloads() {
    let covered = pattern_errors(
        "func f(o: Opt<Opt<i32>>) -> i32 { match o { Opt::Some(Opt::Some(x)) => x, Opt::Some(Opt::None) => 1, Opt::None => 0 } }
         func g(t: Flags) -> i32 { match t {
             Flags::Two(true, _) => 1, Flags::Two(false, true) => 2, Flags::Two(false, false) => 3, Flags::Off => 0
         } }",
    );
    assert!(covered.is_empty(), "got {covered:?}");

    assert_eq!(
        non_exhaustive_reason(
            "func f(o: Opt<Opt<i32>>) -> i32 { match o { Opt::Some(Opt::Some(x)) => x, Opt::None => 0 } }"
        ),
        "no arm matches `Opt::Some(Opt::None)`"
    );
    assert_eq!(
        non_exhaustive_reason(
            "func f(t: Flags) -> i32 { match t { Flags::Two(true, _) => 1, Flags::Two(_, true) => 2, Flags::Off => 0 } }"
        ),
        "no arm matches `Flags::Two(false, false)`"
    );
    // A literal says nothing about the rest of an integer.
    assert_eq!(
        non_exhaustive_reason(
            "func f(o: Opt<i32>) -> i32 { match o { Opt::Some(1) => 1, Opt::None => 0 } }"
        ),
        "no arm matches `Opt::Some(_)`"
    );
}

#[test]
fn a_guarded_nested_arm_covers_nothing() {
    assert_eq!(
        non_exhaustive_reason(
            "func f(o: Opt<bool>) -> i32 { match o { Opt::Some(true) if 1 > 0 => 1, Opt::Some(false) => 2, Opt::None => 0 } }"
        ),
        "no arm matches `Opt::Some(true)`"
    );
}

#[test]
fn exhaustiveness_counts_slice_lengths_and_array_elements() {
    let covered = pattern_errors(
        "func f(s: &[bool]) -> i32 { match s { [] => 0, [true, ..] => 1, [false, ..r] => 2 } }
         func g(a: [bool; 2]) -> i32 { match a { [true, ..] => 1, [false, _] => 2 } }",
    );
    assert!(covered.is_empty(), "got {covered:?}");

    assert_eq!(
        non_exhaustive_reason("func f(s: &[i32]) -> i32 { match s { [] => 0, [a] => a } }"),
        "no arm matches `[_, _, ..]`"
    );
    assert_eq!(
        non_exhaustive_reason("func f(s: &[i32]) -> i32 { match s { [_, ..] => 0 } }"),
        "no arm matches `[]`"
    );
    assert_eq!(
        non_exhaustive_reason("func f(a: [bool; 2]) -> i32 { match a { [true, _] => 1 } }"),
        "no arm matches `[false, _]`"
    );
}

#[test]
fn a_match_too_wide_to_verify_is_an_error_not_a_hang() {
    // Every row pins two neighbours of a wide `bool` payload, and the two constant rows
    // cover what they miss, so a matrix walk has to visit an exponential number of cases
    // before it can say the match is exhaustive.
    const WIDTH: usize = 40;
    let fields = vec!["bool"; WIDTH].join(", ");
    let mut rows: Vec<String> = (0..WIDTH)
        .map(|i| {
            let mut cells = vec!["_"; WIDTH];
            cells[i] = "true";
            cells[(i + 1) % WIDTH] = "false";
            cells.join(", ")
        })
        .collect();
    rows.push(vec!["true"; WIDTH].join(", "));
    rows.push(vec!["false"; WIDTH].join(", "));
    let arms: Vec<String> = rows.iter().map(|r| format!("W::V({r}) => 1,")).collect();
    let body = format!(
        "enum W {{ V({fields}) }}\nfunc f(w: W) -> i32 {{ match w {{ {} }} }}",
        arms.join("\n")
    );
    assert_eq!(
        non_exhaustive_reason(&body),
        "these patterns are too complex to verify"
    );
}

#[test]
fn an_at_binding_of_an_owner_leaves_its_pattern_nothing_to_bind() {
    let copied = pattern_errors(
        "func f(o: Opt<i32>) -> i32 { match o { w @ Opt::Some(v) => v, Opt::None => 0 } }",
    );
    assert!(copied.is_empty(), "got {copied:?}");

    let tested = pattern_errors(
        "func f(o: Opt<string>) -> i32 { match o { w @ Opt::Some(_) => 1, Opt::None => 0 } }",
    );
    assert!(tested.is_empty(), "got {tested:?}");

    let moved = pattern_errors(
        "func f(o: Opt<Vec<i32>>) -> i32 { match o { w @ Opt::Some(v) => 1, Opt::None => 0 } }",
    );
    assert!(
        moved
            .iter()
            .any(|e| matches!(e, TypeError::AtPatternBindsOwner { .. })),
        "got {moved:?}"
    );
}

#[test]
fn nothing_that_owns_leaves_a_borrow_a_nested_pattern_looks_through() {
    let copied = pattern_errors(
        "func f(o: Opt<&Opt<i32>>) -> i32 { match o { Opt::Some(Opt::Some(n)) => n, _ => 0 } }",
    );
    assert!(copied.is_empty(), "got {copied:?}");

    let owner = pattern_errors(
        "func f(o: Opt<&Opt<Vec<i32>>>) -> i32 { match o { Opt::Some(Opt::Some(v)) => 1, _ => 0 } }",
    );
    assert!(
        owner
            .iter()
            .any(|e| matches!(e, TypeError::CannotMoveOutOfBorrow { .. })),
        "got {owner:?}"
    );

    let element =
        pattern_errors("func f(s: &[string]) -> i32 { match s { [a, ..] => 1, [] => 0 } }");
    assert!(
        element
            .iter()
            .any(|e| matches!(e, TypeError::CannotMoveOutOfBorrow { .. })),
        "got {element:?}"
    );
}

#[test]
fn array_patterns_fit_the_array_they_match() {
    for pattern in ["[x]", "[x, y, z]", "[x, y, z, ..]"] {
        let errors = pattern_errors(&format!(
            "func f(a: [i32; 2]) -> i32 {{ match a {{ {pattern} => 1, _ => 0 }} }}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::ArrayPatternLengthMismatch { .. })),
            "{pattern}: got {errors:?}"
        );
    }

    let scalar = pattern_errors("func f(n: i32) -> i32 { match n { [x] => 1, _ => 0 } }");
    assert!(
        scalar
            .iter()
            .any(|e| matches!(e, TypeError::PatternTypeMismatch { .. })),
        "got {scalar:?}"
    );
}

#[test]
fn a_pattern_cannot_test_a_float_or_a_string() {
    for (ty, literal) in [("f64", "1.5"), ("string", "\"x\"")] {
        let errors = pattern_errors(&format!(
            "func f(o: Opt<{ty}>) -> i32 {{ match o {{ Opt::Some({literal}) => 1, _ => 0 }} }}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::UntestableLiteralPattern { .. })),
            "{ty}: got {errors:?}"
        );
    }
}
