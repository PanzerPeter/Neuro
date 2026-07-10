// Semantic tests: const generic parameters, `where` clauses, and turbofish (§3.8).

use semantic_analysis::{type_check, TypeError};

fn check(source: &str) -> Result<(), Vec<TypeError>> {
    let items = syntax_parsing::parse(source).expect("should parse");
    type_check(&items).map(|_warnings| ())
}

#[test]
fn const_param_inferred_from_array_argument() {
    let source = r#"
func first<const N: u32>(a: [i32; N]) -> i32 { a[0] }
func main() -> i32 {
    val xs: [i32; 3] = [1, 2, 3]
    first(xs)
}"#;
    assert!(check(source).is_ok());
}

#[test]
fn const_param_usable_as_value() {
    let source = r#"
func cap<const N: u32>(a: [i32; N]) -> u32 { N }
func main() -> i32 {
    val xs: [i32; 2] = [1, 2]
    cap(xs) as i32
}"#;
    assert!(check(source).is_ok());
}

#[test]
fn where_predicate_violation_is_an_error() {
    let source = r#"
func head<const N: u32>(a: [i32; N]) -> i32 where N > 5 { a[0] }
func main() -> i32 {
    val xs: [i32; 3] = [1, 2, 3]
    head(xs)
}"#;
    let errors = check(source).expect_err("N = 3 violates N > 5");
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ConstPredicateViolated { .. })));
}

#[test]
fn where_predicate_satisfied_is_ok() {
    let source = r#"
func head<const N: u32>(a: [i32; N]) -> i32 where N > 0 { a[0] }
func main() -> i32 {
    val xs: [i32; 3] = [1, 2, 3]
    head(xs)
}"#;
    assert!(check(source).is_ok());
}

#[test]
fn turbofish_type_argument_is_accepted() {
    let source = r#"
func identity<T>(x: T) -> T { x }
func main() -> i32 { identity::<i32>(5) }"#;
    assert!(check(source).is_ok());
}

#[test]
fn turbofish_kind_mismatch_is_rejected() {
    // A type argument supplied where a const value is expected is a kind mismatch.
    let source = r#"
func repeat<const N: u32>(a: [i32; N]) -> i32 { a[0] }
func main() -> i32 {
    val xs: [i32; 2] = [1, 2]
    repeat::<i32>(xs)
}"#;
    let errors = check(source).expect_err("const parameter given a type argument");
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::TurbofishKindMismatch { .. })));
}

#[test]
fn const_generic_struct_infers_capacity() {
    let source = r#"
struct Buffer<T, const CAP: u32> { data: [T; CAP], count: u32 }
func main() -> i32 {
    val b = Buffer { data: [1, 2, 3, 4], count: 4 }
    b.data[0]
}"#;
    assert!(check(source).is_ok());
}

#[test]
fn const_param_must_be_integer() {
    let source = r#"
func f<const N: bool>() -> i32 { 0 }
func main() -> i32 { 0 }"#;
    let errors = check(source).expect_err("a bool const parameter is invalid");
    assert!(errors
        .iter()
        .any(|e| matches!(e, TypeError::ConstParamNotInteger { .. })));
}
