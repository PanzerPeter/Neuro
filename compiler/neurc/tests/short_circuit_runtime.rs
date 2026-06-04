// End-to-end tests for short-circuiting `&&` / `||` with comparison operands (§1.4).
//
// Regression coverage for a span-keyed type-map collision: a binary expression and its
// leftmost descendant share the same `span.start`, so the parent's left-operand-type slot
// (`span.start + 1`) clobbered the child comparison's slot. The leftmost comparison of an
// `&&` / `||` was then codegen'd with `left_ty = Bool`, truncating its i32 operands to i1.
// e.g. `c >= 48 && c <= 57` with `c = 51` wrongly evaluated to `false`.
use std::path::PathBuf;
use std::process::{Command, Output};

fn neurc_path() -> PathBuf {
    std::env::current_exe()
        .expect("get current exe")
        .parent()
        .expect("parent")
        .parent()
        .expect("grandparent")
        .join("neurc")
}

fn compile_source(source: &str, tag: &str) -> PathBuf {
    let dir = std::env::temp_dir();
    let src = dir.join(format!("neuro_sc_{tag}.nr"));
    let exe = dir.join(format!("neuro_sc_{tag}"));
    std::fs::write(&src, source).expect("write source");

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-O")
        .arg("0")
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("run neurc");

    assert!(
        output.status.success(),
        "compile failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    exe
}

fn run(exe: &PathBuf) -> Output {
    Command::new(exe).output().expect("run executable")
}

/// Build a `main` that returns 1 when `cond` holds (with `c = 51`), else 0.
fn cond_prog(cond: &str) -> String {
    format!(
        "func main() -> i32 {{\n    val c: i32 = 51\n    if {cond} {{\n        return 1\n    }}\n    return 0\n}}\n"
    )
}

fn assert_cond(cond: &str, expected: i32, tag: &str) {
    let exe = compile_source(&cond_prog(cond), tag);
    let code = run(&exe).status.code();
    assert_eq!(
        code,
        Some(expected),
        "`{cond}` (c = 51) expected to evaluate to {expected}, got exit {code:?}"
    );
}

#[test]
fn and_with_comparison_lhs_evaluates_correctly() {
    // The leftmost comparison is the one that previously regressed.
    assert_cond("c >= 48 && c <= 57", 1, "and_ge_le");
    assert_cond("c > 48 && c < 57", 1, "and_gt_lt");
    assert_cond("c != 1 && c != 2", 1, "and_ne_ne");
    assert_cond("c >= 48 && true", 1, "and_ge_true");
}

#[test]
fn or_with_comparison_lhs_evaluates_correctly() {
    assert_cond("c < 10 || c >= 48", 1, "or_lt_ge");
    assert_cond("c > 100 || c == 51", 1, "or_gt_eq");
}

#[test]
fn comparison_lhs_false_branch_still_short_circuits() {
    // c = 51: first comparison genuinely false -> whole `&&` is false.
    assert_cond("c >= 60 && c <= 70", 0, "and_false_lhs");
    // First operand of `||` false, second true -> true.
    assert_cond("c >= 60 || c <= 57", 1, "or_false_then_true");
}

#[test]
fn nested_and_chains_evaluate_correctly() {
    assert_cond("c >= 48 && c <= 57 && c != 0", 1, "and_chain");
    assert_cond("c > 0 && c < 100 && c == 51", 1, "and_chain_eq");
}
