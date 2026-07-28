#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn array_literal_index_len_and_iteration_type_check() {
    // A typed array literal, index read/write, `.len()`, and `for x in arr`
    // / `for x in &arr` all type-check in one program.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    mut b = [10, 20, 30]
    b[0] = 99
    val first: i32 = a[0]
    val n: u64 = a.len()
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    for y in &b {
        total = total + y
    }
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "valid array program; got {errors:?}");
}

#[test]
fn array_length_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ArrayLengthMismatch { .. })),
        "a literal whose length differs from the annotation must be rejected; got {errors:?}"
    );
}

#[test]
fn non_integer_array_index_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    val x: i32 = a[true]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::IndexNotInteger { .. })),
        "a non-integer index must be rejected; got {errors:?}"
    );
}

#[test]
fn indexing_a_non_array_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val n: i32 = 5
    val x: i32 = n[0]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NotIndexable { .. })),
        "indexing a non-array must be rejected; got {errors:?}"
    );
}

#[test]
fn heterogeneous_array_literal_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a = [1, true, 3]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "elements of differing types must be rejected; got {errors:?}"
    );
}

#[test]
fn array_of_non_copy_element_is_rejected() {
    // String elements need per-element move tracking, not yet supported.
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [string; 2] = ["a", "b"]
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::NonCopyArrayElement { .. })),
        "a non-Copy element array must be rejected; got {errors:?}"
    );
}

#[test]
fn assigning_through_index_of_immutable_array_is_rejected() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    a[0] = 9
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssignToImmutable { .. })),
        "writing an element of a `val` array must be rejected; got {errors:?}"
    );
}

// --- Newtype declarations ---------------------------------------------
