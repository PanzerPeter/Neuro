// End-to-end tests for tuples `(T1, T2, ...)`: the tuple type annotation,
// tuple literals, `.N` constant index access, destructuring binds (flat, nested,
// and `_` wildcard), and tuples crossing function boundaries.
use crate::compile_harness::CompileTest;

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
    let err = test
        .compile(&test.write_source("tuple_grouping.nr", source))
        .expect_err("expected `.0` on a grouped scalar to be a type error");
    assert!(err.contains("cannot index a value of type i32"), "{err}");
}

#[test]
fn non_copy_tuple_element_is_moved_not_copied() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s = "a" + "b"
    val t: (i32, string) = (1, s)
    println(t.1)
    0
}
"#;
    test.compile(&test.write_source("tuple_noncopy.nr", source))
        .expect("a tuple holding a `string` compiles");

    let moved = r#"
func main() -> i32 {
    val s = "a" + "b"
    val t: (string, string) = (s, s)
    0
}
"#;
    let err = test
        .compile(&test.write_source("tuple_double_move.nr", moved))
        .expect_err("expected the same binding in two tuple slots to be a double move");
    assert!(err.contains("use of moved value 's'"), "{err}");
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
    let err = test
        .compile(&test.write_source("tuple_oob.nr", source))
        .expect_err("expected an out-of-range tuple index to be rejected");
    assert!(err.contains("tuple index 5 is out of range"), "{err}");
}

#[test]
fn one_element_tuple_literal_is_rejected() {
    // There is no one-element tuple: one value is written as itself.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t = (7,)
    return 0
}
"#;
    let err = test
        .compile(&test.write_source("tuple_one.nr", source))
        .expect_err("`(x,)` must be rejected");
    assert!(
        err.contains("a tuple has zero or at least two elements"),
        "{err}"
    );
}

#[test]
fn trailing_comma_after_two_elements_is_still_a_tuple() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t = (3, 4,)
    t.0 + t.1
}
"#;
    let exit = test
        .compile_and_run("tuple_trailing.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

/// `()` is the tuple of no elements: a type, a value, and the success payload of a
/// `Result` that has nothing to hand back.
#[test]
fn regression_bug_139_unit_is_a_type_and_a_value() {
    let test = CompileTest::new();
    let source = r#"
func code(o: Result<(), i32>) -> i32 {
    match o {
        Result::Ok(_) => 1,
        Result::Err(e) => e
    }
}

func check(n: i32) -> Result<(), i32> {
    if n > 0 { return Err(n) }
    Ok(())
}

func both(n: i32) -> Result<(), i32> {
    check(n)?
    check(n - 1)?
    Ok(())
}

func nothing() -> () { }
func same(u: ()) -> () { u }
func maybe(c: bool) -> Option<()> { if c { Some(()) } else { None } }

func main() -> i32 {
    nothing()
    val u: () = ()
    same(u)
    mut acc = code(check(0)) + code(check(5)) + code(both(0)) + code(both(3))
    val Ok(v) = check(0) else { return 99 }
    if val Some(w) = maybe(true) { acc += 1 }
    val pair = ((), 2)
    acc + pair.1
}
"#;
    assert_eq!(test.compile_and_run("unit.nr", source), Ok(13));
}
