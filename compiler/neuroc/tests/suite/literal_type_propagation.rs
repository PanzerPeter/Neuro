// An unsuffixed literal is emitted at the type the frontend resolved for it, not at
// the suffix default (`i32` / `f64`). Call arguments and return position are the
// shapes where nothing coerces afterwards, so a mismatch there reaches the verifier.

use crate::compile_harness::CompileTest;

#[test]
fn unsuffixed_float_literal_as_f32_argument() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_f32_arg.nr",
            r#"
func take(x: f32) -> f32 { x }

func main() -> i32 {
    val a: f32 = take(0.75)
    val scaled: f32 = a * 4.0f32
    scaled as i32
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 3);
}

#[test]
fn unsuffixed_integer_literal_as_wide_and_narrow_argument() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_int_arg.nr",
            r#"
func wide(x: i64) -> i64 { x }
func narrow(x: u8) -> u8 { x }

func main() -> i32 {
    val a: i64 = wide(40)
    val b: u8 = narrow(2)
    val ai: i32 = a as i32
    val bi: i32 = b as i32
    ai + bi
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 42);
}

#[test]
fn unsuffixed_literal_in_return_position() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_return.nr",
            r#"
func half() -> f32 { 0.5 }
func eight() -> i64 { 8 }

func main() -> i32 {
    val h: f32 = half()
    val e: i64 = eight()
    val scaled: f32 = h * 4.0f32
    val si: i32 = scaled as i32
    val ei: i32 = e as i32
    si + ei
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 10);
}

#[test]
fn unsuffixed_literal_as_method_and_associated_function_argument() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_method_arg.nr",
            r#"
struct Scale { factor: f32 }

impl Scale {
    func apply(&self, k: f32) -> f32 { k * self.factor }
    func of(k: i64) -> i64 { k }
}

func main() -> i32 {
    val s: Scale = Scale { factor: 2.0f32 }
    val v: f32 = s.apply(1.5)
    val n: i64 = Scale::of(9)
    val vi: i32 = v as i32
    val ni: i32 = n as i32
    vi + ni
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 12);
}

#[test]
fn unsuffixed_literal_through_an_indirect_call() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_indirect_arg.nr",
            r#"
func main() -> i32 {
    val f: (f32) -> f32 = |x: f32| x
    val v: f32 = f(0.25)
    val scaled: f32 = v * 8.0f32
    scaled as i32
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 2);
}

#[test]
fn suffixed_and_default_literals_keep_their_own_type() {
    // The resolved type is the frontend's, and a suffix is what fixes it there;
    // an unannotated binding still defaults to i32 / f64.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "lit_suffix_kept.nr",
            r#"
func main() -> i32 {
    val a = 1.5f32
    val b = 40i64
    val c = 2
    val ai: i32 = a as i32
    val bi: i32 = b as i32
    ai + bi + c
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 43);
}

#[test]
fn test_bug_086_an_annotation_types_the_literals_of_arithmetic() {
    // `val x: u8 = 200` types its literal by the annotation, and so does arithmetic
    // made of literals alone, in a binding and in a constant. It computes in the
    // annotated type: `2147483647 + 1` is `2147483648` as an `i64`.
    let source = r#"
const Z: u8 = 255 - 255
const MASK: u16 = (1 << 12) | 0xff

func main() -> i32 {
    val d: u8 = 200 + 50
    val big: i64 = 2147483647 + 1
    val huge: i64 = 5000000000 * 2
    val f: f32 = 1.5 * 2.0
    if d != 250u8 { return 1 }
    if big != 2147483648i64 { return 2 }
    if huge != 10000000000i64 { return 3 }
    if f != 3.0f32 { return 4 }
    if MASK != 4351u16 { return 5 }
    Z as i32 + 42
}
"#;
    let code = CompileTest::new()
        .compile_and_run("literal_arithmetic.nr", source)
        .expect("literal arithmetic takes the annotated type");
    assert_eq!(code, 42);

    // The annotation still bounds the result.
    let error = CompileTest::new()
        .check(
            "literal_overflow.nr",
            "func main() -> i32 {\n    val d: u8 = 300 + 1\n    0\n}",
        )
        .expect_err("300 does not fit u8");
    assert!(error.contains("out of range for type u8"), "got: {error}");
}

#[test]
fn test_bug_091_a_literal_range_start_takes_the_end_s_type() {
    let source = r#"
func sum_vec(v: &Vec<i32>) -> i32 {
    mut s = 0
    for i in 0..v.len() {
        s = s + v[i]
    }
    s
}

func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(10)
    v.push(20)
    v.push(5)
    val wide: i64 = 4
    mut steps = 0
    for k in 0..=wide {
        steps = steps + 1
    }
    val n = 3
    mut m = 0
    for j in 0..n {
        m = m + j
    }
    sum_vec(&v) + steps + m
}
"#;
    let code = CompileTest::new()
        .compile_and_run("literal_range_start.nr", source)
        .expect("`0..v.len()` and `0..=wide` type the literal start by the end");
    assert_eq!(code, 43);
}

#[test]
fn test_bug_100_a_literal_left_of_a_scalar_takes_its_type() {
    // `0.5 * x` means `x * 0.5`: the literal takes the type of the scalar beside it, of
    // its own kind, on either side of an arithmetic, bitwise or comparison operator.
    let source = r#"
func main() -> i32 {
    val x: f32 = 0.3
    val n: i64 = 5000000000
    val u: u8 = 200
    if 0.1 * x != x * 0.1 { return 1 }
    if 1.0 - x != -(x - 1.0) { return 2 }
    if 3 * n != n * 3 { return 3 }
    if 255 - u != 55 { return 4 }
    if !(0.25 < x) { return 5 }
    if (0xF0 & u) != 192 { return 6 }
    val y = 2.5 * x + 1.0
    val z: f32 = x * 2.5 + 1.0
    if y != z { return 7 }
    42
}
"#;
    let code = CompileTest::new()
        .compile_and_run("literal_left_of_scalar.nr", source)
        .expect("a literal on the left takes the other operand's type");
    assert_eq!(code, 42);

    // Only within its own kind, and the type still bounds the literal.
    let cases = [
        ("val x: f32 = 2.0\n    val y = 2 * x", "type mismatch"),
        (
            "val u: u8 = 3\n    val y = 300 * u",
            "out of range for type u8",
        ),
    ];
    for (body, expected) in cases {
        let source = format!("func main() -> i32 {{\n    {body}\n    0\n}}");
        let error = CompileTest::new()
            .check("literal_left_rejected.nr", &source)
            .expect_err("the literal cannot take that type");
        assert!(error.contains(expected), "got: {error}");
    }
}
