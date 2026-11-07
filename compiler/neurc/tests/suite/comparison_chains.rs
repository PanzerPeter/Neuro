use crate::compile_harness::check_source;

#[test]
fn chained_less_than_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a < b < c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained comparison should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn chained_greater_equal_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 3
    val b: i32 = 2
    val c: i32 = 1
    val x: bool = a >= b > c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained comparison should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn chained_equality_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 1
    val c: i32 = 1
    val x: bool = a == b == c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained equality should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}

#[test]
fn single_comparison_accepted() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val x: bool = a < b
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(success, "Single comparison should pass, got: {stderr}");
}

#[test]
fn logical_and_comparisons_accepted() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a < b && b < c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        success,
        "Comparisons joined with && should pass, got: {stderr}"
    );
}

#[test]
fn chained_not_equal_rejected() {
    let source = r#"
func main() -> i32 {
    val a: i32 = 1
    val b: i32 = 2
    val c: i32 = 3
    val x: bool = a != b != c
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "Chained != should be rejected");
    assert!(
        stderr.contains("cannot be chained"),
        "Expected 'cannot be chained' in error, got: {stderr}"
    );
}
