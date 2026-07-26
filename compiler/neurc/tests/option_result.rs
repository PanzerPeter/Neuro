// End-to-end tests for the standard-library `Option<T>` / `Result<T, E>` enums and the
// generic enums they are built from: construction, annotation-driven and inferred type
// arguments, `match` deconstruction, flow across function and struct boundaries, the
// prelude's shadowing rule, and the phase's documented limits.

mod common;
use common::CompileTest;

#[test]
fn option_is_available_without_declaring_it() {
    let test = CompileTest::new();
    let source = r#"
func unwrap_or(o: Option<i32>, fallback: i32) -> i32 {
    match o {
        Option::Some(v) => v,
        Option::None => fallback
    }
}

func main() -> i32 {
    val present = Option::Some(30)
    val absent: Option<i32> = Option::None
    unwrap_or(present, 0) + unwrap_or(absent, 12)
}
"#;
    let exit = test
        .compile_and_run("option_prelude.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn result_propagates_ok_and_err_payloads() {
    let test = CompileTest::new();
    // The canonical fallible function: both branches of the tail `if` construct the
    // declared return instance, which is what completes their type arguments.
    let source = r#"
func divide(a: i32, b: i32) -> Result<i32, i32> {
    if b == 0 {
        Result::Err(9)
    } else {
        Result::Ok(a / b)
    }
}

func main() -> i32 {
    val good = divide(40, 4)
    val bad = divide(1, 0)
    val a = match good {
        Result::Ok(v) => v,
        Result::Err(e) => 0 - e
    }
    val b = match bad {
        Result::Ok(v) => v,
        Result::Err(e) => e
    }
    a + b
}
"#;
    let exit = test
        .compile_and_run("result_divide.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 19);
}

#[test]
fn option_crosses_struct_fields_and_returns() {
    let test = CompileTest::new();
    let source = r#"
struct Config { retries: Option<i32> }

func make(explicit: bool) -> Option<i32> {
    if explicit {
        Option::Some(5)
    } else {
        Option::None
    }
}

func main() -> i32 {
    val set = Config { retries: make(true) }
    val unset = Config { retries: make(false) }
    val a = match set.retries {
        Option::Some(n) => n,
        Option::None => 0
    }
    val b = match unset.retries {
        Option::Some(n) => n,
        Option::None => 3
    }
    a + b
}
"#;
    let exit = test
        .compile_and_run("option_struct_field.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 8);
}

#[test]
fn distinct_option_instances_are_independent_types() {
    let test = CompileTest::new();
    // One template, three instances (i32 / i64 / char) — each monomorphized to its own
    // tagged union, so the payload keeps its own width.
    let source = r#"
func main() -> i32 {
    val small: Option<i32> = Option::Some(7)
    val wide: Option<i64> = Option::Some(100i64)
    val letter: Option<char> = Option::Some('a')

    mut total: i32 = 0
    total = total + match small {
        Option::Some(v) => v,
        Option::None => 0
    }
    val w = match wide {
        Option::Some(v) => v,
        Option::None => 0i64
    }
    total = total + (w as i32)
    val c = match letter {
        Option::Some(ch) => ch,
        Option::None => 'z'
    }
    if c == 'a' {
        total = total + 1
    }
    total
}
"#;
    let exit = test
        .compile_and_run("option_instances.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 108);
}

#[test]
fn user_defined_generic_enum_monomorphizes() {
    let test = CompileTest::new();
    let source = r#"
enum Slot<T> { Filled(T), Vacant }
enum Tagged<T, U> { Left(T), Right(U) }

func main() -> i32 {
    val n = Slot::Filled(4)
    val f = Slot::Filled(1.5)
    val v: Slot<bool> = Slot::Vacant
    val t: Tagged<i32, bool> = Tagged::Right(true)

    mut total: i32 = 0
    total = total + match n {
        Slot::Filled(x) => x,
        Slot::Vacant => 0
    }
    val scaled = match f {
        Slot::Filled(x) => x * 2.0,
        Slot::Vacant => 0.0
    }
    total = total + (scaled as i32)
    total = total + match v {
        Slot::Filled(b) => 100,
        Slot::Vacant => 6
    }
    total = total + match t {
        Tagged::Left(x) => x,
        Tagged::Right(b) => 2
    }
    total
}
"#;
    let exit = test
        .compile_and_run("user_generic_enum.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn a_local_declaration_shadows_the_prelude_option() {
    let test = CompileTest::new();
    // A program that defines its own `Option` compiles against that definition; the
    // prelude entry is dropped rather than colliding with it.
    let source = r#"
enum Option { Yes, No }

func main() -> i32 {
    val o = Option::Yes
    match o {
        Option::Yes => 21,
        Option::No => 0
    }
}
"#;
    let exit = test
        .compile_and_run("option_shadowed.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 21);
}

#[test]
fn generic_enum_struct_variant_infers_from_fields() {
    let test = CompileTest::new();
    let source = r#"
enum Shape<T> { Circle { radius: T }, Point }

func main() -> i32 {
    val c = Shape::Circle { radius: 12 }
    match c {
        Shape::Circle { radius } => radius,
        Shape::Point => 0
    }
}
"#;
    let exit = test
        .compile_and_run("generic_enum_struct_variant.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn none_without_a_type_annotation_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val nothing = Option::None
    0
}
"#;
    let path = test.write_source("option_uninferable.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a `None` with no context must not compile");
    assert!(
        err.contains("cannot infer the type arguments"),
        "expected an inference diagnostic, got: {err}"
    );
}

#[test]
fn non_scalar_option_payload_is_rejected() {
    let test = CompileTest::new();
    // The scalar-payload restriction applies per instance: `Option<string>` awaits
    // broader payload support.
    let source = r#"
func main() -> i32 {
    val text: Option<string> = Option::None
    0
}
"#;
    let path = test.write_source("option_string.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a non-scalar payload must not compile");
    assert!(
        err.contains("enum variant payload type"),
        "expected a payload diagnostic, got: {err}"
    );
}

#[test]
fn a_partial_match_over_option_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val o = Option::Some(1)
    match o {
        Option::Some(v) => v
    }
}
"#;
    let path = test.write_source("option_partial_match.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a non-exhaustive match must not compile");
    assert!(
        err.contains("non-exhaustive match"),
        "expected an exhaustiveness diagnostic, got: {err}"
    );
}
