// Labeled break / continue tests: `'outer: for ... { break 'outer }`.
use crate::compile_harness::CompileTest;

#[test]
fn test_labeled_break_exits_outer_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count: i32 = 0
    'outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            if i + j >= 3 {
                break 'outer
            }
        }
    }
    return count
}
"#;

    let exit_code = test
        .compile_and_run("labeled_break_outer.nr", source)
        .expect("Compilation or execution failed");
    // i=0: j=0..3 -> 4th iteration (i+j==3) breaks the outer loop entirely.
    assert_eq!(exit_code, 4, "labeled break should exit the outer loop");
}

#[test]
fn test_labeled_continue_skips_to_outer_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count: i32 = 0
    'outer: for i in 0..3 {
        for j in 0..3 {
            if j == 1 {
                continue 'outer
            }
            count = count + 1
        }
    }
    return count
}
"#;

    let exit_code = test
        .compile_and_run("labeled_continue_outer.nr", source)
        .expect("Compilation or execution failed");
    // Each outer iteration counts once (j==0) then jumps back to the outer loop.
    assert_eq!(
        exit_code, 3,
        "labeled continue should re-enter the outer loop"
    );
}

#[test]
fn test_labeled_break_from_loop_through_while() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut total: i32 = 0
    'spin: loop {
        mut i: i32 = 0
        while i < 10 {
            total = total + 1
            if total >= 7 {
                break 'spin
            }
            i = i + 1
        }
    }
    return total
}
"#;

    let exit_code = test
        .compile_and_run("labeled_break_loop.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 7, "break 'spin should exit the labeled loop");
}

#[test]
fn test_unlabeled_break_unaffected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count: i32 = 0
    'outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            if j >= 1 {
                break
            }
        }
    }
    return count
}
"#;

    let exit_code = test
        .compile_and_run("unlabeled_break_inner.nr", source)
        .expect("Compilation or execution failed");
    // The unlabeled break leaves only the inner loop, so each of the 5 outer
    // iterations counts twice (j=0 then j=1 breaks).
    assert_eq!(
        exit_code, 10,
        "unlabeled break should only exit the inner loop"
    );
}

/// Labels and values live in separate namespaces: a binding named like the label is
/// still a value after `break`, and the quoted form still names the loop.
#[test]
fn test_label_and_value_of_the_same_name() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val outer: i32 = 9
    val r = 'outer: loop {
        'inner: while true {
            break 'outer outer
        }
    }
    'outer: for i in 0..3 {
        if i == 1 {
            break 'outer
        }
    }
    return r
}
"#;

    let exit_code = test
        .compile_and_run("label_value_same_name.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(
        exit_code, 9,
        "`break 'outer outer` should carry the binding"
    );
}

#[test]
fn test_unquoted_label_is_rejected_with_the_quoted_spelling() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    outer: for i in 0..3 { }
    return 0
}
"#;

    let error = test
        .check("unquoted_label.nr", source)
        .expect_err("a label without its quote must not compile");
    assert!(error.contains("`'outer:`"), "got: {error}");
}

#[test]
fn test_undefined_label_names_the_quoted_label() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    'outer: for i in 0..3 {
        continue 'other
    }
    return 0
}
"#;

    let error = test
        .check("undefined_label.nr", source)
        .expect_err("an unknown label must not compile");
    assert!(
        error.contains("use of undefined loop label `'other`"),
        "got: {error}"
    );
}

#[test]
fn test_label_on_a_non_loop_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    'outer: if true { }
    return 0
}
"#;

    let error = test
        .check("label_on_if.nr", source)
        .expect_err("a label names a loop and nothing else");
    assert!(error.contains("after a loop label"), "got: {error}");
}

/// A nested loop may reuse an enclosing label; `break 'a` then leaves the innermost
/// loop carrying it, and each loop keeps its own value type.
#[test]
fn test_reused_label_targets_the_innermost_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut n: i32 = 0
    val r = 'a: loop {
        val inner: i64 = 'a: loop {
            n = n + 1
            break 'a 40
        }
        break 'a n + inner as i32
    }
    return r
}
"#;

    let exit_code = test
        .compile_and_run("reused_label.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(
        exit_code, 41,
        "the inner `break 'a` should leave only the inner loop"
    );
}
