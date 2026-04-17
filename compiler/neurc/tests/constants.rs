// Constant declaration tests (§1.3)
// Covers AC1–AC5: module-level consts, function-body consts, forward references,
// arithmetic folding, and rejection of non-const expressions.
mod common;
use common::CompileTest;

// ── AC1: Module-level const is visible in function body ──────────────────────

#[test]
fn module_const_integer_visible_in_function() {
    let test = CompileTest::new();
    let source = r#"
const ANSWER: i32 = 42

func main() -> i32 {
    return ANSWER
}
"#;
    let exit_code = test
        .compile_and_run("module_const_integer.nr", source)
        .expect("module-level const should compile and run");
    assert_eq!(exit_code, 42, "ANSWER should be 42");
}

// ── AC1: Multiple module-level consts ────────────────────────────────────────

#[test]
fn multiple_module_consts() {
    let test = CompileTest::new();
    let source = r#"
const A: i32 = 10
const B: i32 = 20

func main() -> i32 {
    return A + B
}
"#;
    let exit_code = test
        .compile_and_run("multiple_module_consts.nr", source)
        .expect("multiple module consts should compile and run");
    assert_eq!(exit_code, 30, "A + B should be 30");
}

// ── AC2: Function-body const ──────────────────────────────────────────────────

#[test]
fn function_body_const() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    const LOCAL: i32 = 7
    return LOCAL
}
"#;
    let exit_code = test
        .compile_and_run("function_body_const.nr", source)
        .expect("function-body const should compile and run");
    assert_eq!(exit_code, 7, "LOCAL should be 7");
}

// ── AC3: Const-expr arithmetic (module level) ────────────────────────────────

#[test]
fn module_const_arithmetic_expression() {
    let test = CompileTest::new();
    let source = r#"
const BASE: i32 = 10
const DOUBLED: i32 = BASE * 2

func main() -> i32 {
    return DOUBLED
}
"#;
    let exit_code = test
        .compile_and_run("module_const_arithmetic.nr", source)
        .expect("const arithmetic should compile and run");
    assert_eq!(exit_code, 20, "DOUBLED should be 20");
}

// ── AC3: Const-expr arithmetic (function body) ───────────────────────────────

#[test]
fn function_const_arithmetic_expression() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    const HALF: i32 = 50
    const FULL: i32 = HALF * 2
    return FULL
}
"#;
    let exit_code = test
        .compile_and_run("func_const_arithmetic.nr", source)
        .expect("function const arithmetic should compile and run");
    assert_eq!(exit_code, 100, "FULL should be 100");
}

// ── AC4: Forward reference — function uses const defined after it ─────────────

#[test]
fn module_const_forward_reference() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    return FORWARD
}

const FORWARD: i32 = 77
"#;
    let exit_code = test
        .compile_and_run("forward_ref_const.nr", source)
        .expect("forward reference to const should compile and run");
    assert_eq!(exit_code, 77, "FORWARD should be 77");
}

// ── AC5: Non-const RHS is rejected ───────────────────────────────────────────

#[test]
fn const_with_non_const_rhs_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func get_value() -> i32 {
    return 5
}

func main() -> i32 {
    const BAD: i32 = get_value()
    return BAD
}
"#;
    let source_path = test.write_source("const_non_const_rhs.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "const with function-call RHS should be rejected"
    );
}

// ── AC5: Duplicate const is rejected ─────────────────────────────────────────

#[test]
fn duplicate_module_const_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
const X: i32 = 1
const X: i32 = 2

func main() -> i32 {
    return X
}
"#;
    let source_path = test.write_source("duplicate_module_const.nr", source);
    let result = test.compile(&source_path);
    assert!(result.is_err(), "duplicate module const should be rejected");
}

// ── AC5: Duplicate function-body const is rejected ────────────────────────────

#[test]
fn duplicate_function_const_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    const Y: i32 = 1
    const Y: i32 = 2
    return Y
}
"#;
    let source_path = test.write_source("duplicate_func_const.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "duplicate function-body const should be rejected"
    );
}
