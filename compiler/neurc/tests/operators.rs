// Arithmetic operator tests: basic operations, division, modulo, and complex expressions
mod common;
use common::CompileTest;

#[test]
fn test_arithmetic_operations() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 10
    val b: i32 = 5
    val sum: i32 = a + b
    val diff: i32 = a - b
    val product: i32 = a * b
    return sum + diff + product
}
"#;

    let exit_code = test
        .compile_and_run("arithmetic.nr", source)
        .expect("Compilation or execution failed");
    // sum=15, diff=5, product=50, total=70
    assert_eq!(exit_code, 70, "Expected exit code 70");
}

#[test]
fn test_division_operator() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 20
    val b: i32 = 4
    val result: i32 = a / b
    return result
}
"#;

    let exit_code = test
        .compile_and_run("division.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 5, "Expected exit code 5 (20 / 4)");
}

#[test]
fn test_modulo_operator() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 17
    val b: i32 = 5
    val result: i32 = a % b
    return result
}
"#;

    let exit_code = test
        .compile_and_run("modulo.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 2, "Expected exit code 2 (17 % 5)");
}

#[test]
fn test_complex_expression() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 2
    val b: i32 = 3
    val c: i32 = 4
    val result: i32 = a * b + c * 5 - 10
    return result
}
"#;

    let exit_code = test
        .compile_and_run("complex_expr.nr", source)
        .expect("Compilation or execution failed");
    // 2*3 + 4*5 - 10 = 6 + 20 - 10 = 16
    assert_eq!(exit_code, 16, "Expected exit code 16");
}

#[test]
fn test_nested_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 100
    val b: i32 = 10
    val c: i32 = 3
    val result: i32 = (a / b) % c
    return result
}
"#;

    let exit_code = test
        .compile_and_run("nested_arithmetic.nr", source)
        .expect("Compilation or execution failed");
    // (100 / 10) % 3 = 10 % 3 = 1
    assert_eq!(exit_code, 1, "Expected exit code 1");
}

// Note: Float operations are supported but not tested here
// because exit codes are integers. Float support is verified
// by the compiler's type system and codegen tests.

#[test]
fn test_type_casts() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: i32 = 42
    val f: f64 = a as f64
    val b: f64 = 3.14
    val c: i32 = b as i32
    
    val small: i8 = -10
    val big: i32 = small as i32
    
    val flag: bool = true
    val flag_num: i32 = flag as i32
    
    return c + big + flag_num // 3 + -10 + 1 = -6, wrapping to 250 in u8 exit code
}
"#;

    let exit_code = test
        .compile_and_run("type_casts.nr", source)
        .expect("Compilation or execution failed");

    // 3 + (-10) + 1 = -6, returns 250 as i32 is returned as exit code
    assert_eq!(exit_code as i8, -6, "Expected exit code -6");
}

// Logical short-circuit: the RHS of `&&`/`||` must not be evaluated when
// the LHS already decides the result. `boom()` divides by zero, so if it runs the
// process is killed by SIGFPE and no clean exit code is produced.

#[test]
fn and_short_circuits_rhs_not_evaluated() {
    let test = CompileTest::new();
    let source = r#"
func boom() -> bool {
    val a: i32 = 1
    val z: i32 = 0
    return (a / z) == 0
}
func main() -> i32 {
    if false && boom() { return 2 }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("and_short_circuit.nr", source)
        .expect("Compilation or execution failed");
    // false && boom() => boom() never runs => no SIGFPE => clean exit 0.
    assert_eq!(exit_code, 0, "&& should short-circuit and not run boom()");
}

#[test]
fn or_short_circuits_rhs_not_evaluated() {
    let test = CompileTest::new();
    let source = r#"
func boom() -> bool {
    val a: i32 = 1
    val z: i32 = 0
    return (a / z) == 0
}
func main() -> i32 {
    if true || boom() { return 0 }
    return 2
}
"#;
    let exit_code = test
        .compile_and_run("or_short_circuit.nr", source)
        .expect("Compilation or execution failed");
    // true || boom() => boom() never runs => no SIGFPE => clean exit 0.
    assert_eq!(exit_code, 0, "|| should short-circuit and not run boom()");
}

#[test]
fn logical_and_truth_table() {
    let test = CompileTest::new();
    // true && true => take branch (1); true && false => skip (0).
    let true_true = r#"
func main() -> i32 {
    if true && true { return 1 }
    return 0
}
"#;
    let true_false = r#"
func main() -> i32 {
    if true && false { return 1 }
    return 0
}
"#;
    assert_eq!(
        test.compile_and_run("and_tt.nr", true_true)
            .expect("compile/run failed"),
        1,
        "true && true should be true"
    );
    assert_eq!(
        test.compile_and_run("and_tf.nr", true_false)
            .expect("compile/run failed"),
        0,
        "true && false should be false"
    );
}

#[test]
fn logical_or_truth_table() {
    let test = CompileTest::new();
    // false || true => take branch (1); false || false => skip (0).
    let false_true = r#"
func main() -> i32 {
    if false || true { return 1 }
    return 0
}
"#;
    let false_false = r#"
func main() -> i32 {
    if false || false { return 1 }
    return 0
}
"#;
    assert_eq!(
        test.compile_and_run("or_ft.nr", false_true)
            .expect("compile/run failed"),
        1,
        "false || true should be true"
    );
    assert_eq!(
        test.compile_and_run("or_ff.nr", false_false)
            .expect("compile/run failed"),
        0,
        "false || false should be false"
    );
}
