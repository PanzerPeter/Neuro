//! The `??` operator: which left operands it accepts, what it types to, and the
//! diagnostics for the two ways to get it wrong.
//!
//! `Option` / `Result` are prelude source rather than compiler built-ins, so every
//! program here declares them — which is also what proves `??` is not hard-wired to
//! one specific declaration.

use super::super::*;
use super::*;

/// The two fallible enums, declared exactly as the prelude declares them.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

#[test]
fn coalesce_types_to_the_unwrapped_payload() {
    // Both fallible types unwrap to their success payload, and the result is an
    // ordinary value of that type — usable in arithmetic against a sibling of the
    // same width.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func maybe() -> Option<i64> {{ Option::Some(1i64) }}
func fallible() -> Result<i64, bool> {{ Result::Ok(2i64) }}

func main() -> i32 {{
    val a: i64 = maybe() ?? 0i64
    val b: i64 = fallible() ?? 0i64
    val c: i64 = a + b
    0
}}"
    ));
    assert!(errors.is_empty(), "valid `??` program; got {errors:?}");
}

#[test]
fn coalesce_chains_right_to_left() {
    // `a ?? b ?? c` parses as `a ?? (b ?? c)`, so the middle operand must itself be
    // fallible and only the last one is a bare value.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func maybe(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    maybe(1) ?? maybe(2) ?? 3
}}"
    ));
    assert!(errors.is_empty(), "valid `??` chain; got {errors:?}");
}

#[test]
fn coalesce_discards_the_error_payload() {
    // The `Err` type is unconstrained by `??` — the fallback answers to `T`, never
    // to `E`, so a `Result<i32, string>` coalesces with a plain `i32`.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func fallible() -> Result<i32, char> {{ Result::Err('x') }}

func main() -> i32 {{
    fallible() ?? 7
}}"
    ));
    assert!(
        errors.is_empty(),
        "`E` must not constrain the fallback; got {errors:?}"
    );
}

#[test]
fn coalesce_on_a_non_fallible_left_operand_is_rejected() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
enum Color {{ Red, Green }}

func main() -> i32 {{
    val a = 5 ?? 1
    val b = Color::Red ?? 2
    0
}}"
    ));
    let rejected = errors
        .iter()
        .filter(|e| matches!(e, TypeError::NullCoalesceOnNonFallible { .. }))
        .count();
    assert_eq!(
        rejected, 2,
        "a scalar and an unrelated enum must both be rejected; got {errors:?}"
    );
}

#[test]
fn fallback_must_match_the_payload_type() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func maybe() -> Option<i32> {{ Option::Some(1) }}

func main() -> i32 {{
    val a = maybe() ?? true
    0
}}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "a fallback of the wrong type must be rejected; got {errors:?}"
    );
}

#[test]
fn a_shadowing_declaration_still_coalesces() {
    // `??` resolves `Some` / `Ok` against whichever `Option` is in scope, so a program
    // that declares its own non-generic one is served by the same rule.
    let errors = semantic_errors(
        "enum Option { Some(i32), None }

func maybe() -> Option { Option::Some(4) }

func main() -> i32 {
    maybe() ?? 0
}",
    );
    assert!(
        errors.is_empty(),
        "a locally declared Option must coalesce; got {errors:?}"
    );
}
