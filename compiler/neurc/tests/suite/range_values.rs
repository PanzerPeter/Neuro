// End-to-end tests for a range written as a value: `start..end` and `start..=end` build the
// prelude's `Range<T>`, which binds, passes and returns; `.rev()` and `.step(n)` answer
// `Rev<T>` and `Step<T>`; each walks the values its `for`-head form walks, to the edges of
// every integer type; adapters and generic consumers take them as any iterator; and the
// diagnostics for what a range value cannot do.
use crate::compile_harness::{CompileTest, assert_check_error, assert_exit, stdout_of};

use std::process::Command;

/// Print every value `it` yields on one line.
const SHOW: &str = r#"
func show<I: Iterator<Item = i32>>(it: I) {
    for x in it { print("{x} ") }
    println("")
}
"#;

#[test]
fn a_range_value_binds_passes_and_returns() {
    let source = format!(
        "{SHOW}{}",
        r#"
func up_to(n: i32) -> Range<i32> {
    0..n
}
func down(r: Range<i32>) -> Rev<i32> {
    r.rev()
}
func every(n: i32, k: i32) -> Step<i32> {
    (1..=n).step(k)
}
func main() -> i32 {
    val r = 2..5
    show(r)
    show(up_to(4))
    show(down(0..=3))
    show(every(10, 3))
    show((0..10).rev().step(4))
    0
}
"#
    );
    assert_eq!(
        stdout_of("range_values_basic.nr", &source),
        "2 3 4 \n0 1 2 3 \n3 2 1 0 \n1 4 7 10 \n9 5 1 \n"
    );
}

/// A value walks exactly what the head form walks, including every empty case.
#[test]
fn a_range_value_walks_what_the_head_form_walks() {
    let source = r#"
func main() -> i32 {
    mut mismatches = 0
    for a in -3..4 {
        for b in -3..4 {
            for k in 1..4 {
                mut head = 0
                for x in (a..b).step(k) { head = head * 7 + x + 5 }
                val up = (a..b).step(k)
                mut value = 0
                for x in up { value = value * 7 + x + 5 }
                mut head_down = 0
                for x in (a..=b).rev().step(k) { head_down = head_down * 7 + x + 5 }
                val down = (a..=b).rev().step(k)
                mut value_down = 0
                for x in down { value_down = value_down * 7 + x + 5 }
                if value != head || value_down != head_down {
                    mismatches += 1
                }
            }
        }
    }
    mismatches
}
"#;
    assert_exit("range_values_parity.nr", source, 0);
}

/// The distance between two values of any integer type fits the walk's `u64`, so no
/// step crosses the type's last value or wraps below its first.
#[test]
fn a_range_value_reaches_the_edges_of_its_type() {
    let source = r#"
func main() -> i32 {
    val top: Range<u8> = 250..=255
    for x in top { print("{x} ") }
    println("")
    for x in ((0 as u8)..=3).rev() { print("{x} ") }
    println("")
    for x in ((-128 as i8)..=127).step(100) { print("{x} ") }
    println("")
    for x in ((-128 as i8)..=127).rev().step(127) { print("{x} ") }
    println("")
    val wide = (0 as u64)..=18446744073709551615
    for x in wide.rev().step(9223372036854775807) { print("{x} ") }
    println("")
    0
}
"#;
    assert_eq!(
        stdout_of("range_values_edges.nr", source),
        "250 251 252 253 254 255 \n3 2 1 0 \n-128 -28 72 \n127 0 -127 \n18446744073709551615 9223372036854775808 1 \n"
    );
}

#[test]
fn an_empty_range_value_yields_nothing() {
    let source = format!(
        "{SHOW}{}",
        r#"
func main() -> i32 {
    show(5..5)
    show(5..=4)
    show((5..5).rev())
    show((3..1).step(2))
    show((0..=0).rev().step(5))
    0
}
"#
    );
    assert_eq!(stdout_of("range_values_empty.nr", &source), "\n\n\n\n0 \n");
}

/// The element type follows the bounds, and a `Range<T>` the value is expected to be types
/// an unsuffixed literal.
#[test]
fn the_element_type_follows_the_bounds_or_the_expected_type() {
    let source = r#"
func total(r: Range<u64>) -> u64 {
    mut s: u64 = 0
    for x in r { s += x }
    s
}
func main() -> i32 {
    val n: u64 = 4
    val counted = 0..n
    val typed: Range<i64> = 1..3
    mut t: i64 = 0
    for x in typed { t += x }
    (total(counted) + total(0..=2)) as i32 + t as i32
}
"#;
    assert_exit("range_values_types.nr", source, 12);
}

/// `next` takes `&mut self`, so a borrowed head drives the value in place: a `break`
/// leaves it where it stopped, and `.rev()` / `.step(n)` take what is left.
#[test]
fn a_partly_walked_range_keeps_what_is_left() {
    let source = r#"
func main() -> i32 {
    mut r = 0..10
    mut s = 0
    for x in &mut r {
        if x == 3 { break }
        s += x
    }
    val first = r.next()
    for x in r.rev().step(4) { s += x * 100 }
    match first {
        Option::Some(v) => println("{s} {v}"),
        Option::None => println("none")
    }
    0
}
"#;
    // 0 + 1 + 2, then 4 taken by `next`, then 9 and 5 from the top down.
    assert_eq!(stdout_of("range_values_partial.nr", source), "1403 4\n");
}

/// A range value is an `Iterator`, so the adapters and a generic consumer take it, and a
/// head form may decorate a bound one.
#[test]
fn a_range_value_meets_adapters_and_generic_consumers() {
    let source = format!(
        "{SHOW}{}",
        r#"
func sum_all<I: Iterator<Item = i32>>(it: I) -> i32 {
    mut s = 0
    for x in it { s += x }
    s
}
func main() -> i32 {
    show((1..=4).map(|v| v * v))
    show((0..10).rev().filter(|v| v % 3 == 0))
    val r = 0..4
    for (i, x) in r.rev().enumerate() { print("{i}:{x} ") }
    println("")
    val evens = (0..7).step(2)
    for x in evens.map(|v| v + 1) { print("{x} ") }
    println("")
    sum_all(1..=10)
}
"#
    );
    let test = CompileTest::new();
    let path = test.write_source("range_values_adapters.nr", &source);
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run binary");
    assert_eq!(output.status.code(), Some(55));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "1 4 9 16 \n9 6 3 0 \n0:3 1:2 2:1 3:0 \n1 3 5 7 \n"
    );
}

#[test]
fn a_range_value_moves() {
    assert_check_error(
        "range_values_moved.nr",
        r#"
func main() -> i32 {
    val r = 0..3
    for x in r { }
    for x in r { }
    0
}
"#,
        "use of moved value 'r'",
    );
}

#[test]
fn a_range_value_needs_integer_bounds() {
    assert_check_error(
        "range_values_float.nr",
        "func main() -> i32 {\n    val r = 0.5..2.0\n    0\n}\n",
        "a range bound or `.step(n)` stride must be an integer type, found f64",
    );
}

/// Striding down from the top and reversing an upward stride visit different values, and
/// only the first is written `.rev().step(n)`: a `Step` has no `.rev()`, as a value or in
/// a head.
#[test]
fn rev_after_step_is_refused() {
    for (name, body) in [
        ("value", "val s = (0..9).step(3).rev()"),
        ("head", "for x in (0..9).step(3).rev() { }"),
        ("twice", "val s = (0..9).rev().rev()"),
    ] {
        assert_check_error(
            &format!("range_values_rev_{name}.nr"),
            &format!("func main() -> i32 {{\n    {body}\n    0\n}}\n"),
            "`.rev()` reverses a range, so its receiver must be one",
        );
    }
    assert_check_error(
        "range_values_step_twice.nr",
        "func main() -> i32 {\n    val s = (0..9).step(3).step(2)\n    0\n}\n",
        "`.step(n)` strides over a range, so its receiver must be one",
    );
}

/// The range-only methods keep their diagnostic on every other receiver, in a head and
/// outside one alike.
#[test]
fn rev_and_step_on_a_non_range_name_the_range_spelling() {
    for body in ["for x in xs.rev() { }", "val r = xs.rev()"] {
        assert_check_error(
            "range_values_array_rev.nr",
            &format!("func main() -> i32 {{\n    val xs = [1, 2]\n    {body}\n    0\n}}\n"),
            "`.rev()` reverses a range, so its receiver must be one",
        );
    }
    for body in ["for x in xs.step(2) { }", "val r = xs.step(2)"] {
        assert_check_error(
            "range_values_array_step.nr",
            &format!("func main() -> i32 {{\n    val xs = [1, 2]\n    {body}\n    0\n}}\n"),
            "`.step(n)` strides over a range, so its receiver must be one",
        );
    }
}

#[test]
fn a_constant_stride_below_one_is_refused() {
    for stride in ["0", "-2"] {
        assert_check_error(
            "range_values_bad_stride.nr",
            &format!("func main() -> i32 {{\n    val s = (0..9).rev().step({stride})\n    0\n}}\n"),
            &format!("`.step(n)` must be a positive integer, found {stride}"),
        );
    }
}

#[test]
fn a_zero_stride_at_run_time_panics() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut stride = 0
    val s = (0..9).step(stride)
    0
}
"#;
    let path = test.write_source("range_values_zero.nr", source);
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "a zero stride should abort");
    assert!(
        stderr.contains("panic: range step must be positive"),
        "unexpected stderr: {stderr}"
    );
}

/// The walk's state and its intrinsics are the prelude's own.
#[test]
fn the_walk_is_private_to_the_prelude() {
    assert_check_error(
        "range_values_field.nr",
        "func main() -> i32 {\n    val r = 0..3\n    r.start\n}\n",
        "field 'start' of struct 'Range<i32>' is private",
    );
    assert_check_error(
        "range_values_intrinsic.nr",
        "func main() -> i32 {\n    val x = 3\n    val y = x.__to_u64()\n    0\n}\n",
        "has no method '__to_u64'",
    );
}

/// A program's own `Range` displaces the prelude's, and with it the range value; a range
/// still works in a `for` head and as a slice argument.
#[test]
fn a_program_range_displaces_the_value() {
    assert_check_error(
        "range_values_displaced.nr",
        "struct Range { a: i32 }\nfunc main() -> i32 {\n    val r = 0..3\n    0\n}\n",
        "a range value is the prelude's `Range`",
    );
    let source = r#"
struct Range { a: i32 }
func main() -> i32 {
    val xs = [1, 2, 3, 4]
    mut s = 0
    for x in xs.slice(1..3) { s += x }
    for i in (0..3).rev() { s += i }
    s + Range { a: 1 }.a
}
"#;
    assert_exit("range_values_own_range.nr", source, 9);
}

/// The prelude's `step` shares its name with a program's own: a labelled call binds the
/// program's labels, and a positional one reaches whichever method its receiver has.
#[test]
fn a_program_step_method_coexists_with_the_range_one() {
    let source = r#"
struct Acc { total: i32 }
impl Acc {
    func step(&mut self, add a: i32, times t: i32) {
        self.total = self.total + a * t
    }
}
func main() -> i32 {
    mut acc = Acc { total: 1 }
    acc.step(times: 5, add: 8)
    acc.step(1, 2)
    for x in (0..7).step(3) { acc.total += x }
    acc.total
}
"#;
    assert_exit("range_values_own_step.nr", source, 52);
}
