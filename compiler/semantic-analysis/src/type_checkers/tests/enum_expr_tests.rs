use super::super::*;
use super::*;

#[test]
fn enum_construction_forms_type_check() {
    // All three variant forms construct, and an enum flows as a binding,
    // a function parameter/return, and a struct field.
    let errors = semantic_errors(
        r#"
enum Color { Red, Green, Blue }
enum Shape { Circle { radius: f64 }, Rectangle { width: f64, height: f64 } }
enum Msg { Quit, Move(i32, i32) }

struct Tagged { kind: Color, n: i32 }

func make() -> Msg { Msg::Move(1, 2) }
func take(s: Shape) -> i32 { 0 }

func main() -> i32 {
    val c = Color::Red
    val s = Shape::Circle { radius: 5.0 }
    val r = Shape::Rectangle { width: 2.0, height: 3.0 }
    val m = make()
    val t = Tagged { kind: Color::Green, n: take(s) }
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "valid enum program; got {errors:?}");
}

#[test]
fn unknown_enum_variant_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Color { Red, Green }
func main() -> i32 {
    val c = Color::Blue
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownEnumVariant { .. })),
        "an undeclared variant must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_variant_form_mismatch_is_rejected() {
    // A struct variant constructed with call syntax is a form error.
    let errors = semantic_errors(
        r#"
enum Shape { Circle { radius: f64 } }
func main() -> i32 {
    val s = Shape::Circle(5.0)
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::EnumVariantFormMismatch { .. })),
        "constructing a struct variant with `(...)` must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_tuple_variant_arity_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Msg { Move(i32, i32) }
func main() -> i32 {
    val m = Msg::Move(1)
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::EnumVariantArityMismatch { .. })),
        "a tuple variant built with the wrong arity must be rejected; got {errors:?}"
    );
}

#[test]
fn enum_struct_variant_field_type_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Shape { Circle { radius: f64 } }
func main() -> i32 {
    val s = Shape::Circle { radius: true }
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "a struct-variant field of the wrong type must be rejected; got {errors:?}"
    );
}

#[test]
fn non_scalar_enum_payload_is_rejected() {
    // Phase 1E — payloads are scalar Copy primitives; a string payload is rejected.
    let errors = semantic_errors(
        r#"
enum Bad { Holds(string) }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnsupportedEnumPayload { .. })),
        "a non-scalar payload must be rejected; got {errors:?}"
    );
}
