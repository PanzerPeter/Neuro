// End-to-end tests for fixed-size arrays `[T; N]`: literals, indexing,
// element assignment, `.len()`, `for x in arr` / `for x in &arr` iteration, and
// the debug-build out-of-bounds panic.
use crate::compile_harness::CompileTest;

use std::process::Command;

#[test]
fn array_index_read_returns_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 4] = [10, 20, 30, 40]
    a[2]
}
"#;
    let exit = test
        .compile_and_run("array_index_read.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 30);
}

#[test]
fn array_element_assignment_mutates_in_place() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut a = [1, 2, 3]
    a[1] = 42
    a[1]
}
"#;
    let exit = test
        .compile_and_run("array_index_write.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn array_len_is_compile_time_length() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 7] = [0, 0, 0, 0, 0, 0, 0]
    a.len() as i32
}
"#;
    let exit = test
        .compile_and_run("array_len.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn for_in_array_sums_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 5] = [1, 2, 3, 4, 5]
    mut total: i32 = 0
    for x in a {
        total = total + x
    }
    total
}
"#;
    let exit = test
        .compile_and_run("array_for_value.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn for_in_array_borrow_sums_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut a = [4, 5, 6]
    a[0] = 1
    mut total: i32 = 0
    for x in &a {
        total = total + x
    }
    total
}
"#;
    let exit = test
        .compile_and_run("array_for_borrow.nr", source)
        .expect("compile/run failed");
    // a becomes [1, 5, 6] -> 12
    assert_eq!(exit, 12);
}

#[test]
fn array_passed_to_function_by_value() {
    let test = CompileTest::new();
    let source = r#"
func sum3(xs: [i32; 3]) -> i32 {
    xs[0] + xs[1] + xs[2]
}

func main() -> i32 {
    val a: [i32; 3] = [7, 8, 9]
    sum3(a)
}
"#;
    let exit = test
        .compile_and_run("array_param.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 24);
}

#[test]
fn array_typed_literal_coerces_element_width() {
    // The untyped integer literals default to i32 but are coerced into the
    // declared `[i64; 3]` element width by the initializer path.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i64; 3] = [1, 2, 3]
    a[2] as i32
}
"#;
    let exit = test
        .compile_and_run("array_i64.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);
}

#[test]
fn out_of_bounds_index_panics_in_debug_build() {
    // A debug build (`-O0`) bounds-checks every index and aborts with
    // a located diagnostic on an out-of-range access.
    let dir = std::env::temp_dir();
    let src = dir.join("neuro_array_oob.nr");
    let exe = dir.join("neuro_array_oob");
    std::fs::write(
        &src,
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    mut i: i32 = 9
    a[i]
}
"#,
    )
    .expect("write source");

    let compile = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg("0")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run neurc");
    assert!(
        compile.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&exe).output().expect("run executable");
    assert!(
        !run.status.success(),
        "an out-of-bounds index must abort, not exit cleanly"
    );
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("array index out of bounds"),
        "expected a bounds-check panic diagnostic, got: {stderr}"
    );
}

/// An out-of-range run-time index panics at every optimization level. The guards were
/// emitted only with debug-build overflow trapping, so `-O2` read and wrote past the end
/// of the storage, an undefined result that wrapping arithmetic never excuses.
#[test]
fn test_bug_068_out_of_range_index_panics_in_release_build() {
    let cases = [
        (
            "array read",
            "val a: [i32; 3] = [1, 2, 3]\n    a[far()] as i32",
            "array index out of bounds",
        ),
        (
            "array write",
            "mut a: [i32; 3] = [1, 2, 3]\n    a[far()] = 9\n    a[0]",
            "array index out of bounds",
        ),
        (
            "slice read",
            "val a: [i32; 3] = [1, 2, 3]\n    val s: &[i32] = a.slice(0..2)\n    s[far()]",
            "slice index out of bounds",
        ),
        (
            "tensor read",
            "val t: Tensor<i32, [3]> = [1, 2, 3]\n    t[far()]",
            "tensor index out of bounds",
        ),
    ];
    let dir = tempfile::TempDir::new().expect("temp dir");
    for (i, (shape, body, message)) in cases.iter().enumerate() {
        let src = dir.path().join(format!("oob_release_{i}.nr"));
        let exe = dir.path().join(format!("oob_release_{i}"));
        std::fs::write(
            &src,
            format!("func far() -> u64 {{ 700000 }}\n\nfunc main() -> i32 {{\n    {body}\n}}\n"),
        )
        .expect("write source");
        let compile = Command::new(neurc_path())
            .arg("compile")
            .arg(&src)
            .args(["-O", "2", "-o"])
            .arg(&exe)
            .output()
            .expect("run neurc");
        assert!(
            compile.status.success(),
            "{shape}: compile failed: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&exe).output().expect("run executable");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            !run.status.success() && stderr.contains(message),
            "{shape}: an out-of-range index must panic at -O2, got status {:?} and: {stderr}",
            run.status.code()
        );
    }
}

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}
