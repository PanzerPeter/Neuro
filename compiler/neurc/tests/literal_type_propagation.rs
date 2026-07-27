// An unsuffixed literal is emitted at the type the frontend resolved for it, not at
// the suffix default (`i32` / `f64`). Call arguments and return position are the
// shapes where nothing coerces afterwards, so a mismatch there reaches the verifier.

mod common;
use common::CompileTest;

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
