// Move-by-default ownership tests (Phase 1.7)
// Verifies use-after-move is rejected at `neurc check`, and that valid
// straight-line and `.clone()` programs still compile and run end-to-end.
use crate::compile_harness::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stderr)
}

#[test]
fn use_after_move_on_bind_is_rejected() {
    let source = r#"
func main() -> i32 {
    val s1: string = "Hello"
    val s2: string = s1
    val n: u64 = s1.len()
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "use after move should be rejected");
    assert!(
        stderr.contains("use of moved value"),
        "expected move diagnostic, got: {stderr}"
    );
}

#[test]
fn use_after_move_into_call_is_rejected() {
    let source = r#"
func consume(s: string) -> i32 { 0 }

func main() -> i32 {
    val greeting: string = "Hi"
    val r: i32 = consume(greeting)
    val n: u64 = greeting.len()
    return 0
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "use after move-into-call should be rejected");
    assert!(
        stderr.contains("use of moved value"),
        "expected move diagnostic, got: {stderr}"
    );
}

#[test]
fn clone_avoids_the_move() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val original: string = "neuro"
    val copy: string = original.clone()
    if original == copy {
        return 0
    }
    return 1
}
"#;
    let exit_code = test
        .compile_and_run("move_clone.nr", source)
        .expect("clone program should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn a_conditional_move_reaches_past_the_branch() {
    // `v` is consumed when the branch runs, so the later read may see a moved value.
    // Accepting it released the vector's buffer twice.
    let source = r#"
func consume(v: Vec<i32>) -> i32 { v.len() as i32 }

func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    val c = true
    if c {
        val r = consume(v)
    }
    consume(v)
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a move in a branch that falls through must count after it"
    );
    assert!(
        stderr.contains("use of moved value 'v'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn a_move_before_a_break_or_continue_is_settled_by_the_loop() {
    // A `break` leaves with `v` moved, so `v` is moved past the loop; a `continue`
    // starts the next iteration with `v` moved, which would move it again; `break v`
    // hands `v` to the loop's value. Each was accepted and released the buffer twice.
    let cases = [
        (
            "loop {\n        if c {\n            val r = consume(v)\n            break\n        }\n    }\n    consume(v)",
            "use of moved value 'v'",
        ),
        (
            "mut t = 0\n    for i in 0..3 {\n        if c {\n            t = t + consume(v)\n            continue\n        }\n    }\n    t",
            "moved out inside a loop body",
        ),
        (
            "val w = loop { break v }\n    consume(v) + consume(w)",
            "use of moved value 'v'",
        ),
    ];
    for (body, expected) in cases {
        let source = format!(
            r#"
func consume(v: Vec<i32>) -> i32 {{ v.len() as i32 }}

func main() -> i32 {{
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    val c = true
    {body}
}}
"#
        );
        let (success, stderr) = check_source(&source);
        assert!(!success, "must be rejected:\n{source}");
        assert!(
            stderr.contains(expected),
            "expected `{expected}` for:\n{source}\ngot: {stderr}"
        );
    }
}

#[test]
fn a_move_in_a_branch_that_returns_does_not_reach_past_it() {
    let test = CompileTest::new();
    let source = r#"
func consume(s: string) -> i32 { s.len() as i32 }

func main() -> i32 {
    val s: string = "hi" + "!"
    val c = false
    if c {
        return consume(s)
    }
    s.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("move_conditional.nr", source)
        .expect("a move on a path that leaves keeps the binding on every other path");
    assert_eq!(exit_code, 3);
}

#[test]
fn copy_scalars_are_not_moved() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 5
    val b: i32 = a
    val c: i32 = a + b
    return c - 10
}
"#;
    let exit_code = test
        .compile_and_run("move_scalars.nr", source)
        .expect("scalar copy program should compile and run");
    assert_eq!(exit_code, 0);
}

// A move out of a binding declared outside a loop repeats on the next iteration.
// Before these landed, the checker restored the body's move state wholesale, so the
// program compiled and freed the same buffer once per iteration.

#[test]
fn regression_move_in_while_body_is_rejected_not_double_freed() {
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    mut i = 0
    while i < 2 {
        s = s + eat(b)
        i = i + 1
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a move repeated by a `while` body should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn regression_move_in_for_body_is_rejected() {
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    for i in 0..2 {
        s = s + eat(b)
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a move repeated by a `for` body should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn regression_move_of_drop_value_in_loop_body_is_rejected() {
    // The `Drop` case is the silent one: no allocator is involved, so the old
    // behaviour ran the destructor once per iteration without aborting.
    let source = r#"
struct Res { id: i32 }

impl Drop for Res {
    func drop(&mut self) {
        println("drop {self.id}")
    }
}

func eat(r: Res) -> i32 { return r.id }

func main() -> i32 {
    val r = Res { id: 7 }
    mut s = 0
    mut i = 0
    while i < 2 {
        s = s + eat(r)
        i = i + 1
    }
    return s
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a repeated move of a Drop value should be rejected"
    );
    assert!(
        stderr.contains("is moved out inside a loop body"),
        "expected the loop-move diagnostic, got: {stderr}"
    );
}

#[test]
fn move_in_loop_body_is_allowed_when_the_binding_is_given_a_fresh_value() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    mut b: Tensor<i32, [3]> = [1, 2, 3]
    mut s = 0
    mut i = 0
    while i < 3 {
        s = s + eat(b)
        b = [1, 2, 3]
        i = i + 1
    }
    return s - 3
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_reassigned.nr", source)
        .expect("a body that re-establishes the binding should compile and run");
    assert_eq!(exit_code, 0);
}

#[test]
fn move_in_loop_body_is_allowed_when_the_body_always_breaks() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    val b: Tensor<i32, [3]> = [7, 2, 3]
    mut s = 0
    mut i = 0
    while i < 3 {
        s = s + eat(b)
        break
    }
    return s - 7
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_break.nr", source)
        .expect("a body that always leaves the loop moves once and should compile");
    assert_eq!(exit_code, 0);
}

#[test]
fn move_of_a_binding_declared_inside_the_loop_body_is_allowed() {
    let test = CompileTest::new();
    let source = r#"
func eat(t: Tensor<i32, [3]>) -> i32 { return t[0] }

func main() -> i32 {
    mut s = 0
    mut i = 0
    while i < 3 {
        val local: Tensor<i32, [3]> = [2, 0, 0]
        s = s + eat(local)
        i = i + 1
    }
    return s - 6
}
"#;
    let exit_code = test
        .compile_and_run("move_loop_local.nr", source)
        .expect("a binding fresh each iteration should compile and run");
    assert_eq!(exit_code, 0);
}

// Regression: a move out of a struct FIELD was recorded nowhere, so the field
// could be consumed twice or read after being consumed. Both released the same
// buffer twice at run time, with no diagnostic.

#[test]
fn consuming_a_tensor_field_then_reading_it_is_rejected() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val t = l.w.t()
    return t[0, 1] + l.w[0, 0]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "reading a field after a shape method consumed it should be rejected"
    );
    assert!(
        stderr.contains("use of moved value 'l'"),
        "expected the whole binding to be reported moved, got: {stderr}"
    );
}

#[test]
fn consuming_a_tensor_field_twice_is_rejected() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val a = l.w.t()
    val b = l.w.t()
    return a[0, 1] + b[0, 1]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "a field cannot be consumed twice");
    assert!(
        stderr.contains("use of moved value 'l'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn passing_a_field_by_value_then_reading_it_is_rejected() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

func take(t: Tensor<i32, [2, 2]>) -> i32 { return t[0, 0] }

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val a = take(l.w)
    return a + l.w[0, 0]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a field passed by value is moved out of its struct"
    );
    assert!(
        stderr.contains("use of moved value 'l'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn consuming_a_field_through_a_borrowed_receiver_is_rejected() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

impl Layer {
    func flip(&self) -> i32 {
        val t = self.w.t()
        return t[0, 1]
    }
}

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    return l.flip() + l.flip()
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a &self method cannot consume a field its caller still owns"
    );
    assert!(
        stderr.contains("cannot move out of 'self'"),
        "expected a borrow diagnostic, got: {stderr}"
    );
}

#[test]
fn moving_out_of_a_dereferenced_borrow_is_rejected() {
    let source = r#"
func main() -> i32 {
    val s: string = "a" + "b"
    val r: &string = &s
    val x: string = *r
    return (x.len() as i32) + (s.len() as i32)
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "a borrow owns nothing to move out of");
    assert!(
        stderr.contains("cannot move out of 'r'"),
        "expected a borrow diagnostic, got: {stderr}"
    );
}

#[test]
fn a_struct_literal_field_value_is_moved() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

func main() -> i32 {
    val a: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val l = Layer { w: a }
    return l.w[0, 0] + a[0, 0]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "a value placed into a struct field is moved into it"
    );
    assert!(
        stderr.contains("use of moved value 'a'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn one_tensor_cannot_fill_two_fields_of_one_struct() {
    let source = r#"
struct Pair { a: Tensor<i32, [2, 2]>, b: Tensor<i32, [2, 2]> }

func main() -> i32 {
    val t: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val p = Pair { a: t, b: t }
    return p.a[0, 0] + p.b[0, 0]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(!success, "one buffer cannot have two owners");
    assert!(
        stderr.contains("use of moved value 't'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn struct_update_with_an_owned_field_moves_the_base() {
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]>, n: i32 }

func main() -> i32 {
    val a = Layer { w: [[1, 2], [3, 4]], n: 1 }
    val b = Layer { n: 2, ..a }
    return b.w[0, 0] + a.w[0, 0]
}
"#;
    let (success, stderr) = check_source(source);
    assert!(
        !success,
        "`..base` supplies an owned field, so the base is partially moved"
    );
    assert!(
        stderr.contains("use of moved value 'a'"),
        "expected a move diagnostic, got: {stderr}"
    );
}

#[test]
fn a_copy_field_does_not_move_its_struct() {
    let test = CompileTest::new();
    let source = r#"
struct Counter { n: i32, m: i32 }

func main() -> i32 {
    val c = Counter { n: 3, m: 4 }
    val a = c.n
    val b = c.m
    return a + b - 7
}
"#;
    let exit_code = test
        .compile_and_run("copy_field.nr", source)
        .expect("reading a Copy field moves nothing");
    assert_eq!(exit_code, 0);
}

#[test]
fn struct_update_with_only_copy_fields_leaves_the_base_usable() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32, y: i32 }

func main() -> i32 {
    val p = Point { x: 1, y: 2 }
    val q = Point { x: 10, ..p }
    return q.y + p.x - 3
}
"#;
    let exit_code = test
        .compile_and_run("copy_update.nr", source)
        .expect("an all-Copy `..base` moves nothing");
    assert_eq!(exit_code, 0);
}

#[test]
fn cloning_a_field_leaves_the_struct_usable() {
    let test = CompileTest::new();
    let source = r#"
struct Layer { w: Tensor<i32, [2, 2]> }

func main() -> i32 {
    val l = Layer { w: [[1, 2], [3, 4]] }
    val t = l.w.clone().t()
    return t[0, 1] + l.w[0, 0] - 4
}
"#;
    let exit_code = test
        .compile_and_run("clone_field.nr", source)
        .expect("a cloned field is the documented way to keep the original");
    assert_eq!(exit_code, 0);
}

#[test]
fn a_value_placed_into_an_enum_payload_is_moved() {
    // Every constructor that puts an owner into a new holder moves it: a tuple variant,
    // `Option::Some`, a struct variant and a newtype construction alike.
    for construct in [
        "val o = Box2::Full(v)",
        "val o = Option::Some(v)",
        "val o = Some(v)",
        "val o = Pair::Of { a: v }",
        "val o = Wrap(v)",
    ] {
        let source = format!(
            r#"
enum Box2 {{ Full(Vec<i32>), Empty }}
enum Pair {{ Of {{ a: Vec<i32> }} }}
newtype Wrap = Vec<i32>

func consume(v: Vec<i32>) -> i32 {{ v.len() as i32 }}

func main() -> i32 {{
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    {construct}
    consume(v)
}}
"#
        );
        let (success, stderr) = check_source(&source);
        assert!(!success, "`{construct}` must move `v`");
        assert!(
            stderr.contains("use of moved value 'v'"),
            "expected a move diagnostic for `{construct}`, got: {stderr}"
        );
    }
}
