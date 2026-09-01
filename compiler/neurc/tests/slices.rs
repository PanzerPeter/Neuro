// End-to-end tests for borrowed slices `&[T]` / `&mut [T]`: the unsizing
// coercion from an array and a `Vec`, `.slice(range)` sub-ranges, `.len()`,
// indexing, iteration, writes through a mutable slice, and the runtime bounds
// panics on both the range and the index paths.
mod common;
use common::CompileTest;

use std::process::Command;

/// A `sum` over `&[i32]`, prepended to each program that needs it.
const SUM_OVER_SLICE: &str = r#"
func sum(xs: &[i32]) -> i32 {
    mut total: i32 = 0
    for x in xs {
        total = total + x
    }
    total
}
"#;

#[test]
fn one_signature_reads_an_array_a_sub_range_and_a_vec() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SUM_OVER_SLICE}
func main() -> i32 {{
    val fixed: [i32; 4] = [1, 2, 3, 4]
    mut grown: Vec<i32> = Vec::new()
    grown.push(10)
    grown.push(20)
    sum(&fixed) + sum(fixed.slice(1..3)) + sum(&grown)
}}
"#
    );
    let exit = test
        .compile_and_run("slice_one_signature.nr", &source)
        .expect("compile/run failed");
    // 10 (whole array) + 5 (elements 1..3) + 30 (the Vec).
    assert_eq!(exit, 45);
}

#[test]
fn slice_len_is_the_borrowed_run_not_the_container() {
    let test = CompileTest::new();
    let source = r#"
func size(xs: &[i32]) -> i32 {
    xs.len() as i32
}

func main() -> i32 {
    val a: [i32; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9]
    size(a.slice(2..5))
}
"#;
    let exit = test
        .compile_and_run("slice_len.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);
}

#[test]
fn slice_indexing_reads_through_the_offset() {
    let test = CompileTest::new();
    let source = r#"
func second(xs: &[i32]) -> i32 {
    xs[1]
}

func main() -> i32 {
    val a: [i32; 5] = [10, 20, 30, 40, 50]
    second(a.slice(2..5))
}
"#;
    let exit = test
        .compile_and_run("slice_index.nr", source)
        .expect("compile/run failed");
    // The view starts at element 2, so its element 1 is the array's element 3.
    assert_eq!(exit, 40);
}

#[test]
fn an_inclusive_range_includes_its_upper_bound() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SUM_OVER_SLICE}
func main() -> i32 {{
    val a: [i32; 5] = [1, 2, 3, 4, 5]
    sum(a.slice(1..=3))
}}
"#
    );
    let exit = test
        .compile_and_run("slice_inclusive.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn an_empty_range_yields_an_empty_slice() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SUM_OVER_SLICE}
func main() -> i32 {{
    val a: [i32; 3] = [1, 2, 3]
    sum(a.slice(3..3)) + (a.slice(3..3).len() as i32)
}}
"#
    );
    let exit = test
        .compile_and_run("slice_empty.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn writing_through_a_mutable_slice_reaches_the_owning_array() {
    let test = CompileTest::new();
    let source = r#"
func bump(xs: &mut [i32]) {
    mut i: u64 = 0
    while i < xs.len() {
        xs[i] = xs[i] + 1
        i = i + 1
    }
}

func main() -> i32 {
    mut a: [i32; 3] = [1, 2, 3]
    bump(&mut a)
    a[0] + a[1] + a[2]
}
"#;
    let exit = test
        .compile_and_run("slice_mut_write.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn writing_through_a_mutable_slice_reaches_the_owning_vec() {
    let test = CompileTest::new();
    let source = r#"
func zero_first(xs: &mut [i32]) {
    xs[0] = 0
}

func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(7)
    v.push(5)
    zero_first(&mut v)
    v[0] + v[1]
}
"#;
    let exit = test
        .compile_and_run("slice_mut_vec.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_slice_forwards_to_another_slice_parameter() {
    let test = CompileTest::new();
    let source = format!(
        r#"{SUM_OVER_SLICE}
func forward(xs: &[i32]) -> i32 {{
    sum(xs)
}}

func main() -> i32 {{
    val a: [i32; 4] = [4, 5, 6, 7]
    forward(a.slice(1..4))
}}
"#
    );
    let exit = test
        .compile_and_run("slice_forward.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

#[test]
fn enumerate_over_a_slice_counts_from_zero() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 5] = [10, 20, 30, 40, 50]
    mut acc: i32 = 0
    for (i, v) in a.slice(2..5).enumerate() {
        acc = acc + ((i as i32) * 10) + v
    }
    acc
}
"#;
    let exit = test
        .compile_and_run("slice_enumerate.nr", source)
        .expect("compile/run failed");
    // Values 30 + 40 + 50 = 120; positions 0, 1, 2 contribute 0 + 10 + 20.
    assert_eq!(exit, 150);
}

#[test]
fn an_out_of_bounds_range_panics() {
    let stderr = run_expecting_abort(
        "neuro_slice_range_oob",
        r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    val view = a.slice(1..9)
    view[0]
}
"#,
    );
    assert!(
        stderr.contains("slice range out of bounds"),
        "expected a range-guard diagnostic, got: {stderr}"
    );
}

#[test]
fn an_out_of_bounds_slice_index_panics_in_debug_build() {
    let stderr = run_expecting_abort(
        "neuro_slice_index_oob",
        r#"
func at(xs: &[i32], i: u64) -> i32 {
    xs[i]
}

func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    at(a.slice(0..2), 5)
}
"#,
    );
    assert!(
        stderr.contains("slice index out of bounds"),
        "expected a bounds-check diagnostic, got: {stderr}"
    );
}

/// Compile `source` at `-O0` under `name`, run it, and return the standard error
/// of a run that must abort. Panics if it compiles badly or exits cleanly.
fn run_expecting_abort(name: &str, source: &str) -> String {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("{name}.nr"));
    let exe = dir.join(name);
    std::fs::write(&src, source).expect("write source");

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
        "an out-of-bounds slice access must abort, not exit cleanly"
    );
    String::from_utf8_lossy(&run.stderr).into_owned()
}

/// Path to the `neurc` binary Cargo built for this test run.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}
