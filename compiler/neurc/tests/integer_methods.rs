// Integer primitive methods — wrapping_* / saturating_* / .shr(n) (Phase 1.5)
// End-to-end coverage: these dispatch through the builtin-method intrinsic path and lower
// to LLVM arithmetic / saturating intrinsics. Tests compile in the default (debug) profile,
// so the wrapping cases also prove they never trap on overflow.
mod common;
use common::CompileTest;

#[test]
fn wrapping_add_wraps_without_trapping() {
    let test = CompileTest::new();
    // 200 + 100 = 300, wraps modulo 256 -> 44. Debug build must NOT trap.
    let source = r#"
func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100
    return a.wrapping_add(b) as i32
}
"#;
    let exit = test
        .compile_and_run("wrapping_add.nr", source)
        .expect("wrapping_add compilation or execution failed");
    assert_eq!(exit, 44);
}

#[test]
fn saturating_add_clamps_to_max() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100
    return a.saturating_add(b) as i32
}
"#;
    let exit = test
        .compile_and_run("saturating_add.nr", source)
        .expect("saturating_add compilation or execution failed");
    assert_eq!(exit, 255);
}

#[test]
fn saturating_sub_unsigned_floors_at_zero() {
    let test = CompileTest::new();
    // 100 - 200 underflows; unsigned saturating subtract clamps to 0.
    let source = r#"
func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100
    return b.saturating_sub(a) as i32
}
"#;
    let exit = test
        .compile_and_run("saturating_sub.nr", source)
        .expect("saturating_sub compilation or execution failed");
    assert_eq!(exit, 0);
}

#[test]
fn shr_unsigned_is_logical() {
    let test = CompileTest::new();
    // 0b1111_0000 (240) >> 4 = 0b1111 (15), logical shift on an unsigned value.
    let source = r#"
func main() -> i32 {
    val mask: u8 = 240
    return mask.shr(4) as i32
}
"#;
    let exit = test
        .compile_and_run("shr_unsigned.nr", source)
        .expect("unsigned shr compilation or execution failed");
    assert_eq!(exit, 15);
}

#[test]
fn shr_signed_is_arithmetic() {
    let test = CompileTest::new();
    // -16 >> 2 = -4 (arithmetic, sign-preserving). +100 keeps the exit code positive.
    // A logical shift would yield a large positive number, not 96.
    let source = r#"
func main() -> i32 {
    val n: i32 = -16
    return n.shr(2) + 100
}
"#;
    let exit = test
        .compile_and_run("shr_signed.nr", source)
        .expect("signed shr compilation or execution failed");
    assert_eq!(exit, 96);
}

#[test]
fn saturating_mul_signed_clamps_to_max() {
    let test = CompileTest::new();
    // 2_000_000_000 * 2 overflows i32; signed saturating multiply clamps to i32::MAX.
    let source = r#"
func main() -> i32 {
    val x: i32 = 2000000000
    val y: i32 = 2
    return x.saturating_mul(y) - 2147483647
}
"#;
    let exit = test
        .compile_and_run("saturating_mul_max.nr", source)
        .expect("saturating_mul (max) compilation or execution failed");
    assert_eq!(exit, 0);
}

#[test]
fn saturating_mul_signed_clamps_to_min() {
    let test = CompileTest::new();
    // -2_000_000_000 * 2 overflows negatively; clamps to i32::MIN (a negative value).
    let source = r#"
func main() -> i32 {
    val x: i32 = -2000000000
    val y: i32 = 2
    val r: i32 = x.saturating_mul(y)
    if r < 0 {
        return 0
    }
    return 1
}
"#;
    let exit = test
        .compile_and_run("saturating_mul_min.nr", source)
        .expect("saturating_mul (min) compilation or execution failed");
    assert_eq!(exit, 0);
}

#[test]
fn wrapping_method_on_string_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val s: string = "hi"
    val n: u64 = s.wrapping_add(s)
    return 0
}
"#;
    let source_path = test.write_source("wrapping_on_string.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "expected compilation to fail for wrapping_add on a string receiver"
    );
}

#[test]
fn checked_add_reports_overflow_as_none() {
    let test = CompileTest::new();
    // 200 + 100 = 300 exceeds u8::MAX, so the result is `Option::None`.
    let source = r#"
func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100
    return match a.checked_add(b) {
        Option::Some(v) => v as i32,
        Option::None => 7
    }
}
"#;
    let exit = test
        .compile_and_run("checked_add_overflow.nr", source)
        .expect("checked_add (overflow) compilation or execution failed");
    assert_eq!(exit, 7);
}

#[test]
fn checked_add_carries_the_sum_when_in_range() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val a: u8 = 100
    val b: u8 = 55
    return match a.checked_add(b) {
        Option::Some(v) => v as i32,
        Option::None => 0
    }
}
"#;
    let exit = test
        .compile_and_run("checked_add_in_range.nr", source)
        .expect("checked_add (in range) compilation or execution failed");
    assert_eq!(exit, 155);
}

#[test]
fn checked_sub_unsigned_underflow_is_none() {
    let test = CompileTest::new();
    // 100 - 200 underflows an unsigned type, which the unsigned intrinsic reports.
    let source = r#"
func main() -> i32 {
    val a: u8 = 200
    val b: u8 = 100
    return match b.checked_sub(a) {
        Option::Some(v) => v as i32,
        Option::None => 9
    }
}
"#;
    let exit = test
        .compile_and_run("checked_sub_underflow.nr", source)
        .expect("checked_sub (underflow) compilation or execution failed");
    assert_eq!(exit, 9);
}

#[test]
fn checked_mul_signed_overflow_is_none() {
    let test = CompileTest::new();
    // 100_000 * 100_000 = 10^10, far beyond i32; the signed intrinsic reports it.
    let source = r#"
func main() -> i32 {
    val x: i32 = 100000
    return match x.checked_mul(x) {
        Option::Some(v) => v,
        Option::None => 3
    }
}
"#;
    let exit = test
        .compile_and_run("checked_mul_overflow.nr", source)
        .expect("checked_mul (overflow) compilation or execution failed");
    assert_eq!(exit, 3);
}

#[test]
fn checked_intrinsic_result_is_a_typed_option_instance() {
    let test = CompileTest::new();
    // The result annotates as `Option<i64>` because the receiver is `i64`; a
    // function taking that instance accepts it directly.
    let source = r#"
func unwrap_or(o: Option<i64>, fallback: i64) -> i64 {
    match o {
        Option::Some(v) => v,
        Option::None => fallback
    }
}

func main() -> i32 {
    val big: i64 = 9000000000000000000i64
    val ok: Option<i64> = big.checked_sub(8999999999999999958i64)
    val bad: Option<i64> = big.checked_mul(2i64)
    return (unwrap_or(ok, 0i64) + unwrap_or(bad, 1i64)) as i32
}
"#;
    let exit = test
        .compile_and_run("checked_option_instance.nr", source)
        .expect("checked option instance compilation or execution failed");
    assert_eq!(exit, 43);
}

#[test]
fn checked_method_on_float_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val f: f64 = 1.5
    val r = f.checked_add(2.0)
    return 0
}
"#;
    let source_path = test.write_source("checked_on_float.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "expected compilation to fail for checked_add on a float receiver"
    );
}
