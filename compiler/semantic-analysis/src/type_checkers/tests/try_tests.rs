//! The `?` operator: what it accepts, what it types to, and the three ways to get it
//! wrong (a non-fallible operand, a function that cannot carry the failure, and an
//! error type that does not already match — `?` never converts).
//!
//! `Option` / `Result` are prelude source rather than compiler built-ins, so every
//! program here declares them, which also proves `?` is not wired to one declaration.

use super::super::*;
use super::*;

/// The two fallible enums, declared exactly as the prelude declares them.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

#[test]
fn try_types_to_the_unwrapped_payload() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func fallible() -> Result<i64, char> {{ Result::Ok(2i64) }}
func maybe() -> Option<i64> {{ Option::Some(1i64) }}

func propagates() -> Result<i64, char> {{
    val a: i64 = fallible()?
    Result::Ok(a + 1i64)
}}

func optional() -> Option<i64> {{
    val b: i64 = maybe()?
    Option::Some(b)
}}

func main() -> i32 {{ 0 }}"
    ));
    assert!(errors.is_empty(), "valid `?` program; got {errors:?}");
}

#[test]
fn try_on_a_non_fallible_operand_is_rejected() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
enum Color {{ Red, Green }}

func main() -> i32 {{
    val a = 5?
    val b = Color::Red?
    0
}}"
    ));
    let rejected = errors
        .iter()
        .filter(|e| matches!(e, TypeError::TryOnNonFallible { .. }))
        .count();
    assert_eq!(
        rejected, 2,
        "a scalar and an unrelated enum must both be rejected; got {errors:?}"
    );
}

#[test]
fn try_needs_a_fallible_enclosing_function() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func maybe() -> Option<i32> {{ Option::Some(1) }}

func main() -> i32 {{
    val a = maybe()?
    0
}}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TryOutsideFallibleFunction { .. })),
        "a function returning i32 cannot carry a failure; got {errors:?}"
    );
}

#[test]
fn the_failure_kind_must_match_the_return_kind() {
    // An `Option`'s `None` is not a `Result`'s `Err`: there is no conversion between
    // the two fallible enums, so propagating across them is rejected.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func maybe() -> Option<i32> {{ Option::Some(1) }}

func wrap() -> Result<i32, char> {{
    val a = maybe()?
    Result::Ok(a)
}}

func main() -> i32 {{ 0 }}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TryOutsideFallibleFunction { .. })),
        "an Option cannot propagate out of a Result function; got {errors:?}"
    );
}

#[test]
fn the_error_type_must_already_match() {
    // The error is propagated as-is, with no implicit `.into()`. A `char` error
    // in an `i32`-error function is an ordinary type mismatch.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func fallible() -> Result<i32, char> {{ Result::Err('x') }}

func wrap() -> Result<i32, i32> {{
    val a = fallible()?
    Result::Ok(a)
}}

func main() -> i32 {{ 0 }}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "a mismatched error payload must be rejected; got {errors:?}"
    );
}

#[test]
fn the_payload_type_may_differ_from_the_returned_payload() {
    // Only the error types must agree — the success payloads are independent, since
    // the unwrapped value goes on to be used, not returned.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func fallible() -> Result<bool, char> {{ Result::Ok(true) }}

func wrap() -> Result<i32, char> {{
    val flag: bool = fallible()?
    if flag {{ Result::Ok(1) }} else {{ Result::Ok(0) }}
}}

func main() -> i32 {{ 0 }}"
    ));
    assert!(errors.is_empty(), "payloads need not match; got {errors:?}");
}

#[test]
fn a_shadowing_declaration_still_propagates() {
    // `?` resolves `Ok` / `Err` against whichever `Result` is in scope, so a program
    // that declares its own non-generic one is served by the same rule.
    let errors = semantic_errors(
        "enum Result { Ok(i32), Err(i32) }

func fallible() -> Result { Result::Ok(4) }

func wrap() -> Result {
    val a = fallible()?
    Result::Ok(a)
}

func main() -> i32 { 0 }",
    );
    assert!(
        errors.is_empty(),
        "a shadowing `Result` still propagates; got {errors:?}"
    );
}
