// Integration tests for compound assignment operators (+=, -=, *=, /=, %=, &=, |=, ^=, <<=) on a
// plain binding. The operator survives parsing and is resolved by the target's type,
// so these tests also validate that the desugaring path reaches semantic analysis
// and codegen correctly for both integer and float types. The same operators over a
// field, an element, or a referent are in `place_expressions`.
use crate::compile_harness::CompileTest;

#[test]
fn test_plus_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 10
    x += 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("plus_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 15);
}

#[test]
fn test_minus_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 20
    x -= 7
    return x
}
"#;
    let exit_code = test
        .compile_and_run("minus_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 13);
}

#[test]
fn test_star_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 6
    x *= 7
    return x
}
"#;
    let exit_code = test
        .compile_and_run("star_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 42);
}

#[test]
fn test_slash_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 50
    x /= 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("slash_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 10);
}

#[test]
fn test_percent_equal_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 17
    x %= 5
    return x
}
"#;
    let exit_code = test
        .compile_and_run("percent_equal_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 2);
}

#[test]
fn test_compound_assignment_chained_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut n: i32 = 1
    n += 4
    n *= 3
    n -= 2
    return n
}
"#;
    // (1+4)*3-2 = 13
    let exit_code = test
        .compile_and_run("chained_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 13);
}

#[test]
fn test_compound_assignment_in_loop_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut sum: i32 = 0
    mut i: i32 = 0
    while i < 5 {
        sum += i
        i += 1
    }
    return sum
}
"#;
    // 0+1+2+3+4 = 10
    let exit_code = test
        .compile_and_run("loop_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 10);
}

#[test]
fn test_slash_equal_i32_truncates() {
    let test = CompileTest::new();
    // Integer division truncates; verify compound /= also truncates
    let source = r#"
func main() -> i32 {
    mut x: i32 = 7
    x /= 2
    return x
}
"#;
    let exit_code = test
        .compile_and_run("slash_equal_trunc.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 3);
}

#[test]
fn test_desugaring_equivalence() {
    let test = CompileTest::new();
    // Verify that x += n produces identical results to x = x + n
    let source_compound = r#"
func main() -> i32 {
    mut x: i32 = 7
    x += 3
    return x
}
"#;
    let source_explicit = r#"
func main() -> i32 {
    mut x: i32 = 7
    x = x + 3
    return x
}
"#;
    let r1 = test
        .compile_and_run("desugar_compound.nr", source_compound)
        .expect("compilation failed");
    let r2 = test
        .compile_and_run("desugar_explicit.nr", source_explicit)
        .expect("compilation failed");
    assert_eq!(
        r1, r2,
        "compound assignment must desugar to equivalent plain assignment"
    );
}

#[test]
fn test_bitwise_compound_assignment_i32() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: i32 = 0b1100
    x &= 0b1010
    if x != 0b1000 { return 1 }
    x |= 0b0011
    if x != 0b1011 { return 2 }
    x ^= 0b0110
    if x != 0b1101 { return 3 }
    x <<= 2
    return x
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_compound_i32.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 52);
}

#[test]
fn test_bitwise_compound_assignment_matches_its_desugaring() {
    let test = CompileTest::new();
    // Unsigned and 64-bit targets, with a right-hand side that is itself a bitwise
    // expression: the operator applies to the whole value, `x = x OP (rhs)`.
    let source = r#"
func main() -> i32 {
    mut a: u8 = 0xF0u8
    mut b: u8 = 0xF0u8
    a ^= 0x0Fu8 | 0x30u8
    b = b ^ (0x0Fu8 | 0x30u8)
    if a != b { return 1 }

    mut wide: i64 = 1
    wide <<= 40
    if wide != 1099511627776 { return 2 }
    wide |= 1 << 3
    wide &= 0xFF
    return wide as i32
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_compound_desugar.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 8);
}

#[test]
fn test_bitwise_compound_assignment_on_places() {
    let test = CompileTest::new();
    let source = r#"
struct Flags { bits: u32 }

func set(r: &mut u32, mask: u32) {
    *r |= mask
}

func main() -> i32 {
    mut f = Flags { bits: 0u32 }
    f.bits |= 0b101u32
    mut arr: [u32; 3] = [1u32, 2u32, 3u32]
    arr[1] <<= 3u32
    arr[2] ^= f.bits
    mut pair = (0xFFu32, 0u32)
    pair.0 &= 0x0Fu32
    set(&mut pair.1, 64u32)
    // 5 + 16 + 6 + 15 + 64
    return (f.bits + arr[1] + arr[2] + pair.0 + pair.1) as i32
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_compound_places.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 106);
}

#[test]
fn test_bitwise_compound_assignment_rejects_a_float_target() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut x: f64 = 1.0
    x |= 2.0
    return 0
}
"#;
    let error = test
        .check("bitwise_compound_float.nr", source)
        .expect_err("`|=` on a float must not type-check");
    assert!(error.contains("binary operator |"), "got: {error}");
}

#[test]
fn test_bitwise_compound_assignment_rejects_an_immutable_target() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val x: i32 = 1
    x <<= 2
    return x
}
"#;
    let error = test
        .check("bitwise_compound_immutable.nr", source)
        .expect_err("`<<=` on a `val` must not type-check");
    assert!(error.contains("immutable"), "got: {error}");
}

#[test]
fn test_bitwise_compound_assignment_dispatches_through_the_operator_trait() {
    // `m |= n` desugars to `m = m | n`, which a user type answers with its `BitOr` impl.
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Mask { bits: i32 }
impl BitOr for Mask { type Output = Mask
    func bitor(self, rhs: Mask) -> Mask { Mask { bits: self.bits | rhs.bits } } }
func main() -> i32 {
    mut m = Mask { bits: 1 }
    m |= Mask { bits: 4 }
    m |= Mask { bits: 16 }
    m.bits
}
"#;
    let exit_code = test
        .compile_and_run("bitwise_compound_trait.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 21);
}
