// End-to-end tests for the extreme values of each integer type.
//
// A literal is never negative in source — `-1` is a negation over `1` — so the most
// negative value of a signed type is written with a magnitude one past that type's
// maximum. Range-checking the magnitude alone rejected every such value, and carrying
// the magnitude as an `i64` in the lexer put `i64::MIN` and the upper half of `u64` out
// of reach before a type was even known. Both ends must now round-trip.
mod common;
use common::CompileTest;

#[test]
fn regression_most_negative_signed_literals_are_accepted() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "signed_min.nr",
            r#"
func main() -> i32 {
    val a: i8 = -128
    val b: i16 = -32768
    val c: i32 = -2147483648
    val d: i64 = -9223372036854775808i64
    mut count: i32 = 0
    if a + 1i8 == -127i8 {
        count = count + 1
    }
    if b + 1i16 == -32767i16 {
        count = count + 1
    }
    if c + 1 == -2147483647 {
        count = count + 1
    }
    if d + 1i64 == -9223372036854775807i64 {
        count = count + 1
    }
    return count
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_unsigned_maximum_literals_are_accepted() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "unsigned_max.nr",
            r#"
func main() -> i32 {
    val a: u8 = 255
    val b: u16 = 65535
    val c: u32 = 4294967295
    val d: u64 = 18446744073709551615u64
    mut count: i32 = 0
    if a - 1u8 == 254u8 {
        count = count + 1
    }
    if b - 1u16 == 65534u16 {
        count = count + 1
    }
    if c - 1u32 == 4294967294u32 {
        count = count + 1
    }
    if d - 1u64 == 18446744073709551614u64 {
        count = count + 1
    }
    return count
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 4);
}

#[test]
fn regression_signed_minimum_in_every_literal_base() {
    // A magnitude one past the maximum has to survive whichever base it is spelled in,
    // not just decimal: the range check runs after the base is decoded.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "signed_min_bases.nr",
            r#"
func main() -> i32 {
    val hex: i32 = -0x80000000
    val oct: i32 = -0o20000000000
    val bin: i32 = -0b10000000000000000000000000000000
    mut count: i32 = 0
    if hex == -2147483648 {
        count = count + 1
    }
    if oct == hex {
        count = count + 1
    }
    if bin == hex {
        count = count + 1
    }
    return count
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 3);
}

#[test]
fn regression_one_past_the_bound_is_still_rejected() {
    // Widening the carrier must not widen what the checker accepts: each literal below
    // is one step outside its type and has to stay an error.
    for (name, source) in [
        (
            "i8_low",
            "func main() -> i32 { val x: i8 = -129\n return 0 }",
        ),
        (
            "i8_high",
            "func main() -> i32 { val x: i8 = 128\n return 0 }",
        ),
        (
            "i32_low",
            "func main() -> i32 { val x: i32 = -2147483649\n return 0 }",
        ),
        (
            "i32_high",
            "func main() -> i32 { val x: i32 = 2147483648\n return 0 }",
        ),
        // `val x: u8 = -1` is deliberately absent: negating an unsigned literal keeps
        // its existing wrapping meaning (it yields 255), which is a separate open
        // defect and not something the bound check above changes.
        (
            "i8_suffixed_low",
            "func main() -> i32 { val x = -129i8\n return 0 }",
        ),
    ] {
        let test = CompileTest::new();
        let source_path = test.write_source(&format!("{name}.nr"), source);
        assert!(
            test.compile(&source_path).is_err(),
            "{name} compiled but is out of range for its type"
        );
    }
}
