// End-to-end tests for pattern matching: enum deconstruction with payload
// binding, value/or/range/guard/wildcard patterns, `match` as a value expression,
// and combination with prior features (structs via enum struct variants, functions,
// arithmetic).

mod common;
use common::CompileTest;

#[test]
fn matches_all_enum_variant_forms_with_payload_binding() {
    let test = CompileTest::new();
    let source = r#"
enum Shape {
    Circle(i32),
    Rect { w: i32, h: i32 },
    Unit
}

func area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => r * r * 3,
        Shape::Rect { w, h } => w * h,
        Shape::Unit => 0
    }
}

func main() -> i32 {
    val a = area(Shape::Circle(4))            // 48
    val b = area(Shape::Rect { w: 5, h: 6 })  // 30
    val c = area(Shape::Unit)                 // 0
    a + b + c                                 // 78
}
"#;
    let exit = test
        .compile_and_run("match_enum.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 78);
}

#[test]
fn matches_value_or_range_guard_and_wildcard() {
    let test = CompileTest::new();
    let source = r#"
func classify(n: i32) -> i32 {
    match n {
        0 => 1,
        1 | 2 => 2,
        3..=9 => 3,
        n if n < 0 => 4,
        _ => 9
    }
}

func main() -> i32 {
    mut acc = 0
    if classify(0)  == 1 { acc = acc + 1 }
    if classify(2)  == 2 { acc = acc + 1 }
    if classify(7)  == 3 { acc = acc + 1 }
    if classify(-8) == 4 { acc = acc + 1 }
    if classify(42) == 9 { acc = acc + 1 }
    acc   // 5
}
"#;
    let exit = test
        .compile_and_run("match_values.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn match_is_a_value_expression_bound_to_a_variable() {
    let test = CompileTest::new();
    let source = r#"
enum Dir { North, East, South, West }

func main() -> i32 {
    val d = Dir::South
    val turns = match d {
        Dir::North => 0,
        Dir::East => 1,
        Dir::South => 2,
        Dir::West => 3
    }
    turns * 10 + 3   // 23
}
"#;
    let exit = test
        .compile_and_run("match_value_expr.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 23);
}

#[test]
fn matches_char_and_bool_scrutinees() {
    let test = CompileTest::new();
    let source = r#"
func grade(c: char) -> i32 {
    match c {
        'a' => 4,
        'b' | 'c' => 3,
        _ => 0
    }
}

func flag(b: bool) -> i32 {
    match b {
        true => 10,
        false => 20
    }
}

func main() -> i32 {
    grade('b') + flag(false)   // 3 + 20 = 23
}
"#;
    let exit = test
        .compile_and_run("match_char_bool.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 23);
}

#[test]
fn non_exhaustive_match_fails_to_compile() {
    let test = CompileTest::new();
    let source = r#"
enum E { A, B, C }
func f(e: E) -> i32 {
    match e {
        E::A => 1,
        E::B => 2
    }
}
func main() -> i32 { f(E::A) }
"#;
    let result = test.compile_and_run("match_nonexhaustive.nr", source);
    assert!(
        result.is_err(),
        "a non-exhaustive match must fail to compile"
    );
}
