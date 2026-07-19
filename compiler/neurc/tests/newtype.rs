// End-to-end tests for newtype declarations: `newtype Name = T` creates a
// distinct nominal type wrapping `T`. Construction is `Name(value)`, the inner
// value is read via `.0`, and — unlike a transparent `type` alias — the newtype is
// not interchangeable with its inner type. These tests exercise the full pipeline:
// parse → type-check → HIR lowering → LLVM codegen → native run.
mod common;
use common::CompileTest;

#[test]
fn newtype_construction_and_inner_access_run() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(29)
    m.0
}
"#;
    let exit = test
        .compile_and_run("newtype_basic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 29);
}

#[test]
fn newtype_crosses_function_boundaries() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
func add(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}
func main() -> i32 {
    val total: Meters = add(Meters(17), Meters(25))
    total.0
}
"#;
    let exit = test
        .compile_and_run("newtype_boundaries.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn newtype_over_float_inner_runs() {
    // A newtype forwards Copy from its inner type; an f64 wrapper works end to end.
    let test = CompileTest::new();
    let source = r#"
newtype Celsius = f64
func main() -> i32 {
    val t: Celsius = Celsius(36.5)
    val raw: f64 = t.0
    raw as i32
}
"#;
    let exit = test
        .compile_and_run("newtype_float.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 36);
}

#[test]
fn newtype_as_struct_field_runs() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
newtype Seconds = i32
struct Trip {
    distance: Meters,
    duration: Seconds
}
func score(t: &Trip) -> i32 {
    t.distance.0 + t.duration.0
}
func main() -> i32 {
    val trip = Trip { distance: Meters(30), duration: Seconds(12) }
    score(&trip)
}
"#;
    let exit = test
        .compile_and_run("newtype_field.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}
