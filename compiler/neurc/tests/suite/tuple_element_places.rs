// A tuple element as an assignment target: `pair.0 = v`, `pair.1 += 1`, at any depth,
// held in a struct, an array or a `Vec`, and reached through a `&mut` borrow.
use crate::compile_harness::CompileTest;

#[test]
fn test_assignment_and_compound_assignment_to_a_tuple_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut pair = (1, 2)
    pair.0 = 7
    pair.1 += 3
    pair.1 *= 2
    return pair.0 * 10 + pair.1
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_store.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 80);
}

#[test]
fn test_a_float_tuple_element_takes_compound_operators() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut p: (f64, i32) = (1.5, 2)
    p.0 *= 4.0
    p.0 -= 1.0
    return p.0 as i32
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_float.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 5);
}

#[test]
fn test_a_nested_tuple_element_is_a_place() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t = ((1, 2), 3)
    (t.0).1 = 9
    (t.0).0 *= 4
    t.1 += (t.0).1
    return (t.0).0 * 10 + t.1
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_nested.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 52);
}

#[test]
fn test_a_tuple_element_held_in_a_field_an_array_and_a_vec() {
    let test = CompileTest::new();
    let source = r#"
struct Holder { pair: (i32, i32) }

func main() -> i32 {
    mut h = Holder { pair: (0, 0) }
    h.pair.1 = 5
    mut arr = [(1, 1), (2, 2)]
    arr[1].0 = 10
    mut v: Vec<(i32, i32)> = Vec::new()
    v.push((1, 2))
    v[0].1 += 20
    return h.pair.1 + arr[1].0 + v[0].1
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_held.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 37);
}

#[test]
fn test_a_tuple_element_written_through_a_mutable_borrow() {
    let test = CompileTest::new();
    let source = r#"
func bump(r: &mut (i32, i64)) {
    r.0 += 1
    r.1 = 40
}

func main() -> i32 {
    mut w: (i32, i64) = (1, 0)
    bump(&mut w)
    bump(&mut w)
    return w.0 + w.1 as i32
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_borrow.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 43);
}

#[test]
fn test_a_string_tuple_element_is_replaced_and_read_back() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val k: string = "x"
    mut total: u64 = 0
    mut i = 0
    while i < 50 {
        mut t: (string, i32) = (k + "a", 1)
        t.0 = k + "bb"
        t.0 = "lit"
        t.0 = k + "ccc"
        mut u: (i32, (string, i32)) = (1, (k + "x", 2))
        (u.1).0 = k + "yyyy"
        total += t.0.len() + (u.1).0.len()
        i += 1
    }
    return total as i32 / 10
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_string.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 45);
}

#[test]
fn test_a_closure_writes_a_captured_tuple_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut p = (1, 2)
    {
        mut bump = || { p.1 += 10 }
        bump()
        bump()
    }
    return p.1
}
"#;
    let exit_code = test
        .compile_and_run("tuple_element_closure.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 22);
}

#[test]
fn test_an_element_of_an_immutable_tuple_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val pair = (1, 2)
    pair.0 = 5
    return pair.0
}
"#;
    let error = test
        .check("tuple_element_immutable.nr", source)
        .expect_err("an immutable binding's element is not assignable");
    assert!(error.contains("immutable"), "got: {error}");
}

#[test]
fn test_an_element_behind_a_shared_borrow_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val p = (1, 2)
    mut r: &(i32, i32) = &p
    r.0 = 9
    return p.0
}
"#;
    let error = test
        .check("tuple_element_shared.nr", source)
        .expect_err("a write through a shared borrow must be refused");
    assert!(error.contains("shared `&` borrow"), "got: {error}");
}

#[test]
fn test_an_out_of_range_tuple_element_target_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut p = (1, 2)
    p.2 = 1
    return 0
}
"#;
    let error = test
        .check("tuple_element_range.nr", source)
        .expect_err("a tuple has no element past its arity");
    assert!(error.contains("out of range"), "got: {error}");
}

#[test]
fn test_a_mistyped_tuple_element_store_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut p = (1, 2)
    p.0 = true
    return 0
}
"#;
    let error = test
        .check("tuple_element_mistyped.nr", source)
        .expect_err("the element's type decides what may be stored");
    assert!(error.contains("expected i32, found bool"), "got: {error}");
}

#[test]
fn test_a_tuple_element_store_while_borrowed_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut p = (1, 2)
    val r = &p
    p.0 = 9
    return r.0
}
"#;
    let error = test
        .check("tuple_element_borrowed.nr", source)
        .expect_err("a borrowed tuple may not be written");
    assert!(error.contains("while it is borrowed"), "got: {error}");
}

#[test]
fn test_a_newtype_inner_value_is_not_a_place() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32

func main() -> i32 {
    mut m = Meters(1)
    m.0 = 2
    return 0
}
"#;
    let error = test
        .check("newtype_inner_target.nr", source)
        .expect_err("a newtype's `.0` reads but does not write");
    assert!(
        error.contains("inner value of newtype 'Meters'"),
        "got: {error}"
    );
}
