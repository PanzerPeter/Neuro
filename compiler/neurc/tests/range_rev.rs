// End-to-end tests for `.rev()` on a range: descending `for`-head iteration, the
// reversed axis of a tensor index, how the two compose with the adapters and
// bindings a head already carries, and the diagnostics for a receiver `.rev()`
// does not apply to.
mod common;
use common::CompileTest;

#[test]
fn an_exclusive_range_counts_down_from_its_last_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut digits = 0
    for i in (0..3).rev() {
        digits = digits * 10 + i
    }
    digits
}
"#;
    let exit = test
        .compile_and_run("rev_exclusive.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 210);
}

/// An inclusive range names its last value, so reversing it starts there rather than
/// one below, which is the only place the two spellings differ.
#[test]
fn an_inclusive_range_starts_at_its_upper_bound() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut first = -1
    for i in (0..=4).rev() {
        if first < 0 { first = i }
    }
    first
}
"#;
    let exit = test
        .compile_and_run("rev_inclusive.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn an_empty_range_reversed_runs_no_iterations() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut runs = 7
    for i in (5..5).rev() {
        runs = runs + 1
    }
    runs
}
"#;
    let exit = test
        .compile_and_run("rev_empty.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

/// The binding is mirrored off an ascending counter precisely so this terminates:
/// counting the binding itself down would step below zero to stop, and an unsigned
/// step below zero wraps to the top of the type instead.
#[test]
fn an_unsigned_range_reaching_zero_terminates() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: u8 = 0
    for i in (0u8..4u8).rev() {
        total = total + i
    }
    total as i32
}
"#;
    let exit = test
        .compile_and_run("rev_unsigned.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

/// A range whose bounds are run-time values, which is what proves the mirror is
/// computed rather than folded.
#[test]
fn a_reversed_range_over_variable_bounds_walks_down() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val lo = 1
    val hi = 3
    mut trace = 0
    for i in (lo..hi).rev() {
        trace = trace * 10 + i
    }
    trace
}
"#;
    let exit = test
        .compile_and_run("rev_variable.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

/// The position keeps counting iterations up from zero while the value counts down,
/// so the two bindings run in opposite directions over one head.
#[test]
fn an_enumerated_reversed_range_counts_the_position_up() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut mirrored = 0
    mut first = -1
    for (k, v) in (0..3).rev().enumerate() {
        if (k as i32) + v == 2 { mirrored = mirrored + 1 }
        if first < 0 { first = v }
    }
    mirrored * 10 + first
}
"#;
    let exit = test
        .compile_and_run("rev_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 32);
}

#[test]
fn a_reversed_range_wears_adapters() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    mut first = -1
    for v in (0..6).rev().filter(|n: i32| -> bool { n % 2 == 1 }).map(|n: i32| -> i32 { n * 2 }) {
        total = total + v
        if first < 0 { first = v }
    }
    total * 10 + first
}
"#;
    let exit = test
        .compile_and_run("rev_adapters.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 190);
}

#[test]
fn a_reversed_range_is_breakable_and_labelable() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut hits = 0
    outer: for i in (0..4).rev() {
        for j in (0..4).rev() {
            if j < i { continue outer }
            hits = hits + 1
        }
    }
    hits
}
"#;
    let exit = test
        .compile_and_run("rev_labels.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn a_reversed_axis_reads_a_tensor_back_to_front() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [5]> = [1, 2, 3, 4, 5]
    val r: Tensor<i32, [5]> = t[(0..5).rev()]
    r[0] * 10 + r[4]
}
"#;
    let exit = test
        .compile_and_run("rev_axis.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 51);
}

/// The base offset already carries the range's start, so a reversed sub-range must
/// reflect about its own extent and not about the axis's.
#[test]
fn a_reversed_sub_range_stays_inside_its_own_bounds() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [5]> = [1, 2, 3, 4, 5]
    val r: Tensor<i32, [3]> = t[(1..4).rev()]
    mut ok = 0
    if r[0] == 4 { ok = ok + 1 }
    if r[1] == 3 { ok = ok + 10 }
    if r[2] == 2 { ok = ok + 100 }
    ok
}
"#;
    let exit = test
        .compile_and_run("rev_sub_range.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 111);
}

#[test]
fn reversed_axes_compose_with_positions_and_full_axes() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val row: Tensor<i32, [3]> = m[1, (0..3).rev()]
    val both: Tensor<i32, [2, 3]> = m[(0..2).rev(), (0..3).rev()]
    val rows: Tensor<i32, [2, 3]> = m[(0..2).rev(), ..]
    mut ok = 0
    if row[0] == 6 { ok = ok + 1 }
    if both[0, 0] == 6 { ok = ok + 10 }
    if both[1, 2] == 1 { ok = ok + 100 }
    if rows[0, 0] == 4 { ok = ok + 1000 }
    ok % 256
}
"#;
    let exit = test
        .compile_and_run("rev_axis_mix.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1111 % 256);
}

/// `.rev()` is scoped to ranges. The receiver is checked where the head is parsed,
/// so the diagnostic names the spelling that works rather than a missing method.
#[test]
fn rev_on_a_sequence_head_is_reported() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    for x in a.rev() {
        return x
    }
    0
}
"#;
    let error = test
        .check("rev_on_array.nr", source)
        .expect_err("a reversed array head should be rejected");
    assert!(
        error.contains("`.rev()` reverses a range"),
        "unexpected diagnostic: {error}"
    );
}
