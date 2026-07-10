// End-to-end tests for const generic parameters, `where` clauses, and turbofish (§3.8).
//
// A const parameter is a compile-time *value* monomorphized per distinct value (zero
// runtime cost), inferred from array-argument lengths or supplied by a turbofish. A
// `where` clause carries value predicates checked at each instantiation. These tests
// drive the whole pipeline (parse → type-check → HIR lowering → LLVM → native binary)
// and assert on the program's exit code.
mod common;
use common::CompileTest;

#[test]
fn const_param_inferred_from_array_length() {
    let test = CompileTest::new();
    // N is inferred from the passed array's length; the body iterates the array.
    let source = r#"
func sum<const N: u32>(a: [i32; N]) -> i32 {
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    total
}

func main() -> i32 {
    val nums: [i32; 3] = [10, 20, 12]
    sum(nums)          // 42
}
"#;
    let exit = test
        .compile_and_run("const_sum.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn const_param_value_used_in_body() {
    let test = CompileTest::new();
    // A const parameter is usable as a value; here N (= 4) drives the result directly.
    let source = r#"
func capacity<const N: u32>(a: [i32; N]) -> u32 {
    N
}

func main() -> i32 {
    val xs: [i32; 4] = [1, 1, 1, 1]
    capacity(xs) as i32   // 4
}
"#;
    let exit = test
        .compile_and_run("const_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn distinct_const_values_monomorphize_separately() {
    let test = CompileTest::new();
    // Two calls with different lengths produce two distinct instances.
    let source = r#"
func first<const N: u32>(a: [i32; N]) -> i32 {
    a[0]
}

func main() -> i32 {
    val two: [i32; 2] = [30, 0]
    val four: [i32; 4] = [12, 0, 0, 0]
    first(two) + first(four)   // 30 + 12 = 42
}
"#;
    let exit = test
        .compile_and_run("const_distinct.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn where_predicate_satisfied() {
    let test = CompileTest::new();
    let source = r#"
func head<const N: u32>(a: [i32; N]) -> i32 where N > 0 {
    a[0]
}

func main() -> i32 {
    val xs: [i32; 3] = [42, 1, 2]
    head(xs)   // 42, predicate 3 > 0 holds
}
"#;
    let exit = test
        .compile_and_run("where_ok.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn where_predicate_violation_is_rejected() {
    let test = CompileTest::new();
    // N = 3 violates `N > 5`, so the instantiation is a compile error.
    let source = r#"
func head<const N: u32>(a: [i32; N]) -> i32 where N > 5 {
    a[0]
}

func main() -> i32 {
    val xs: [i32; 3] = [1, 2, 3]
    head(xs)
}
"#;
    let path = test.write_source("where_bad.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a violated `where` predicate must be rejected"
    );
}

#[test]
fn turbofish_disambiguates_type_argument() {
    let test = CompileTest::new();
    let source = r#"
func identity<T>(x: T) -> T {
    x
}

func main() -> i32 {
    identity::<i32>(42)
}
"#;
    let exit = test
        .compile_and_run("turbofish.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn const_generic_struct_infers_capacity() {
    let test = CompileTest::new();
    // CAP is inferred from the field array's length; `[T; CAP]` is sized concretely.
    let source = r#"
struct Buffer<T, const CAP: u32> {
    data: [T; CAP],
    count: u32
}

func main() -> i32 {
    val buf = Buffer { data: [40, 1, 1, 0], count: 4 }
    buf.data[0] + (buf.count as i32) - 2   // 40 + 4 - 2 = 42
}
"#;
    let exit = test
        .compile_and_run("const_struct.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn const_generic_struct_explicit_annotation() {
    let test = CompileTest::new();
    // The explicit `Buffer<i32, 4>` annotation carries the const argument `4`.
    let source = r#"
@derive(Copy)
struct Buffer<T, const CAP: u32> {
    data: [T; CAP]
}

func main() -> i32 {
    val buf: Buffer<i32, 4> = Buffer { data: [7, 8, 9, 42] }
    buf.data[3]   // 42
}
"#;
    let exit = test
        .compile_and_run("const_struct_annot.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}
