// End-to-end tests for the `.map(f)` / `.filter(p)` adapters a `for` head may wear:
// every head shape they apply to, how a chain composes, how they interact with
// `.enumerate()` and with the loop's other machinery (labels, `continue`, nesting),
// and the diagnostics for a malformed adapter.
mod common;
use common::CompileTest;

use std::process::Command;

#[test]
fn map_transforms_every_array_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    for v in [1, 2, 3, 4].map(|x: i32| -> i32 { x * 3 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("map_array.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 30);
}

#[test]
fn filter_skips_the_elements_its_predicate_rejects() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    for v in (0..10).filter(|x: i32| -> bool { x % 3 == 0 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("filter_range.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

/// The chain runs left to right, so a filter sees what the map before it produced.
#[test]
fn a_chain_composes_in_source_order() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    for v in [1, 2, 3, 4, 5].map(|x: i32| -> i32 { x * 2 }).filter(|x: i32| -> bool { x > 5 }).map(|x: i32| -> i32 { x + 1 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_chain.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 27);
}

/// `.map` retypes the binding, so the loop variable is whatever the function returns.
#[test]
fn map_may_change_the_element_type() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0.0
    for v in [1, 2, 3].map(|x: i32| -> f64 { x as f64 * 1.5 }) {
        total += v
    }
    total as i32
}
"#;
    let exit = test
        .compile_and_run("map_retypes.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn adapters_apply_to_a_vec_head() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut xs: Vec<i32> = Vec::new()
    xs.push(4)
    xs.push(7)
    xs.push(9)
    mut total = 0
    for v in xs.filter(|x: i32| -> bool { x % 2 == 1 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_vec.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 16);
}

#[test]
fn adapters_apply_to_a_borrowed_slice_head() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val xs: [i32; 5] = [1, 2, 3, 4, 5]
    val window = xs.slice(1..4)
    mut total = 0
    for v in window.map(|x: i32| -> i32 { x * 10 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_slice.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 90);
}

/// The protocol path desugars to a `while` over `.next()`; the chain folds into its
/// yielding arm exactly as it folds into a counted body.
#[test]
fn adapters_apply_to_a_protocol_head() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct CountIter { at: i32, end: i32 }

impl Iterator for CountIter {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.at >= self.end { return Option::None }
        val current = self.at
        self.at = self.at + 1
        Option::Some(current)
    }
}

@derive(Copy, Clone)
struct Count { end: i32 }

impl IntoIterator for Count {
    type Item = i32
    type Iter = CountIter
    func into_iter(self) -> CountIter {
        CountIter { at: 0, end: self.end }
    }
}

func main() -> i32 {
    mut total = 0
    for v in (Count { end: 8 }).filter(|x: i32| -> bool { x % 2 == 0 }).map(|x: i32| -> i32 { x * 2 }) {
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_protocol.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 24);
}

/// The position an enumerated adapted head binds counts what the CHAIN yielded, not
/// how many source elements were stepped over.
#[test]
fn enumerate_after_a_filter_counts_yielded_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut positions = 0
    mut values = 0
    for (i, v) in [10, 3, 20, 4, 30].filter(|x: i32| -> bool { x >= 10 }).enumerate() {
        positions += i as i32
        values += v
    }
    positions * 10 + values
}
"#;
    let exit = test
        .compile_and_run("adapter_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 90);
}

/// A `continue` in the user's body must not skip the position advance, or the next
/// yielded element would repeat an index.
#[test]
fn a_continue_in_the_body_does_not_repeat_a_position() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut last = 0
    for (i, v) in (0..6).filter(|x: i32| -> bool { x % 2 == 0 }).enumerate() {
        if v == 0 {
            continue
        }
        last = i as i32
    }
    last
}
"#;
    let exit = test
        .compile_and_run("adapter_continue.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn a_labeled_break_leaves_an_adapted_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    outer: for v in (0..20).filter(|x: i32| -> bool { x % 3 == 0 }) {
        for inner in 0..3 {
            if v > 9 {
                break outer
            }
        }
        total += v
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_labeled_break.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

/// Two adapted loops in one function get their own generated bindings, so neither
/// shadows the other.
#[test]
fn nested_adapted_loops_keep_separate_bindings() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    for outer in [1, 2].map(|x: i32| -> i32 { x * 10 }) {
        for inner in [1, 2, 3].filter(|x: i32| -> bool { x > 1 }) {
            total += outer + inner
        }
    }
    total
}
"#;
    let exit = test
        .compile_and_run("adapter_nested.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 70);
}

/// The adapter's function is read, not moved: a closure binding used by a head stays
/// usable after the loop.
#[test]
fn an_adapter_function_binding_survives_the_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val double = |x: i32| -> i32 { x * 2 }
    mut total = 0
    for v in [1, 2, 3].map(double) {
        total += v
    }
    total + double(5)
}
"#;
    let exit = test
        .compile_and_run("adapter_reuse.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 22);
}

#[test]
fn a_non_function_adapter_argument_is_rejected() {
    assert_check_error(
        "adapter_not_callable.nr",
        r#"
func main() -> i32 {
    for x in [1, 2].map(3) {
        return x
    }
    0
}
"#,
        "needs a function of one parameter",
    );
}

#[test]
fn an_adapter_over_the_wrong_element_type_is_rejected() {
    assert_check_error(
        "adapter_wrong_input.nr",
        r#"
func main() -> i32 {
    for x in [1, 2].map(|b: bool| -> i32 { 1 }) {
        return x
    }
    0
}
"#,
        "is applied to elements of type i32",
    );
}

#[test]
fn a_filter_predicate_that_is_not_bool_is_rejected() {
    assert_check_error(
        "adapter_bad_predicate.nr",
        r#"
func main() -> i32 {
    for x in [1, 2].filter(|x: i32| -> i32 { x }) {
        return x
    }
    0
}
"#,
        "needs a function returning bool",
    );
}

#[test]
fn an_adapter_without_one_argument_is_rejected() {
    assert_check_error(
        "adapter_arity.nr",
        r#"
func main() -> i32 {
    for x in [1, 2].map() {
        return x
    }
    0
}
"#,
        "takes exactly one argument",
    );
}

/// Assert `neurc check` rejects `source` with a diagnostic containing `expected`.
fn assert_check_error(filename: &str, source: &str, expected: &str) {
    let test = CompileTest::new();
    let path = test.write_source(filename, source);
    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&path)
        .output()
        .expect("run neurc");
    assert!(
        !output.status.success(),
        "expected `{filename}` to be rejected"
    );
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains(expected),
        "expected a diagnostic containing {expected:?}, got: {text}"
    );
}

/// Path to the `neurc` binary Cargo built for this test run.
fn neurc_path() -> &'static str {
    env!("CARGO_BIN_EXE_neurc")
}
