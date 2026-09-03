//! Type rules for the `.map(f)` / `.filter(p)` chain a `for` head may wear.

use super::semantic_errors;
use crate::errors::TypeError;
use crate::types::Type;

#[test]
fn map_rebinds_the_loop_variable_to_the_function_result() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            mut total = 0.0
            for f in [1, 2, 3].map(|x: i32| -> f64 { x as f64 }) {
                total = total + f
            }
            0
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "map should retype the binding; got {errors:?}"
    );
}

#[test]
fn filter_leaves_the_element_type_alone() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            mut total = 0
            for v in (0..10).filter(|x: i32| -> bool { x % 2 == 0 }) {
                total = total + v
            }
            0
        }
        "#,
    );
    assert!(errors.is_empty(), "filter should keep i32; got {errors:?}");
}

#[test]
fn a_chain_applies_left_to_right() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for s in [1, 2].map(|x: i32| -> f64 { x as f64 }).filter(|f: f64| -> bool { f > 1.0 }) {
                val kept: f64 = s
            }
            0
        }
        "#,
    );
    assert!(
        errors.is_empty(),
        "chain should thread f64 through; got {errors:?}"
    );
}

#[test]
fn a_non_function_argument_is_rejected() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for x in [1, 2].map(3) { }
            0
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::LoopAdapterNotCallable { .. })),
        "expected a not-callable error; got {errors:?}"
    );
}

#[test]
fn a_function_of_the_wrong_parameter_type_is_rejected() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for x in [1, 2].map(|s: bool| -> i32 { 1 }) { }
            0
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::LoopAdapterInput {
                expected: Type::I32,
                found: Type::Bool,
                ..
            }
        )),
        "expected an input mismatch; got {errors:?}"
    );
}

#[test]
fn a_filter_predicate_must_answer_bool() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for x in [1, 2].filter(|x: i32| -> i32 { x }) { }
            0
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::LoopAdapterOutput {
                found: Type::I32,
                ..
            }
        )),
        "expected a non-bool predicate error; got {errors:?}"
    );
}

/// A `void` result would bind the loop variable to nothing, which the backend cannot
/// represent — the same class as a `void` binding.
#[test]
fn a_map_producing_void_is_rejected() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for x in [1, 2].map(|x: i32| -> void { }) { }
            0
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::LoopAdapterOutput {
                found: Type::Void,
                ..
            }
        )),
        "expected a void-result error; got {errors:?}"
    );
}

/// The adapter's function is evaluated outside the loop, so it cannot name the
/// binding it feeds.
#[test]
fn an_adapter_cannot_see_the_loop_binding() {
    let errors = semantic_errors(
        r#"
        func main() -> i32 {
            for x in [1, 2].map(|v: i32| -> i32 { v + x }) { }
            0
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndefinedVariable { .. })),
        "expected the loop binding to be out of scope; got {errors:?}"
    );
}
