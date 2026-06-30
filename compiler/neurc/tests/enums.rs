// End-to-end tests for enums with associated data (§3.5): declaration, the three
// construction forms (unit / tuple / struct), and enums flowing across functions
// and into struct fields. Pattern matching (payload extraction) is a separate
// roadmap item, so these assert construction + round-trip, not deconstruction.

mod common;
use common::CompileTest;

#[test]
fn constructs_all_three_variant_forms() {
    let test = CompileTest::new();
    let source = r#"
enum Color { Red, Green, Blue }

enum Shape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 }
}

enum Msg { Quit, Move(i32, i32), Write(bool) }

func main() -> i32 {
    val c = Color::Red
    val s = Shape::Circle { radius: 5.0 }
    val r = Shape::Rectangle { width: 2.0, height: 3.0 }
    val m = Msg::Move(1, 2)
    val w = Msg::Write(true)
    val q = Msg::Quit
    return 0
}
"#;
    let exit = test
        .compile_and_run("enum_forms.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn enum_crosses_function_boundaries() {
    let test = CompileTest::new();
    let source = r#"
enum Status { Ok, Busy, Failed }

func make() -> Status {
    Status::Busy
}

func consume(s: Status) -> i32 {
    42
}

func main() -> i32 {
    val s = make()
    consume(s) - 42
}
"#;
    let exit = test
        .compile_and_run("enum_fn.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn enum_as_struct_field() {
    let test = CompileTest::new();
    let source = r#"
enum Kind { A, B }

struct Tagged {
    kind: Kind,
    value: i32
}

func main() -> i32 {
    val t = Tagged { kind: Kind::B, value: 9 }
    t.value - 9
}
"#;
    let exit = test
        .compile_and_run("enum_field.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn rejects_unknown_variant() {
    let test = CompileTest::new();
    let source = r#"
enum Color { Red, Green }

func main() -> i32 {
    val c = Color::Blue
    0
}
"#;
    let path = test.write_source("enum_bad_variant.nr", source);
    let err = test.compile(&path).expect_err("expected a type error");
    assert!(
        err.contains("Blue"),
        "diagnostic should name the variant: {err}"
    );
}

#[test]
fn rejects_wrong_construction_form() {
    let test = CompileTest::new();
    let source = r#"
enum Shape { Circle { radius: f64 } }

func main() -> i32 {
    val s = Shape::Circle(5.0)
    0
}
"#;
    let path = test.write_source("enum_bad_form.nr", source);
    let err = test.compile(&path).expect_err("expected a type error");
    assert!(
        err.contains("struct"),
        "diagnostic should mention the variant is a struct variant: {err}"
    );
}

#[test]
fn rejects_non_scalar_payload() {
    let test = CompileTest::new();
    let source = r#"
enum Bad { Holds(string) }

func main() -> i32 {
    0
}
"#;
    let path = test.write_source("enum_bad_payload.nr", source);
    let err = test.compile(&path).expect_err("expected a type error");
    assert!(
        err.contains("payload") || err.contains("scalar"),
        "diagnostic should reject the non-scalar payload: {err}"
    );
}
