#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn generic_call_type_checks_and_infers_return_type() {
    // A well-formed generic function and its inferable call are accepted; the call's
    // result flows into a matching return with no error.
    let errors = semantic_errors(
        r#"
func identity<T>(x: T) -> T { x }
func main() -> i32 { return identity(41) }
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_body_operation_without_bound_is_rejected() {
    // A bare `T` has no `+` without a trait bound (the trait system does not exist yet).
    let errors = semantic_errors("func bad<T>(a: T, b: T) -> T { a + b }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })),
        "arithmetic on an unbounded generic must be rejected; got {errors:?}"
    );
}

#[test]
fn returning_concrete_value_as_type_parameter_is_rejected() {
    // Returning a concrete `string` where the type parameter `T` is expected is a
    // mismatch. (A parameter used only in return position is now permitted at the
    // declaration (turbofish may supply it) and is instead reported at the call
    // site when it cannot be inferred; see the const-generics test suite.)
    let errors = semantic_errors("func p<T>(s: string) -> T { s }");
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })),
        "returning a concrete value as `T` must be reported; got {errors:?}"
    );
}

#[test]
fn non_inferable_generic_param_is_rejected_at_call_site() {
    // `U` appears in no parameter, so an un-turbofished call cannot bind it.
    let errors = semantic_errors(
        r#"
func firstof<T, U>(x: T) -> T { x }
func main() -> i32 { firstof(5) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericParamNotInferable { .. })),
        "a call that cannot bind a type parameter must be reported; got {errors:?}"
    );
}

#[test]
fn non_copy_generic_argument_is_accepted() {
    // A type argument carries no `Copy` requirement: the template was move-checked
    // against an abstract `T` that is conservatively non-`Copy`, so it stays valid
    // whichever type the call site supplies.
    let errors = semantic_errors(
        r#"
func identity<T>(x: T) -> T { x }
func main() -> i32 { val s = identity("hi")
    return 0 }
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn use_after_move_through_an_abstract_type_is_rejected() {
    // The abstract body is checked once, so `T` must answer for its non-`Copy`
    // instantiations: binding `v` twice would move one owner twice at `T = string`.
    let errors = semantic_errors(
        r#"
func dup<T>(v: T) -> T { val a = v
    val b = v
    b }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "a second move of an abstract-typed binding must be reported; got {errors:?}"
    );
}

#[test]
fn closure_capturing_an_abstract_type_is_rejected() {
    // A capture duplicates the binding into the closure, so every call would run the
    // destructor of one owner again once `T` was instantiated with a `Drop` type.
    let errors = semantic_errors(
        r#"
func hold<T>(v: T) -> i32 { val f = |x: i32| -> i32 { val held = v
        x }
    f(1) }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ClosureCapturesNonCopy { .. })),
        "capturing an abstract-typed binding must be reported; got {errors:?}"
    );
}

#[test]
fn array_literal_of_an_abstract_element_is_rejected() {
    // `[v, v]` duplicates the owner the same way a capture does; an abstract element
    // type is conservatively non-`Copy`, so the second slot is a move of a moved value.
    let errors = semantic_errors(
        r#"
func dup<T>(v: T) -> i32 { val a = [v, v]
    0 }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "an array literal over an abstract element type must be reported; got {errors:?}"
    );
}

#[test]
fn an_abstract_element_in_a_signature_still_resolves() {
    // The deferral half of the rule: `[T; N]` in a signature is re-validated at each
    // call, where the caller's own annotation for the argument is what carries `Copy`.
    let errors = semantic_errors(
        r#"
func first<T>(a: [T; 3]) -> T { a[0] }
func main() -> i32 { val xs: [i32; 3] = [1, 2, 3]
    return first(xs) }
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_argument_count_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func pair<T, U>(a: T, b: U) -> T { a }
func main() -> i32 { return pair(1) }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })),
        "a wrong-arity generic call must be reported; got {errors:?}"
    );
}

#[test]
fn generic_struct_literal_infers_and_field_access_types() {
    // A generic struct literal infers its type arguments; a field read yields the
    // concrete field type, so `p.first + 1` type-checks as i32 arithmetic.
    let errors = semantic_errors(
        r#"
struct Pair<T, U> { first: T, second: U }
func main() -> i32 {
    val p = Pair { first: 10, second: 2.5 }
    return p.first + 1
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_impl_method_dispatches_on_instance() {
    // A generic inherent impl's method returns the concrete element type.
    let errors = semantic_errors(
        r#"
struct Wrapper<T> { value: T }
impl<T> Wrapper<T> { func get(&self) -> T { self.value } }
func main() -> i32 {
    val w = Wrapper { value: 7 }
    return w.get()
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn bare_generic_struct_without_arguments_is_rejected() {
    let errors = semantic_errors(
        r#"
struct Box<T> { v: T }
func take(b: Box) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericStructNeedsArgs { .. })),
        "a generic struct used without type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn non_copy_generic_struct_argument_is_accepted() {
    let errors = semantic_errors(
        r#"
struct Box<T> { v: T }
func main() -> i32 {
    val b = Box { v: "hi" }
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a generic struct may be instantiated over a non-Copy type; got {errors:?}"
    );
}

#[test]
fn generic_struct_field_type_mismatch_is_rejected() {
    // Once a type parameter is bound by one field, another field's value must agree.
    let errors = semantic_errors(
        r#"
struct Same<T> { a: T, b: T }
func main() -> i32 {
    val s = Same { a: 1, b: true }
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "conflicting inferred type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn declared_lifetime_annotation_is_accepted() {
    // The canonical example: an explicit lifetime declared in `<'a>` and used on
    // reference parameters and the return type type-checks (returning a borrowed
    // parameter is already permitted by elision).
    let errors = semantic_errors(
        r#"
func longest<'a>(a: &'a string, b: &'a string) -> &'a string {
    if a.len() > b.len() { a } else { b }
}
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "declared lifetime should type-check; got {errors:?}"
    );
}

#[test]
fn undeclared_lifetime_is_rejected() {
    // `'b` is used but never declared in the parameter list: a well-formedness error.
    let errors = semantic_errors(
        r#"
func f<'a>(a: &'b string) -> i32 { 0 }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndeclaredLifetime { name, .. } if name == "b")),
        "an undeclared lifetime must be rejected; got {errors:?}"
    );
}

#[test]
fn lifetime_annotation_does_not_change_reference_type() {
    // `&'a string` is the same type as `&string`: passing an unannotated borrow to a
    // parameter typed with an explicit lifetime type-checks.
    let errors = semantic_errors(
        r#"
func take<'a>(s: &'a string) -> i32 { s.len() as i32 }
func main() -> i32 {
    val msg = "hi"
    return take(&msg)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "an explicit lifetime must not change the reference type; got {errors:?}"
    );
}
