//! `val-else`: where the pattern's bindings land, what `else |name|` names for each
//! scrutinee type, and the divergence rule on the `else` branch.
//!
//! `Option` / `Result` are prelude source rather than compiler built-ins, so every
//! program here declares them — which also proves the rules key off the scrutinee's
//! type rather than one specific declaration.

use super::super::*;
use super::*;

/// The two fallible enums, declared exactly as the prelude declares them.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

#[test]
fn bindings_are_visible_for_the_rest_of_the_block() {
    // The defining property: `data` outlives the statement, unlike a `match` arm's.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func parse(n: i32) -> Result<i32, i32> {{ Result::Ok(n) }}

func main() -> i32 {{
    val Result::Ok(data) = parse(1) else |err| {{ return err }}
    val doubled = data * 2
    doubled
}}"
    ));
    assert!(errors.is_empty(), "valid `val-else`; got {errors:?}");
}

#[test]
fn a_result_else_binding_names_the_error_payload() {
    // `E` here is `bool`, so using `err` as an `i32` must be a mismatch — proof the
    // binding took the `Err` payload type and not the whole `Result`.
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func parse(n: i32) -> Result<i32, bool> {{ Result::Ok(n) }}

func main() -> i32 {{
    val Result::Ok(v) = parse(1) else |err| {{ return err }}
    v
}}"
    ));
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::ReturnTypeMismatch {
                found: Type::Bool,
                ..
            }
        )),
        "`err` should be the `Err` payload (bool); got {errors:?}"
    );
}

#[test]
fn a_named_else_binding_on_an_option_is_rejected() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func lookup(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    val Option::Some(v) = lookup(1) else |e| {{ return 0 }}
    v
}}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ValElseBindingOnOption { .. })),
        "`Option::None` carries no payload to bind; got {errors:?}"
    );
}

#[test]
fn a_wildcard_else_binding_on_an_option_is_accepted() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func lookup(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    val Option::Some(v) = lookup(1) else |_| {{ return 0 }}
    v
}}"
    ));
    assert!(errors.is_empty(), "`|_|` binds nothing; got {errors:?}");
}

#[test]
fn a_plain_enum_else_binding_names_the_whole_scrutinee() {
    // Neither Option nor Result, so `|s|` is the untouched `Shape` — matchable again.
    let errors = semantic_errors(
        "enum Shape { Circle { radius: i32 }, Square(i32), Empty }

func main() -> i32 {
    val Shape::Circle { radius } = Shape::Square(3) else |s| {
        match s {
            Shape::Square(side) => { return side },
            _ => { return 0 }
        }
    }
    radius
}",
    );
    assert!(errors.is_empty(), "valid `val-else`; got {errors:?}");
}

#[test]
fn a_falling_through_else_branch_is_rejected() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func lookup(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    val Option::Some(v) = lookup(1) else {{ val fallback = 0 }}
    v
}}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ValElseMustDiverge { .. })),
        "the `else` branch must exit the scope; got {errors:?}"
    );
}

#[test]
fn panic_and_break_both_satisfy_the_divergence_rule() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func lookup(n: i32) -> Option<i32> {{ Option::Some(n) }}

func main() -> i32 {{
    mut total: i32 = 0
    mut i: i32 = 0
    loop {{
        val Option::Some(v) = lookup(i) else {{ break }}
        total = total + v
        i = i + 1
        if i > 3 {{ break }}
    }}
    val Option::Some(last) = lookup(9) else {{ panic(\"absent\") }}
    total + last
}}"
    ));
    assert!(errors.is_empty(), "valid `val-else`; got {errors:?}");
}

#[test]
fn the_else_binding_is_scoped_to_the_else_branch() {
    let errors = semantic_errors(&format!(
        "{FALLIBLE_DECLS}
func parse(n: i32) -> Result<i32, i32> {{ Result::Ok(n) }}

func main() -> i32 {{
    val Result::Ok(v) = parse(1) else |err| {{ return err }}
    err
}}"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UndefinedVariable { .. })),
        "`err` must not escape the `else` branch; got {errors:?}"
    );
}
