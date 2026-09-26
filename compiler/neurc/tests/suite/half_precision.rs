// End-to-end tests for the `f16` / `bf16` half-precision primitives.
//
// The scalar contract is deliberately narrow: binding, copy, `==`/`!=`, and
// `as`-cast to/from any numeric type, but no arithmetic (compute in `f32`).

use crate::compile_harness::CompileTest;

#[test]
fn f16_cast_round_trip_through_int() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val h: f16 = 7.0f16
    return h as i32
}
"#;
    let exit_code = test
        .compile_and_run("f16_cast.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 7);
}

#[test]
fn bf16_cast_round_trip_through_int() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val h: bf16 = 42.0bf16
    return h as i32
}
"#;
    let exit_code = test
        .compile_and_run("bf16_cast.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 42);
}

#[test]
fn half_precision_equality() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: f16 = 1.5f16
    val b: f16 = 1.5f16
    val c: f16 = 2.0f16
    if a == b && a != c {
        return 9
    }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("half_eq.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 9);
}

#[test]
fn half_precision_is_copy() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val h: f16 = 5.0f16
    val copy = h
    if copy == h {
        return h as i32
    }
    return 0
}
"#;
    let exit_code = test
        .compile_and_run("half_copy.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 5);
}

#[test]
fn compute_in_f32_then_narrow_to_half() {
    // The spec's prescribed workaround: widen to f32, do the math, narrow back.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: bf16 = 10.0bf16
    val b: bf16 = 4.0bf16
    val sum: bf16 = (a as f32 + b as f32) as bf16
    return sum as i32
}
"#;
    let exit_code = test
        .compile_and_run("half_compute.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 14);
}

#[test]
fn half_as_function_param_and_return() {
    let test = CompileTest::new();
    let source = r#"
func scale(x: f16) -> f16 {
    return (x as f32 * 3.0f32) as f16
}

func main() -> i32 {
    val r: f16 = scale(2.0f16)
    return r as i32
}
"#;
    let exit_code = test
        .compile_and_run("half_param.nr", source)
        .expect("Compilation or execution failed");
    assert_eq!(exit_code, 6);
}

#[test]
fn half_precision_arithmetic_is_rejected() {
    // Half-precision scalars have no arithmetic operators.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: f16 = 1.0f16
    val b: f16 = 2.0f16
    val c: f16 = a + b
    return 0
}
"#;
    let result = test.compile_and_run("half_no_arith.nr", source);
    assert!(
        result.is_err(),
        "half-precision arithmetic must be a compile error, got {result:?}"
    );
}

/// Half-precision tensors compute: elementwise operators, a half scalar broadcast, `@`,
/// compound assignment and reductions, each element widened to `f32` for the operation.
/// A reduction also accumulates in `f32`, so a `bf16` sum of a thousand ones is 1000 and
/// not the 256 where a 16-bit running total stops growing.
#[test]
fn test_bug_073_half_precision_tensors_compute() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: Tensor<bf16, [2]> = [1.0bf16, 2.0bf16]
    val s = (&a * &a).sum()
    val m: Tensor<f16, [2, 2]> = [[1.0f16, 2.0f16], [3.0f16, 4.0f16]]
    val p = &m @ &m
    mut w: Tensor<bf16, [2]> = [4.0bf16, 8.0bf16]
    w += &a
    w -= &a * 2.0bf16
    val ones: Tensor<bf16, [1000]> = Tensor::<bf16, [1000]>::ones()
    val many = ones.sum()
    val avg = m.mean()
    if s as f32 != 5.0 { return 1 }
    if p[1, 1] as f32 != 22.0 { return 2 }
    if w[0] as f32 != 3.0 || w[1] as f32 != 6.0 { return 3 }
    if many as f32 != 1000.0 { return 4 }
    if avg as f32 != 2.5 { return 5 }
    0
}
"#;
    let exit = test
        .compile_and_run("half_tensor_compute.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}
