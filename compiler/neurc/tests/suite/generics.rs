// End-to-end tests for generic functions with monomorphization.
//
// A generic function is a template: each distinct set of concrete type arguments
// produces its own specialized native function, so `T` carries zero runtime cost and
// is fully erased before codegen. These tests drive the whole pipeline
// (parse → type-check → HIR lowering → LLVM → native binary) and assert on the
// program's exit code.
use crate::compile_harness::CompileTest;

#[test]
fn identity_at_multiple_types() {
    let test = CompileTest::new();
    // One template `identity<T>` instantiated at i32 and f64 (two distinct instances)
    // plus a reuse of the i32 instance.
    let source = r#"
func identity<T>(x: T) -> T {
    x
}

func main() -> i32 {
    val a = identity(40)      // identity<i32>
    val f = identity(2.5)     // identity<f64>: distinct instance, not used in the result
    val b = identity(a)       // reuse identity<i32>
    return identity(b) + 2    // 40 + 2 = 42
}
"#;
    let exit = test
        .compile_and_run("generic_identity.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn generic_choose_uses_if_branch() {
    let test = CompileTest::new();
    let source = r#"
func choose<T>(cond: bool, a: T, b: T) -> T {
    if cond { a } else { b }
}

func main() -> i32 {
    val hit = choose(true, 7, 99)
    val miss = choose(false, 1, 5)
    return hit + miss    // 7 + 5 = 12
}
"#;
    let exit = test
        .compile_and_run("generic_choose.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 12);
}

#[test]
fn multi_param_and_nested_generic_calls() {
    let test = CompileTest::new();
    // `second<T, U>` returns its second argument; `rewrap<T>` forwards to `wrap<T>`,
    // exercising a generic function calling another generic function.
    let source = r#"
func second<T, U>(a: T, b: U) -> U {
    b
}

func wrap<T>(v: T) -> T {
    v
}

func rewrap<T>(v: T) -> T {
    wrap(v)
}

func main() -> i32 {
    val s = second(1.5, 9)    // second<f64, i32> -> 9
    val w = rewrap(33)        // rewrap<i32> -> wrap<i32> -> 33
    return s + w              // 9 + 33 = 42
}
"#;
    let exit = test
        .compile_and_run("generic_nested.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn tuple_type_argument_crosses_boundary() {
    let test = CompileTest::new();
    let source = r#"
func wrap<T>(v: T) -> T {
    v
}

func main() -> i32 {
    val pair = wrap((15, 27))   // wrap<(i32, i32)>
    return pair.0 + pair.1      // 15 + 27 = 42
}
"#;
    let exit = test
        .compile_and_run("generic_tuple.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn operation_needing_a_bound_is_a_type_error() {
    let test = CompileTest::new();
    // Without the trait system a bare `T` has no `+`, so the body must not type-check.
    let source = r#"
func bad<T>(a: T, b: T) -> T {
    a + b
}

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("generic_no_bound.nr", source);
    let err = test
        .compile(&path)
        .expect_err("arithmetic on an unbounded generic parameter must be rejected");
    assert!(
        err.contains("cannot apply binary operator + to types T and T"),
        "{err}"
    );
}

#[test]
fn non_copy_type_argument_runs_end_to_end() {
    let test = CompileTest::new();
    // One template instantiated at a non-`Copy` struct and at `string`, each moved in
    // by value and moved back out, alongside the `Copy` instance that shares the name.
    let source = r#"
struct Holder {
    name: string
}

func identity<T>(x: T) -> T {
    x
}

func main() -> i32 {
    val h = identity(Holder { name: "held" })
    val s = identity("text")
    println("{h.name} {s}")
    return identity(40) + 2
}
"#;
    let exit = test
        .compile_and_run("generic_non_copy.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn a_drop_value_through_a_generic_by_value_drops_once() {
    let test = CompileTest::new();
    // The by-value ABI's correctness condition: a destructor runs exactly once per
    // value whether the generic callee swallows it or hands it back. Two guards go in,
    // so a duplicated owner would report more than 2 and a lost one fewer.
    let source = r#"
struct Guard {
    sink: &mut i32
}

impl Drop for Guard {
    func drop(&mut self) {
        *self.sink = *self.sink + 1
    }
}

func swallow<T>(v: T) -> i32 {
    0
}

func passthrough<T>(v: T) -> T {
    v
}

func main() -> i32 {
    mut dropped: i32 = 0
    {
        val a = Guard { sink: &mut dropped }
        val ignored = swallow(a)
    }
    {
        val b = Guard { sink: &mut dropped }
        val c = passthrough(b)
    }
    return dropped
}
"#;
    let exit = test
        .compile_and_run("generic_drop_once.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 2);
}

#[test]
fn moving_an_abstract_capture_out_of_a_closure_is_rejected() {
    let test = CompileTest::new();
    // The capture is borrowed, since `T` is not known to be `Copy`, so moving it out
    // would hand every call the same buffer to free at `T = string`.
    let source = r#"
func hold<T>(v: T) -> i32 {
    val f = |x: i32| -> i32 {
        val held = v
        x
    }
    f(1)
}

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("generic_capture.nr", source);
    let err = test
        .compile(&path)
        .expect_err("a closure capturing an abstract-typed binding must be rejected");
    assert!(err.contains("closure moves captured 'v' out"), "{err}");
}

#[test]
fn regression_bug_105_a_literal_takes_its_concrete_or_bound_parameter_type() {
    // `second: i64` is concrete, so `5` is an `i64` as in a non-generic call; a type
    // parameter the turbofish or an earlier argument has bound types a literal the same
    // way. Free functions, methods, associated functions and generic types alike.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "generic_literals.nr",
            r#"
struct Shelf { left: bool }
impl Shelf {
    func pick<U>(&self, a: U, b: U) -> U { if self.left { a } else { b } }
    func mix<U>(&self, a: U, n: i64) -> i64 { n }
    func make<U>(u: U, n: i64) -> i64 { n }
}
struct Cell<T> { v: T }
impl<T> Cell<T> {
    func fill<U>(u: U, n: i64) -> Option<U> { Option::Some(u) }
    func scale(&self, k: i64) -> i64 { k * 2 }
}
func gen<T>(first: T, second: i64) -> T { first }
func both<T>(a: T, b: T) -> T { b }
func main() -> i32 {
    val x: i32 = gen(1, 5)
    val s = Shelf { left: false }
    val w = s.pick::<i64>(3, 4)
    val m = s.mix(true, 10)
    val k = Shelf::make(1u8, 20)
    val f = Cell::fill::<i32, i64>(9, 2) ?? 0i64
    val b = both(1i64, 2)
    val c = Cell { v: 1 }
    val sc = c.scale(3)
    x + w as i32 + m as i32 + k as i32 + f as i32 + b as i32 + sc as i32 + 10
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 62);

    // A value of another type is still a mismatch; only a literal follows its context.
    let err = test
        .check(
            "generic_mismatch.nr",
            r#"
func gen<T>(first: T, second: i64) -> T { first }
func both<T>(a: T, b: T) -> T { b }
func main() -> i32 {
    val y: i32 = 3
    val a = gen(1, 2.5)
    val b = both(1i64, 2.5)
    val c = gen(1, y)
    val d = gen(1, 300000000000)
    0
}
"#,
        )
        .expect_err("mismatched arguments must be refused");
    assert_eq!(err.matches("type mismatch").count(), 3, "{err}");
}

#[test]
fn regression_bug_106_instances_whose_type_arguments_spell_alike_get_their_own_symbols() {
    // A struct named `tup2_i32_i32` against the tuple `(i32, i32)`, `Pair<A_B, C>`
    // against `Pair<A, B_C>`, a struct named `ref_i32` against `&i32`, and two
    // function types: each pair used to mangle to one symbol.
    let exit = CompileTest::new()
        .compile_and_run(
            "mangle.nr",
            r#"
struct A_B { v: i32 }
struct B_C { v: i32 }
struct A { v: i32 }
struct C { v: i32 }
struct ref_i32 { v: i32 }
struct Pair<X, Y> { x: X, y: Y }
func first<X, Y>(p: Pair<X, Y>) -> X { p.x }
func id<T>(x: T) -> T { x }
func inc(n: i32) -> i32 { n + 1 }
func neg(b: bool) -> bool { !b }
func apply<F>(f: F, n: i32) -> i32 { 0 }
func main() -> i32 {
    val p1 = Pair { x: A_B { v: 1 }, y: C { v: 2 } }
    val p2 = Pair { x: A { v: 3 }, y: B_C { v: 4 } }
    val a = first(p1).v + first(p2).v
    val r = id(ref_i32 { v: 10 }).v
    val x = 5
    val q = *id(&x)
    val f = id(inc)
    val g = id(neg)
    a + r + q + f(30) + (if g(false) { 7 } else { 0 })
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 57);
}

#[test]
fn regression_bug_106_a_function_named_like_an_instance_keeps_its_own_symbol() {
    // `size_g_i32` was the symbol of `size<i32>`, so `size(5)` called the program's own
    // `size_g_i32` and returned 105.
    let exit = CompileTest::new()
        .compile_and_run(
            "instance_name.nr",
            r#"
func size<T>(x: T) -> T { x }
func size_g_i32(x: i32) -> i32 { x + 100 }
func main() -> i32 {
    val a = size(5)
    val b = size_g_i32(1)
    a + b - 94
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 12);
}
