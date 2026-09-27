// End-to-end tests for `.step(n)` on a range: the strided `for` head, alone and under
// `.rev()`, at the edges of its element type, composed with the adapters a head already
// carries, as a tensor index axis, inside a `@grad` body, and the guard and diagnostics
// for a stride the language does not define.
use std::process::Command;

use crate::compile_harness::CompileTest;

/// BUG-031: the method was specified and did not exist, so this failed to compile.
#[test]
fn regression_bug_031_a_stepped_range_sums_every_second_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t = 0
    for i in (0..6).step(2) { t = t + i }
    t
}
"#;
    let exit = test
        .compile_and_run("step_basic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

/// `.rev()` sits under `.step(n)`: the walk starts at the last value and strides down,
/// so `(0..10).rev().step(4)` is 9, 5, 1 and not the reverse of 0, 4, 8.
#[test]
fn a_reversed_range_strides_down_from_its_last_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut digits = 0
    for i in (0..10).rev().step(4) { digits = digits * 10 + i }
    digits
}
"#;
    let exit = test
        .compile_and_run("step_rev.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 951 % 256);
}

/// An inclusive range still yields its bound when a stride lands exactly on it.
#[test]
fn an_inclusive_stride_lands_on_its_bound() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut last = -1
    mut count = 0
    for i in (0..=12).step(4) {
        last = i
        count = count + 1
    }
    count * 100 + last
}
"#;
    let exit = test
        .compile_and_run("step_inclusive.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 412 % 256);
}

/// Adding the stride before comparing would wrap from 250 back to 4 and never exit;
/// the loop compares the distance left instead.
#[test]
fn a_stride_near_the_top_of_its_type_terminates() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count = 0
    mut last: u8 = 0
    for i in (0u8..255u8).step(10) {
        count = count + 1
        last = i
    }
    for i in (0u8..=255u8).rev().step(100) { count = count + 1 }
    for i in (-100i8..=100i8).step(50) { count = count + 1 }
    if last != 250u8 { return 1 }
    count
}
"#;
    let exit = test
        .compile_and_run("step_edges.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 26 + 3 + 5);
}

#[test]
fn a_run_time_stride_and_bounds_are_honoured() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut stride = 3
    val hi = 10
    mut total = 0
    for i in (1..hi).step(stride) { total = total + i }
    total
}
"#;
    let exit = test
        .compile_and_run("step_runtime.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1 + 4 + 7);
}

/// The position counts iterations, not the stride, and adapters see the strided values.
#[test]
fn a_stepped_range_wears_enumerate_and_adapters() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut checks = 0
    for (k, v) in (10..20).step(5).enumerate() {
        if (k as i32) * 5 + 10 == v { checks = checks + 1 }
    }
    mut total = 0
    for v in (0..10).step(3).filter(|n: i32| -> bool { n > 0 }).map(|n: i32| -> i32 { n * 2 }) {
        total = total + v
    }
    checks * 100 + total
}
"#;
    let exit = test
        .compile_and_run("step_adapters.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 2 * 100 + 36);
}

#[test]
fn a_stepped_range_is_breakable_and_labelable() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut hits = 0
    outer: for i in (0..9).step(3) {
        for j in (0..9).rev().step(2) {
            if j == 4 { continue outer }
            if i == 6 { break outer }
            hits = hits + 1
        }
    }
    hits
}
"#;
    let exit = test
        .compile_and_run("step_labels.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn a_zero_stride_at_run_time_panics() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut stride = 0
    mut n = 0
    for i in (0..9).step(stride) { n = n + 1 }
    n
}
"#;
    let path = test.write_source("step_zero.nr", source);
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a zero stride should abort");
    assert!(
        stderr.contains("panic: range step must be positive"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn a_stepped_axis_keeps_every_nth_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [7]> = [1, 2, 3, 4, 5, 6, 7]
    val evens: Tensor<i32, [4]> = t[(0..7).step(2)]
    val back: Tensor<i32, [3]> = t[(0..7).rev().step(3)]
    val tail: Tensor<i32, [2]> = t[(2..=6).step(3)]
    mut ok = 0
    if evens[0] == 1 && evens[3] == 7 { ok = ok + 1 }
    if back[0] == 7 && back[1] == 4 && back[2] == 1 { ok = ok + 10 }
    if tail[0] == 3 && tail[1] == 6 { ok = ok + 100 }
    ok
}
"#;
    let exit = test
        .compile_and_run("step_axis.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 111);
}

/// Each axis strides independently, and a stepped axis composes with a position.
#[test]
fn stepped_axes_compose_with_positions_and_full_axes() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]]
    val corners: Tensor<i32, [2, 2]> = m[(0..3).rev().step(2), (0..4).step(3)]
    val row: Tensor<i32, [2]> = m[1, (1..4).step(2)]
    val cols: Tensor<i32, [3, 2]> = m[.., (0..4).step(2)]
    mut ok = 0
    if corners[0, 0] == 9 && corners[0, 1] == 12 { ok = ok + 1 }
    if corners[1, 0] == 1 && corners[1, 1] == 4 { ok = ok + 10 }
    if row[0] == 6 && row[1] == 8 { ok = ok + 100 }
    if cols[2, 1] == 11 { ok = ok + 1000 }
    ok % 256
}
"#;
    let exit = test
        .compile_and_run("step_axis_mix.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1111 % 256);
}

/// The replay has no run-time guard for a zero stride, so a `@grad` body takes only a
/// literal one and names the construct it refuses.
#[test]
fn a_run_time_stride_in_a_grad_body_is_refused() {
    let test = CompileTest::new();
    let source = r#"
@grad
func loss(w: &mut Tensor<f32, [4]>, k: i32) -> Tensor<f32, []> {
    mut s = 0.0f32
    for i in (0..4).step(k) { s = s + w[i] }
    return Tensor::scalar(s)
}

func main() -> i32 { 0 }
"#;
    let path = test.write_source("step_grad.nr", source);
    let error = test
        .compile(&path)
        .expect_err("a run-time stride in a @grad body should be refused");
    assert!(
        error.contains("stride is not an integer literal"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn step_on_a_sequence_head_is_reported() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    for x in a.step(2) {
        return x
    }
    0
}
"#;
    let error = test
        .check("step_on_array.nr", source)
        .expect_err("a stepped array head should be rejected");
    assert!(
        error.contains("`.step(n)` strides over a range"),
        "unexpected diagnostic: {error}"
    );
}
