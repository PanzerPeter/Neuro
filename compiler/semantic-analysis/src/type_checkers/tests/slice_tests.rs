use super::semantic_errors;
use crate::errors::TypeError;

#[test]
fn one_slice_signature_accepts_an_array_a_sub_range_and_a_vec() {
    // The driving case: a single `&[i32]` parameter serves all three sources.
    let errors = semantic_errors(
        r#"
func sum(xs: &[i32]) -> i32 {
    mut total = 0
    for x in xs {
        total += x
    }
    total
}

func main() -> i32 {
    val fixed: [i32; 4] = [1, 2, 3, 4]
    mut grown: Vec<i32> = Vec::new()
    grown.push(10)
    val a = sum(&fixed)
    val b = sum(fixed.slice(1..3))
    val c = sum(&grown)
    return a + b + c
}
"#,
    );
    assert!(errors.is_empty(), "valid slice program; got {errors:?}");
}

#[test]
fn slice_len_and_indexing_type_check() {
    let errors = semantic_errors(
        r#"
func head(xs: &[i32]) -> i32 {
    if xs.len() == 0 { return 0 }
    xs[0]
}

func main() -> i32 {
    val a: [i32; 2] = [7, 8]
    return head(&a)
}
"#,
    );
    assert!(errors.is_empty(), "valid slice reads; got {errors:?}");
}

#[test]
fn writing_through_a_mutable_slice_type_checks() {
    let errors = semantic_errors(
        r#"
func zero(xs: &mut [i32]) {
    xs[0] = 0
}

func main() -> i32 {
    mut a: [i32; 2] = [7, 8]
    zero(&mut a)
    return a[0]
}
"#,
    );
    assert!(errors.is_empty(), "valid slice write; got {errors:?}");
}

#[test]
fn writing_through_a_shared_slice_is_rejected() {
    let errors = semantic_errors(
        r#"
func zero(xs: &[i32]) {
    xs[0] = 0
}

func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssignToImmutable { .. })),
        "a `&[T]` grants no write access; got {errors:?}"
    );
}

#[test]
fn a_bare_slice_annotation_is_rejected() {
    let errors = semantic_errors(
        r#"
func take(xs: [i32]) -> i32 { return 0 }
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::SliceNotBehindReference { .. })),
        "`[T]` is unsized; got {errors:?}"
    );
}

#[test]
fn a_slice_element_type_must_match_the_parameter() {
    let errors = semantic_errors(
        r#"
func sum(xs: &[i32]) -> i32 { return 0 }

func main() -> i32 {
    val a: [i64; 2] = [1, 2]
    return sum(&a)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "`&[i64; 2]` does not unsize to `&[i32]`; got {errors:?}"
    );
}

#[test]
fn a_shared_slice_does_not_satisfy_a_mutable_slice_parameter() {
    let errors = semantic_errors(
        r#"
func zero(xs: &mut [i32]) { }

func main() -> i32 {
    mut a: [i32; 2] = [1, 2]
    zero(&a)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "there is no `&T` -> `&mut T` strengthening; got {errors:?}"
    );
}

#[test]
fn a_live_slice_blocks_a_mutable_borrow_of_its_source() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    mut a: [i32; 3] = [1, 2, 3]
    val view = a.slice(0..2)
    val m = &mut a
    return view[0]
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMutablyBorrowWhileBorrowed { .. })),
        "a `.slice` view is a live borrow of its receiver; got {errors:?}"
    );
}

#[test]
fn returning_a_slice_of_a_local_is_rejected() {
    let errors = semantic_errors(
        r#"
func leak() -> &[i32] {
    val local: [i32; 3] = [1, 2, 3]
    val view = local.slice(0..2)
    return view
}
func main() -> i32 { return 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnsReferenceToLocal { .. })),
        "the view outlives the buffer it points into; got {errors:?}"
    );
}

#[test]
fn slice_takes_a_range_and_nothing_else() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    val view = a.slice(1)
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::SliceExpectsRange { .. })),
        "`.slice` needs `a..b`; got {errors:?}"
    );
}
