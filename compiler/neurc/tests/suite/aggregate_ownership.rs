//! Non-`Copy` values held in aggregates: arrays, tuples, enum payloads, newtypes, and
//! generic struct or enum type arguments.
//!
//! The aggregate becomes the owner, so the rules under test are ownership rules rather
//! than layout ones: the aggregate moves when it holds a move-tracked value, a sub-place
//! gives up one element at a time, and reading the aggregate as a whole afterwards is a
//! use of a partially moved value.
use crate::compile_harness::CompileTest;

#[test]
fn an_array_of_strings_round_trips_through_codegen() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val names: [string; 3] = ["alpha", "beta", "gamma"]
    println(names[1])
    names[2].len() as i32
}
"#;
    let exit = test
        .compile_and_run("agg_array_strings.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn a_tuple_carries_a_string_out_of_a_function() {
    let test = CompileTest::new();
    let source = r#"
func split(head: string) -> (string, i32) {
    (head, 7)
}

func main() -> i32 {
    val pair = split("head")
    println(pair.0)
    pair.1
}
"#;
    let exit = test
        .compile_and_run("agg_tuple_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn every_aggregate_shape_destructures_element_by_element() {
    // The destructure desugar binds one projection per leaf, so a rule that collapsed
    // each of them onto the root binding rejected the second leaf of every pattern.
    let test = CompileTest::new();
    let source = r#"
struct Two { a: string, b: string }

func main() -> i32 {
    val Two { a, b } = Two { a: "x", b: "yy" }
    val (l, r) = ("p", "qqq")
    val [i, j] = ["m", "nnnn"]
    (a.len() + b.len() + l.len() + r.len() + i.len() + j.len()) as i32
}
"#;
    let exit = test
        .compile_and_run("agg_destructure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn reading_an_aggregate_whole_after_a_partial_move_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Two { a: string, b: string }

func take(t: Two) -> i32 { 0 }

func main() -> i32 {
    val t = Two { a: "x", b: "y" }
    val a = t.a
    take(t)
}
"#;
    let path = test.write_source("agg_partial_move.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a partially moved value must not be passed on");
    assert!(
        err.contains("use of moved value 't'"),
        "expected a moved-value diagnostic naming the root, got: {err}"
    );
}

#[test]
fn taking_the_same_element_twice_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val pair: (string, string) = ("p", "q")
    val first = pair.0
    val again = pair.0
    0
}
"#;
    let path = test.write_source("agg_same_element.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "one element may be given away once, not twice"
    );
}

#[test]
fn an_element_taken_by_a_runtime_index_moves_the_whole_array() {
    // The compiler cannot say which element `a[i]` gave away, so the conservative
    // answer is the only sound one.
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val names: [string; 2] = ["a", "b"]
    mut i = 0
    val taken = names[i]
    println(names[1])
    0
}
"#;
    let path = test.write_source("agg_runtime_index.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a move through a runtime index must take the whole binding"
    );
}

#[test]
fn an_enum_payload_holds_a_string_a_struct_and_an_array() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32, y: i32 }

enum Cell {
    Text(string),
    At(Point),
    Row([string; 2]),
    Empty,
}

func weigh(c: Cell) -> i32 {
    match c {
        Cell::Text(t) => t.len() as i32
        Cell::At(p) => p.x + p.y
        Cell::Row(r) => r[1].len() as i32
        Cell::Empty => 0
    }
}

func main() -> i32 {
    weigh(Cell::Text("abcd"))
        + weigh(Cell::At(Point { x: 3, y: 4 }))
        + weigh(Cell::Row(["a", "bbb"]))
        + weigh(Cell::Empty)
}
"#;
    let exit = test
        .compile_and_run("agg_enum_payload.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}

#[test]
fn an_enum_payload_may_name_a_struct_declared_after_it() {
    // Enum names are registered before structs and enum payloads after, so neither
    // declaration order is the special case.
    let test = CompileTest::new();
    let source = r#"
enum Holder { At(Point), Empty }

struct Point { x: i32, y: i32 }

func main() -> i32 {
    match Holder::At(Point { x: 2, y: 3 }) {
        Holder::At(p) => p.x * p.y
        Holder::Empty => 0
    }
}
"#;
    let exit = test
        .compile_and_run("agg_enum_forward_struct.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

#[test]
fn option_and_vec_carry_a_string_payload() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut words: Vec<string> = Vec::new()
    words.push("alpha")
    words.push("bravo")
    match words.pop() {
        Option::Some(w) => println(w)
        Option::None => println("empty")
    }
    words.len() as i32
}
"#;
    let exit = test
        .compile_and_run("agg_option_string.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn a_newtype_wraps_a_string_and_moves_with_it() {
    let test = CompileTest::new();
    let source = r#"
newtype Name = string

func take(n: Name) -> i32 {
    n.0.len() as i32
}

func main() -> i32 {
    val n = Name("ada")
    take(n)
}
"#;
    let exit = test
        .compile_and_run("agg_newtype.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 3);

    let reused = r#"
newtype Name = string

func take(n: Name) -> i32 { 0 }

func main() -> i32 {
    val n = Name("ada")
    val first = take(n)
    take(n)
}
"#;
    let path = test.write_source("agg_newtype_moved.nr", reused);
    assert!(
        test.compile(&path).is_err(),
        "a newtype over a non-Copy inner must move, not copy"
    );
}

#[test]
fn a_copy_derive_still_rejects_an_aggregate_field_that_owns() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Bad { names: [string; 2] }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("agg_copy_derive.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an owning array field must block `@derive(Copy)`");
    assert!(
        err.contains("cannot derive Copy"),
        "expected a derive diagnostic, got: {err}"
    );
}
