// `loop` as a value expression: `val x = loop { ... break v }`.
mod common;
use common::CompileTest;

#[test]
fn test_loop_expression_yields_break_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    val first_even = loop {
        i = i + 1
        if i % 2 == 0 {
            break i
        }
    }
    return first_even
}
"#;

    let exit_code = test
        .compile_and_run("loop_value_basic.nr", source)
        .expect("Compilation or execution failed");
    // The first even candidate is 2.
    assert_eq!(exit_code, 2, "loop expression should evaluate to `break i`");
}

#[test]
fn test_loop_value_used_in_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut j: i32 = 0
    val found = loop {
        j = j + 1
        if j >= 5 {
            break j * 10
        }
    }
    return found - 48
}
"#;

    let exit_code = test
        .compile_and_run("loop_value_arith.nr", source)
        .expect("Compilation or execution failed");
    // found == 50, so 50 - 48 == 2.
    assert_eq!(exit_code, 2);
}

#[test]
fn test_labeled_loop_expression_yields_outer_break_value() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut acc: i32 = 0
    val r = outer: loop {
        mut k: i32 = 0
        loop {
            k = k + 1
            if k > 3 {
                break
            }
            acc = acc + k
        }
        if acc > 5 {
            break outer acc
        }
    }
    return r
}
"#;

    let exit_code = test
        .compile_and_run("loop_value_labeled.nr", source)
        .expect("Compilation or execution failed");
    // Inner loop adds 1+2+3 = 6; `break outer acc` carries 6 out of the labeled loop.
    assert_eq!(
        exit_code, 6,
        "`break outer acc` should yield the outer loop value"
    );
}

// A tail `loop` is the function's implicit return. It used to reach codegen as a
// discard-value statement, so its exit block got `unreachable` — no instruction at
// -O0 — and the `break` target collapsed into a jump to itself.
#[test]
fn test_tail_loop_is_the_implicit_return() {
    let test = CompileTest::new();
    let source = r#"
func count_to(n: i32) -> i32 {
    mut i: i32 = 0
    loop {
        if i == n {
            break i
        }
        i += 1
    }
}

func main() -> i32 {
    return count_to(7)
}
"#;

    let exit_code = test
        .compile_and_run("loop_tail_return.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 7, "a tail `loop` must return its `break` value");
}

// A `loop` no `break` targets never reaches its exit block, so it satisfies any
// declared return type — the same divergent contract `panic` carries.
#[test]
fn test_tail_loop_without_break_leaves_via_return() {
    let test = CompileTest::new();
    let source = r#"
func first_multiple_of_three(from: i32) -> i32 {
    mut i: i32 = from
    loop {
        if i % 3 == 0 {
            return i
        }
        i += 1
    }
}

func main() -> i32 {
    return first_multiple_of_three(7)
}
"#;

    let exit_code = test
        .compile_and_run("loop_tail_diverges.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 9);
}

#[test]
fn test_break_value_type_mismatch_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut i: i32 = 0
    val x = loop {
        i = i + 1
        if i == 1 { break 5 }
        if i == 2 { break "hi" }
    }
    return 0
}
"#;

    let source_path = test.write_source("loop_value_mismatch.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "value-breaks with disagreeing types must be a compile error"
    );
}
