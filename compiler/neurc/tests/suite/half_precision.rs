// End-to-end tests for the `f16` / `bf16` half-precision primitives.
//
// Half precision is ordinary floating point: binding, copy, `as` casts, arithmetic, the six
// comparisons, the math methods and `.to_checked`. Each operation computes in `f32` and
// rounds once to the operand type, so a chain rounds after every step.

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
    // Computing in `f32` by hand still works, and is how `f32` accumulation is spelled.
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

/// `1 + 2^-8` is a tie in `bf16` and rounds to `1`, and so does the second addition. Carried
/// at `f32` precision the two additions would make `1 + 2^-7`, which `bf16` holds. `f16` is
/// the same at `2^-11`. Both builds must round after every operation.
#[test]
fn half_arithmetic_rounds_after_every_operation() {
    let test = CompileTest::new();
    let source = test.write_source(
        "half_rounding.nr",
        r#"
func main() -> i32 {
    val one: bf16 = 1.0
    val tiny: bf16 = 0.00390625
    val chain = one + tiny + tiny
    if chain as f32 != 1.0 { return 1 }
    mut acc = 1.0f16
    acc += 0.00048828125
    acc += 0.00048828125
    if acc as f32 != 1.0 { return 2 }
    val third = 1.0f16 / 3.0
    if third as f32 != 0.333251953125 { return 3 }
    if (7.5bf16 % 2.0) as f32 != 1.5 { return 4 }
    if (-third) as f32 != -0.333251953125 { return 5 }
    val product = 3.0f16 * 0.1 - 0.5 * 2.0f16
    if product as f32 != -0.7001953125 { return 6 }
    0
}
"#,
    );
    for level in ["0", "2"] {
        let exe = source.with_extension(format!("o{level}"));
        let built = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
            .args(["compile", "-O", level, "-o"])
            .arg(&exe)
            .arg(&source)
            .output()
            .expect("neurc runs");
        assert!(built.status.success(), "-O{level}: {built:?}");
        let status = test.run_executable(&exe).expect("the program runs");
        assert_eq!(status, 0, "-O{level}");
    }
}

/// The six comparisons are `f32`'s: every one involving NaN is false, `!=` included.
#[test]
fn half_comparisons_follow_ieee() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: f16 = 1.5
    val b: f16 = 2.0
    if !(a < b && b > a && a <= a && a >= a && a != b) { return 1 }
    val zero: bf16 = 0.0
    val nan = zero / zero
    if nan == nan || nan != nan || nan < 1.0 || nan >= 1.0 { return 2 }
    if !nan.is_nan() || a.is_nan() { return 3 }
    0
}
"#;
    let exit = test
        .compile_and_run("half_compare.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// The math methods round once from `f32`, and `.to_checked` answers `None` exactly where
/// `as` would saturate, through a borrow as well.
#[test]
fn half_math_methods_and_checked_conversion() {
    let test = CompileTest::new();
    let source = r#"
func fits(h: &f16) -> bool {
    match h.to_checked::<u16>() {
        Option::Some(_) => true,
        Option::None => false
    }
}

func main() -> i32 {
    if 2.0bf16.sqrt() as f32 != 1.4140625 { return 1 }
    if 1.0f16.exp() as f32 != 2.71875 { return 2 }
    if (-2.5bf16).abs() as f32 != 2.5 { return 3 }
    if 3.0f16.pow(2.0) as f32 != 9.0 { return 4 }
    if (300.5f16.to_checked::<u8>() ?? 7) != 7 { return 5 }
    if (200.5bf16.to_checked::<u8>() ?? 7) != 200 { return 6 }
    val largest = 65504.0f16
    if !fits(&largest) { return 7 }
    val zero: f16 = 0.0
    val infinite = 1.0f16 / zero
    if fits(&infinite) { return 8 }
    0
}
"#;
    let exit = test
        .compile_and_run("half_methods.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

/// There is no implicit widening: a half operand meets only its own type, and an integer
/// literal does not become a float.
#[test]
fn half_operands_do_not_mix() {
    let test = CompileTest::new();
    for (name, expr) in [
        ("half_f32", "a + 1.0f32"),
        ("half_bf16", "a * 1.0bf16"),
        ("half_int", "a + 1"),
    ] {
        let source = format!(
            "func main() -> i32 {{\n    val a: f16 = 1.0\n    val b = {expr}\n    return 0\n}}\n"
        );
        let err = test
            .compile_and_run(&format!("{name}.nr"), &source)
            .expect_err("mixed operands must be a compile error");
        assert!(err.contains("type mismatch"), "{expr}: {err}");
    }
}

/// Sorts order half keys and a traversal may answer a half type.
#[test]
fn half_tensors_sort_and_traverse() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val t: Tensor<bf16, [4]> = [3.0, -1.0, 2.5, 0.5]
    val sorted = t.sort()
    if sorted[0] as f32 != -1.0 || sorted[3] as f32 != 3.0 { return 1 }
    val order = t.argsort()
    if order[0] != 1 || order[3] != 0 { return 2 }
    val (top, at) = t.topk(k: 2)
    if top[1] as f32 != 2.5 || at[1] != 2 { return 3 }
    val doubled = t.map(|x: bf16| x * 2.0)
    if doubled[1] as f32 != -2.0 { return 4 }
    val h: Tensor<f16, [3]> = [1.0, 2.0, 3.0]
    val g: Tensor<f16, [3]> = [0.5, 0.25, 0.125]
    val fused = h.zip(&g, |a: f16, b: f16| a * b + 1.0)
    if fused[2] as f32 != 1.375 { return 5 }
    val total = h.reduce(0.0f16, |acc: f16, x: f16| acc + x)
    if total as f32 != 6.0 { return 6 }
    0
}
"#;
    let exit = test
        .compile_and_run("half_sort_traverse.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
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
