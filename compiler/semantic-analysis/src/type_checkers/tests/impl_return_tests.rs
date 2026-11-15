use super::semantic_errors;
use crate::errors::TypeError;

const SHAPES: &str = r#"
trait Shape { func area(&self) -> i32 }
struct Sq { side: i32 }
impl Shape for Sq { func area(&self) -> i32 { self.side * self.side } }
func base(n: i32) -> Sq { Sq { side: n } }
struct Twice<T> { inner: T }
impl<T: Shape> Shape for Twice<T> { func area(&self) -> i32 { self.inner.area() * 2 } }
"#;

fn errors_with_shapes(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{SHAPES}{body}\nfunc main() -> i32 {{ 0 }}\n"))
}

fn not_inferable(errors: &[TypeError]) -> usize {
    errors
        .iter()
        .filter(|e| matches!(e, TypeError::ImplReturnNotInferable { .. }))
        .count()
}

// The prelude is not loaded here, so ranges and adapters are the suite's to cover; a
// generic wrapper stands in for an adapter.
#[test]
fn a_call_result_a_binding_and_a_generic_wrapper_resolve() {
    let errors = errors_with_shapes(
        r#"
func via_call(n: i32) -> impl Shape { base(n) }
func via_val(n: i32) -> impl Shape {
    val s = base(n)
    s
}
func doubled(n: i32) -> impl Shape { Twice { inner: via_call(n) } }
func use_them() -> i32 { via_call(2).area() + via_val(3).area() + doubled(1).area() }
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// A body may call an `impl Trait` function declared after it: the unresolved one waits
/// for the round after its callee resolves.
#[test]
fn an_impl_return_calling_a_later_one_resolves() {
    let errors = errors_with_shapes(
        r#"
func outer(n: i32) -> impl Shape { Twice { inner: inner(n) } }
func inner(n: i32) -> impl Shape { base(n) }
func use_it() -> i32 { outer(2).area() }
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_body_depending_on_its_own_result_is_not_inferable() {
    let direct = errors_with_shapes(
        "func f(n: i32) -> impl Shape { if n == 0 { base(1) } else { f(n - 1) } }",
    );
    assert_eq!(not_inferable(&direct), 1, "got {direct:?}");
    let mutual = errors_with_shapes(
        "func a(n: i32) -> impl Shape { b(n) }\nfunc b(n: i32) -> impl Shape { a(n) }",
    );
    assert_eq!(not_inferable(&mutual), 2, "got {mutual:?}");
}

#[test]
fn a_body_ending_in_no_value_is_not_inferable_and_reports_nothing_else() {
    let errors = errors_with_shapes(
        "func f() -> impl Shape {\n    mut i = 0\n    while i < 3 { i += 1 }\n}",
    );
    assert_eq!(errors.len(), 1, "got {errors:?}");
    assert_eq!(not_inferable(&errors), 1, "got {errors:?}");
}

/// A body with an error of its own reports that error once, from pass 4, and not a
/// second time as an inference failure.
#[test]
fn a_body_error_is_reported_once_and_alone() {
    let errors =
        errors_with_shapes("func f() -> impl Shape {\n    val x: i32 = true\n    base(x)\n}");
    assert_eq!(errors.len(), 1, "got {errors:?}");
    assert!(
        matches!(errors[0], TypeError::Mismatch { .. }),
        "got {errors:?}"
    );
}

/// An instance reports its errors once, when it is made. One made by a check that
/// failed and was thrown away must be made again by pass 4, or its error is lost.
#[test]
fn an_instance_made_by_a_failed_check_reports_its_error() {
    let errors = errors_with_shapes(
        r#"
struct NoClone { n: i32 }
@derive(Clone)
struct W<T> { t: T }
func a() -> impl Shape {
    val w = W { t: NoClone { n: 1 } }
    b()
}
func b() -> impl Shape { base(2) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.to_string().contains("cannot derive `Clone`")),
        "got {errors:?}"
    );
}

#[test]
fn an_early_return_of_another_type_is_a_mismatch() {
    let errors = errors_with_shapes(
        r#"
struct Blob { n: i32 }
func f(n: i32) -> impl Shape {
    if n == 0 { return Blob { n: 1 } }
    base(n)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })),
        "got {errors:?}"
    );
}
