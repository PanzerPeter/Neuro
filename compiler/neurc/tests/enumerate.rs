// End-to-end tests for `.enumerate()` in a `for` head: the position binding it
// introduces over arrays, ranges, and `Vec`, how that binding interacts with the
// loop's other machinery (labels, `continue`, shadowing, closures), and the
// diagnostics for a head whose arity disagrees with its iterable.
mod common;
use common::CompileTest;

use std::process::Command;

#[test]
fn array_enumerate_binds_position_and_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 4] = [10, 20, 30, 40]
    mut total = 0
    for (i, x) in a.enumerate() {
        total += (i as i32) * x
    }
    total
}
"#;
    let exit = test
        .compile_and_run("array_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 200);
}

/// The position counts iterations from zero rather than echoing the range's own
/// values, so it stays a position when the range does not start at zero.
#[test]
fn range_enumerate_counts_from_zero() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut positions = 0
    mut values = 0
    for (i, n) in (5..9).enumerate() {
        positions += i as i32
        values += n
    }
    positions * 10 + values
}
"#;
    let exit = test
        .compile_and_run("range_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6 * 10 + 26);
}

#[test]
fn inclusive_range_enumerate_covers_the_upper_bound() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut last = 0
    for (i, n) in (1..=4).enumerate() {
        last = (i as i32) * 10 + n
    }
    last
}
"#;
    let exit = test
        .compile_and_run("inclusive_range_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 34);
}

#[test]
fn vec_enumerate_binds_position_and_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(7)
    v.push(8)
    v.push(9)
    mut total = 0
    for (i, x) in v.enumerate() {
        total += (i as i32) * x
    }
    total
}
"#;
    let exit = test
        .compile_and_run("vec_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 26);
}

/// A borrowed receiver iterates without consuming, exactly as `for x in &a` does.
#[test]
fn borrowed_array_enumerates() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    mut total = 0
    for (i, x) in (&a).enumerate() {
        total += (i as i32) + x
    }
    total + a[0]
}
"#;
    let exit = test
        .compile_and_run("borrowed_array_enumerate.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

/// The position is `u64`, which is what an index expression takes, so it reaches
/// back into the sequence it walks without a cast.
#[test]
fn position_indexes_its_own_sequence() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: [i32; 3] = [5, 6, 7]
    mut total = 0
    for (i, x) in a.enumerate() {
        total += a[i] + x
    }
    total
}
"#;
    let exit = test
        .compile_and_run("enumerate_self_index.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 36);
}

/// `continue` jumps to the step block, which is where the position advances —
/// skipping a body must not skip a position.
#[test]
fn continue_still_advances_the_position() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut last = 0
    for (i, x) in [1, 2, 3, 4].enumerate() {
        if x % 2 == 0 {
            continue
        }
        last = i as i32
    }
    last
}
"#;
    let exit = test
        .compile_and_run("enumerate_continue.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn labeled_break_leaves_an_enumerated_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    outer: for (i, x) in [1, 2, 3].enumerate() {
        for (j, y) in [10, 20].enumerate() {
            if i == 1 {
                break outer
            }
            total += (j as i32) + x + y
        }
    }
    total
}
"#;
    let exit = test
        .compile_and_run("enumerate_labeled_break.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 33);
}

/// The bindings are loop-local: an outer name they shadow means what it always
/// meant once the loop exits.
#[test]
fn position_binding_restores_the_name_it_shadows() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val i = 100
    mut total = 0
    for (i, x) in [1, 2].enumerate() {
        total += (i as i32) + x
    }
    total + i
}
"#;
    let exit = test
        .compile_and_run("enumerate_shadowing.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 104);
}

#[test]
fn closure_captures_the_position_binding() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total = 0
    for (i, x) in [4, 5].enumerate() {
        val weigh = |m: i32| -> i32 { m * ((i as i32) + x) }
        total += weigh(2)
    }
    total
}
"#;
    let exit = test
        .compile_and_run("enumerate_closure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 20);
}

#[test]
fn pair_head_without_enumerate_is_rejected() {
    assert_check_error(
        "pair_without_enumerate.nr",
        r#"
func main() -> i32 {
    for (i, x) in [1, 2] {
        return x
    }
    0
}
"#,
        "add `.enumerate()`",
    );
}

#[test]
fn enumerate_without_a_pair_head_is_rejected() {
    assert_check_error(
        "enumerate_without_pair.nr",
        r#"
func main() -> i32 {
    for x in [1, 2].enumerate() {
        return x
    }
    0
}
"#,
        "bind both with a pair pattern",
    );
}

#[test]
fn enumerate_with_arguments_is_rejected() {
    assert_check_error(
        "enumerate_with_arguments.nr",
        r#"
func main() -> i32 {
    for (i, x) in [1, 2].enumerate(3) {
        return x
    }
    0
}
"#,
        "takes no arguments",
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
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable suffix.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}
