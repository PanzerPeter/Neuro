// Labeled break / continue tests (§3.7): `outer: for ... { break outer }`.
mod common;
use common::CompileTest;

#[test]
fn test_labeled_break_exits_outer_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count: i32 = 0
    outer: for i in 0..5 {
        for j in 0..5 {
            count = count + 1
            if i + j >= 3 {
                break outer
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
    outer: for i in 0..3 {
        for j in 0..3 {
            if j == 1 {
                continue outer
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
    spin: loop {
        mut i: i32 = 0
        while i < 10 {
            total = total + 1
            if total >= 7 {
                break spin
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
    assert_eq!(exit_code, 7, "break spin should exit the labeled loop");
}

#[test]
fn test_unlabeled_break_unaffected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut count: i32 = 0
    outer: for i in 0..5 {
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
