// End-to-end tests for struct and array destructuring patterns (§3.2): struct
// field binds `val Point { x, y } = p`, positional array binds `val [a, b] = arr`,
// the trailing rest `val [first, ..rest] = arr`, nested patterns, and the arity
// rules. Tuple destructuring is covered separately in `tuples.rs`.
mod common;
use common::CompileTest;

#[test]
fn struct_destructure_binds_fields() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

func main() -> i32 {
    val p = Point { x: 12, y: 30 }
    val Point { x, y } = p
    x + y
}
"#;
    let exit = test
        .compile_and_run("struct_destructure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn array_destructure_binds_all_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr: [i32; 3] = [10, 20, 12]
    val [a, b, c] = arr
    a + b + c
}
"#;
    let exit = test
        .compile_and_run("array_destructure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn array_destructure_rest_captures_remainder() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr: [i32; 5] = [1, 2, 10, 14, 15]
    val [first, second, ..rest] = arr
    mut sum: i32 = first + second
    for r in rest {
        sum = sum + r
    }
    sum
}
"#;
    // first(1) + second(2) + rest(10+14+15) = 42
    let exit = test
        .compile_and_run("array_rest.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn array_destructure_bare_rest_ignores_remainder() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr: [i32; 4] = [42, 1, 2, 3]
    val [head, ..] = arr
    head
}
"#;
    let exit = test
        .compile_and_run("array_bare_rest.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn array_destructure_wildcard_skips_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr: [i32; 3] = [1, 42, 3]
    val [_, mid, _] = arr
    mid
}
"#;
    let exit = test
        .compile_and_run("array_wildcard.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn nested_tuple_in_array_destructure() {
    // A tuple sub-pattern nested inside an array pattern, with a trailing rest. The
    // desugar recurses through both pattern kinds.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr: [(i32, i32); 3] = [(30, 12), (3, 4), (5, 6)]
    val [(a, b), ..rest] = arr
    a + b + rest.len() as i32
}
"#;
    // a(30) + b(12) + rest.len()(2) = 44
    let exit = test
        .compile_and_run("nested_tuple_array.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 44);
}
