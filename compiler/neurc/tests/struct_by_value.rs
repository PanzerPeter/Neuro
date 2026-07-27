// Structs held by value in positions that are not an `impl` method receiver:
// free-function parameters and returns, and a struct field of another struct.
// The backend needs the struct's field layout to build its LLVM type in each of
// these; the frontend accepts them all, so codegen must too.

mod common;
use common::CompileTest;

#[test]
fn struct_as_free_function_parameter_and_return() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_param_return.nr",
            r#"
struct Item { id: i32 }
struct Shelf { n: i32 }

func classify(item: Item) -> Shelf { Shelf { n: item.id * 2 } }

func main() -> i32 {
    val s: Shelf = classify(Item { id: 21 })
    s.n
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 42);
}

#[test]
fn struct_argument_does_not_alias_the_caller_value() {
    // The parameter is bound to its own alloca, so a write in the callee cannot
    // reach the caller's value. `Copy` lets the caller keep reading its own.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_param_copy.nr",
            r#"
@derive(Copy, Clone)
struct Counter { n: i32 }

func bump(c: Counter) -> i32 {
    mut local: Counter = c
    local.n = local.n + 100
    local.n
}

func main() -> i32 {
    val original: Counter = Counter { n: 5 }
    val inner: i32 = bump(original)
    val outer: i32 = original.n
    inner - outer
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 100);
}

#[test]
fn struct_field_of_a_struct() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_nested_field.nr",
            r#"
struct Inner { v: i32 }
struct Outer { inner: Inner, k: i32 }

func unwrap(o: Outer) -> Inner { o.inner }

func main() -> i32 {
    val o: Outer = Outer { inner: Inner { v: 5 }, k: 2 }
    val k: i32 = o.k
    val i: Inner = unwrap(o)
    i.v * k
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 10);
}

#[test]
fn chained_field_access_reads_through_a_nested_struct() {
    // `o.inner.v` reads a field of a field: the intermediate struct is a value, so
    // the read extracts from it rather than addressing a named binding.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_chained_field.nr",
            r#"
struct Inner { v: i32 }
struct Middle { inner: Inner, w: i32 }
struct Outer { middle: Middle, k: i32 }

func main() -> i32 {
    val o: Outer = Outer { middle: Middle { inner: Inner { v: 7 }, w: 3 }, k: 2 }
    val deep: i32 = o.middle.inner.v
    val mid: i32 = o.middle.w
    deep * mid
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 21);
}

#[test]
fn struct_round_trips_through_several_calls() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_round_trip.nr",
            r#"
struct Point { x: i32, y: i32 }

func shift(p: Point) -> Point { Point { x: p.x + 1, ..p } }
func total(p: Point) -> i32 { p.x + p.y }

func main() -> i32 {
    val a: Point = Point { x: 1, y: 10 }
    val b: Point = shift(a)
    val c: Point = shift(b)
    total(c)
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 13);
}

#[test]
fn return_position_impl_trait_yielding_a_struct() {
    // `impl Trait` in return position resolves to the body's concrete struct, which
    // reaches codegen as a by-value struct return.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "impl_trait_struct_return.nr",
            r#"
trait Shape {
    func area(&self) -> i32
}

struct Square { side: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}

func make(side: i32) -> impl Shape { Square { side: side } }

func main() -> i32 {
    val s: Square = make(6)
    s.area()
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 36);
}

#[test]
fn by_value_struct_parameter_with_drop_is_destroyed_once() {
    // A `Drop` value moved into a free function is owned by it and destroyed when the
    // callee returns — exactly once, as for a method parameter.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "struct_param_drop.nr",
            r#"
struct Probe { sink: &mut i32 }

impl Drop for Probe {
    func drop(&mut self) { *self.sink = *self.sink + 1 }
}

func consume(p: Probe) -> i32 { 0 }

func main() -> i32 {
    mut count: i32 = 0
    {
        val p = Probe { sink: &mut count }
        val ignored: i32 = consume(p)
    }
    return count
}
"#,
        )
        .expect("compilation failed");
    assert_eq!(exit, 1, "the destructor must run exactly once");
}
