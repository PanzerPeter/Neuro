#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn generic_enum_tuple_variant_infers_its_type_argument() {
    // `Opt::Some(7)` determines `T = i32` from its payload, and the match arm binds the
    // payload at that concrete type, so `v + 1` is i32 arithmetic.
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val o = Opt::Some(7)
    match o {
        Opt::Some(v) => v + 1,
        Opt::None => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_unit_variant_takes_its_arguments_from_the_annotation() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val none: Opt<i32> = Opt::None
    match none {
        Opt::Some(v) => v,
        Opt::None => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_partial_payload_completes_from_the_return_type() {
    // `Res::Err(1)` binds only `E`; `T` comes from the declared return type, which is
    // the only context a tail `if` branch has.
    let errors = semantic_errors(
        r#"
enum Res<T, E> { Ok(T), Err(E) }
func divide(a: i32, b: i32) -> Res<i32, i32> {
    if b == 0 { Res::Err(1) } else { Res::Ok(a / b) }
}
func main() -> i32 {
    match divide(9, 3) {
        Res::Ok(v) => v,
        Res::Err(e) => e
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_unit_variant_without_context_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val x = Opt::None
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericEnumNotInferable { .. })),
        "a unit variant with nothing to infer from must be rejected; got {errors:?}"
    );
}

#[test]
fn bare_generic_enum_without_arguments_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func take(o: Opt) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericEnumNeedsArgs { .. })),
        "a generic enum used without type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn non_scalar_generic_enum_payload_is_rejected() {
    // The phase-wide scalar-payload restriction is re-checked per instance, so
    // `Opt<string>` is rejected even though the template itself is fine.
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val s: Opt<string> = Opt::None
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnsupportedEnumPayload { .. })),
        "a non-scalar payload instance must be rejected; got {errors:?}"
    );
}

#[test]
fn generic_enum_instances_are_distinct_types() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func take(o: Opt<i64>) -> i32 { 0 }
func main() -> i32 {
    val narrow: Opt<i32> = Opt::None
    return take(narrow)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "two instances of one generic enum must not be interchangeable; got {errors:?}"
    );
}

#[test]
fn generic_enum_struct_variant_infers_from_its_fields() {
    let errors = semantic_errors(
        r#"
enum Shape<T> { Circle { radius: T }, Empty }
func main() -> i32 {
    val c = Shape::Circle { radius: 2 }
    match c {
        Shape::Circle { radius } => radius,
        Shape::Empty => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}
