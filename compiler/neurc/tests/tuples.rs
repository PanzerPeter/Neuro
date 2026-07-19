// End-to-end tests for tuples `(T1, T2, ...)`: the tuple type annotation,
// tuple literals, `.N` constant index access, destructuring binds (flat, nested,
// and `_` wildcard), and tuples crossing function boundaries.
mod common;
use common::CompileTest;

#[test]
fn tuple_index_reads_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val pair: (i32, i32) = (3, 4)
    pair.0 + pair.1
}
"#;
    let exit = test
        .compile_and_run("tuple_index.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn tuple_destructure_binds_each_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val (x, y, z) = (10, 20, 12)
    x + y + z
}
"#;
    let exit = test
        .compile_and_run("tuple_destructure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn tuple_destructure_wildcard_skips_binding() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val (_, keep, _) = (10, 7, 20)
    keep
}
"#;
    let exit = test
        .compile_and_run("tuple_wildcard.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn nested_tuple_destructure_and_index() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val nested = ((1, 2), 5)
    val ((a, b), c) = nested
    val viaindex = (nested.0).1
    a + b + c + viaindex
}
"#;
    let exit = test
        .compile_and_run("tuple_nested.nr", source)
        .expect("compile/run failed");
    // a=1 b=2 c=5 viaindex=2 -> 10
    assert_eq!(exit, 10);
}

#[test]
fn function_returns_tuple() {
    let test = CompileTest::new();
    let source = r#"
func swap(a: i32, b: i32) -> (i32, i32) {
    (b, a)
}

func main() -> i32 {
    val s = swap(3, 4)
    // s.0 = 4, s.1 = 3
    s.0 * 10 + s.1
}
"#;
    let exit = test
        .compile_and_run("tuple_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 43);
}

#[test]
fn heterogeneous_tuple_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: (i32, bool, i32) = (5, true, 9)
    if t.1 { t.0 + t.2 } else { 0 }
}
"#;
    let exit = test
        .compile_and_run("tuple_hetero.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}

#[test]
fn single_parenthesized_expr_is_not_a_tuple() {
    // `(x)` is grouping, not a 1-tuple; `.0` on a plain integer must be rejected.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x = (7)
    x.0
}
"#;
    let result = test.compile(&test.write_source("tuple_grouping.nr", source));
    assert!(
        result.is_err(),
        "expected `.0` on a grouped scalar to be a type error"
    );
}

#[test]
fn non_copy_tuple_element_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: (i32, string) = (1, "x")
    0
}
"#;
    let result = test.compile(&test.write_source("tuple_noncopy.nr", source));
    assert!(
        result.is_err(),
        "expected a non-Copy tuple element to be rejected"
    );
}

#[test]
fn tuple_index_out_of_range_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t = (1, 2)
    t.5
}
"#;
    let result = test.compile(&test.write_source("tuple_oob.nr", source));
    assert!(
        result.is_err(),
        "expected an out-of-range tuple index to be rejected"
    );
}
