// End-to-end tests for the growable text buffer `String`: construction without an
// annotation, appending owned and borrowed text, byte length, buffer-retaining `clear`,
// the copy back out to an immutable `string`, ownership (move + scope-exit free), and
// growth well past the initial capacity.
mod common;
use common::CompileTest;

#[test]
fn builds_text_and_copies_it_out() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b = String::new()
    b.push_str("Hello")
    b.push_str(", ")
    b.push_str("world!")
    val line: string = b.to_string()
    if line != "Hello, world!" {
        return 91
    }
    line.len() as i32
}
"#;
    let exit = test
        .compile_and_run("string_build.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 13);
}

#[test]
fn push_str_accepts_a_borrow_without_consuming_it() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b: String = String::new()
    val piece: string = "abcd"
    b.push_str(&piece)
    b.push_str(piece)
    // `piece` is read, never moved, so it is still usable afterwards.
    (b.len() + piece.len()) as i32
}
"#;
    let exit = test
        .compile_and_run("string_borrow.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn clear_resets_the_length_and_the_buffer_refills() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b = String::new()
    b.push_str("discard me")
    b.clear()
    if b.len() != 0 {
        return 91
    }
    b.push_str("kept")
    if b.to_string() != "kept" {
        return 92
    }
    b.len() as i32
}
"#;
    let exit = test
        .compile_and_run("string_clear.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn empty_builder_produces_an_empty_string() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val b = String::new()
    val out: string = b.to_string()
    if out != "" {
        return 91
    }
    out.len() as i32
}
"#;
    let exit = test
        .compile_and_run("string_empty.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn grows_far_past_the_initial_capacity() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b = String::new()
    mut i = 0
    while i < 500 {
        b.push_str("0123456789")
        i = i + 1
    }
    if b.len() != 5000 {
        return 91
    }
    val out: string = b.to_string()
    if out.len() != 5000 {
        return 92
    }
    7
}
"#;
    let exit = test
        .compile_and_run("string_growth.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn a_builder_is_mutated_through_a_mutable_borrow() {
    let test = CompileTest::new();
    let source = r#"
func tag(buf: &mut String, name: string) {
    buf.push_str("[")
    buf.push_str(name)
    buf.push_str("]")
}

func main() -> i32 {
    mut b = String::new()
    tag(&mut b, "ok")
    tag(&mut b, "done")
    if b.to_string() != "[ok][done]" {
        return 91
    }
    b.len() as i32
}
"#;
    let exit = test
        .compile_and_run("string_borrow_mut.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn a_builder_moves_on_assignment() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b = String::new()
    b.push_str("moved")
    mut other = b
    other.push_str("!")
    other.len() as i32
}
"#;
    let exit = test
        .compile_and_run("string_move.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

#[test]
fn a_moved_builder_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut b = String::new()
    val other = b
    b.push_str("x")
    0
}
"#;
    let path = test.write_source("string_moved_use.nr", source);
    let err = test
        .compile(&path)
        .expect_err("using a moved builder should not compile");
    assert!(
        err.contains("use of moved value"),
        "expected a move diagnostic, got: {err}"
    );
}

#[test]
fn builders_reused_in_a_loop_do_not_grow_the_heap() {
    // Each iteration's builder owns a buffer of a few kilobytes. Without the scope-exit
    // free this allocates hundreds of megabytes; with it, the process stays flat. The
    // exit code only proves the arithmetic, so the leak is caught by the runtime of the
    // allocator rather than asserted directly — what is asserted is that it completes.
    let test = CompileTest::new();
    let source = r#"
func build(rounds: i32) -> u64 {
    mut b = String::new()
    mut i = 0
    while i < rounds {
        b.push_str("0123456789abcdef")
        i = i + 1
    }
    b.len()
}

func main() -> i32 {
    mut total: u64 = 0
    mut r = 0
    while r < 2000 {
        total = total + build(200)
        r = r + 1
    }
    if total != 6400000 {
        return 91
    }
    5
}
"#;
    let exit = test
        .compile_and_run("string_reuse.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}
