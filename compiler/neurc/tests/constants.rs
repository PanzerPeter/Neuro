// Constant declaration tests
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

// ── AC4: Forward reference, function uses const defined after it ─────────────

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

// ── Bool-typed const folding ──────────────────────────────────
// The const folder must handle binary expressions whose operands fold to bools.

#[test]
fn module_const_bool_and() {
    let test = CompileTest::new();
    let source = r#"
const FLAG: bool = true && false
func main() -> i32 {
    if FLAG { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("module_const_bool_and.nr", source)
        .expect("bool const folding should compile and run");
    assert_eq!(exit_code, 0, "true && false should fold to false");
}

#[test]
fn module_const_bool_from_comparisons() {
    let test = CompileTest::new();
    let source = r#"
const OK: bool = (1 < 2) && (3 < 4)
func main() -> i32 {
    if OK { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("module_const_bool_cmp.nr", source)
        .expect("bool const folding over comparisons should compile and run");
    assert_eq!(exit_code, 1, "(1 < 2) && (3 < 4) should fold to true");
}

#[test]
fn module_const_bool_equality() {
    let test = CompileTest::new();
    let source = r#"
const E: bool = true == true
func main() -> i32 {
    if E { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("module_const_bool_eq.nr", source)
        .expect("bool == const folding should compile and run");
    assert_eq!(exit_code, 1, "true == true should fold to true");
}

#[test]
fn function_const_bool_and() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    const G: bool = true && true
    if G { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("function_const_bool_and.nr", source)
        .expect("function-body bool const folding should compile and run");
    assert_eq!(exit_code, 1, "true && true should fold to true");
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

// ── BUG-022: an overflowing const initializer is rejected, not wrapped ────────
//
// A `const` is evaluated by the compiler, so the debug tier's overflow panic has
// nowhere to fire. Folding it to the wrapped value made the same quantity disagree
// between a `const` and a function body on the tier whose purpose is to rule that
// disagreement out, so the folder rejects the initializer instead. Every arithmetic
// operator moves together: fixing one alone would recreate the asymmetry BUG-021 closed.

/// Compile `source` and return the error text, asserting that compilation failed.
fn expect_compile_error(filename: &str, source: &str) -> String {
    let test = CompileTest::new();
    let source_path = test.write_source(filename, source);
    match test.compile(&source_path) {
        Ok(_) => panic!("{filename} should have been rejected but compiled"),
        Err(message) => message,
    }
}

#[test]
fn const_addition_overflow_is_rejected() {
    let error = expect_compile_error(
        "const_add_overflow.nr",
        r#"
const C: u8 = 200u8 + 100u8

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows u8") && error.contains('+'),
        "error should name the operator and the type: {error}"
    );
}

#[test]
fn const_subtraction_overflow_is_rejected() {
    let error = expect_compile_error(
        "const_sub_overflow.nr",
        r#"
const C: u8 = 10u8 - 20u8

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows u8"),
        "error should name the type: {error}"
    );
}

#[test]
fn const_multiplication_overflow_is_rejected() {
    let error = expect_compile_error(
        "const_mul_overflow.nr",
        r#"
const C: i16 = 300i16 * 300i16

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows i16"),
        "error should name the type: {error}"
    );
}

#[test]
fn const_negation_overflow_is_rejected() {
    let error = expect_compile_error(
        "const_neg_overflow.nr",
        r#"
const C: i8 = -(-128i8)

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows i8"),
        "error should name the type: {error}"
    );
}

#[test]
fn const_division_min_over_minus_one_is_rejected() {
    let error = expect_compile_error(
        "const_div_overflow.nr",
        r#"
const C: i8 = -128i8 / -1i8

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows i8"),
        "error should name the type: {error}"
    );
}

#[test]
fn const_remainder_min_over_minus_one_is_rejected() {
    // `MIN % -1` is mathematically 0, so the range check alone never sees it. The debug
    // tier still panics on it, so the folder rejects it against the operand type's minimum.
    let error = expect_compile_error(
        "const_rem_overflow.nr",
        r#"
const C: i8 = -128i8 % -1i8

func main() -> i32 {
    return C as i32
}
"#,
    );
    assert!(
        error.contains("overflows i8"),
        "error should name the type: {error}"
    );
}

#[test]
fn const_ordinary_division_and_remainder_still_fold() {
    let test = CompileTest::new();
    let source = r#"
const Q: i32 = 100 / 7
const R: i32 = 100 % 7

func main() -> i32 {
    return Q * 10 + R
}
"#;
    let exit_code = test
        .compile_and_run("const_div_rem.nr", source)
        .expect("ordinary const division should still fold");
    assert_eq!(exit_code, 142, "100 / 7 is 14 and 100 % 7 is 2");
}

#[test]
fn const_arithmetic_inside_range_still_folds() {
    // The check rejects only what does not fit. Values at the edge of the type
    // must keep folding, or the fix would be a regression dressed as a diagnostic.
    let test = CompileTest::new();
    let source = r#"
const A: u8 = 200u8 + 55u8
const B: i8 = -127i8 - 1i8

func main() -> i32 {
    if A == 255u8 && B == -128i8 { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("const_edge_of_range.nr", source)
        .expect("in-range const arithmetic should still fold");
    assert_eq!(exit_code, 1, "255u8 and -128i8 are representable");
}

#[test]
fn const_cast_still_truncates() {
    // An `as` cast is an explicit narrowing at run time and stays one in the folder.
    // Only implicit arithmetic overflow became an error.
    let test = CompileTest::new();
    let source = r#"
const C: u8 = 300 as u8

func main() -> i32 {
    return C as i32
}
"#;
    let exit_code = test
        .compile_and_run("const_cast_truncates.nr", source)
        .expect("an explicit cast should still narrow");
    assert_eq!(exit_code, 44, "300 as u8 is 44");
}

#[test]
fn const_bitwise_operators_wrap_to_their_type() {
    // Bitwise operators and `<<` have no overflow rule at run time, so they must not
    // acquire one here. `~0u8` is 255, and subtracting from it stays in range.
    let test = CompileTest::new();
    let source = r#"
const M: u8 = ~0u8
const K: u8 = ~0u8 - 100u8
const S: u8 = 1u8 << 3

func main() -> i32 {
    if M == 255u8 && K == 155u8 && S == 8u8 { return 1 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("const_bitwise.nr", source)
        .expect("bitwise const folding should be unaffected");
    assert_eq!(
        exit_code, 1,
        "~0u8 is 255, ~0u8 - 100u8 is 155, 1u8 << 3 is 8"
    );
}
