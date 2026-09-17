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
    assert!(
        test.compile(&path).is_err(),
        "arithmetic on an unbounded generic parameter must be rejected"
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
fn capturing_an_abstract_type_in_a_closure_is_rejected() {
    let test = CompileTest::new();
    // A capture duplicates the binding, which the template cannot allow: at
    // `T = string` every call would hand the closure the same buffer to free.
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
    assert!(
        test.compile(&path).is_err(),
        "a closure capturing an abstract-typed binding must be rejected"
    );
}
