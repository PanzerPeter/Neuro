// End-to-end tests for generic functions with monomorphization.
//
// A generic function is a template: each distinct set of concrete type arguments
// produces its own specialized native function, so `T` carries zero runtime cost and
// is fully erased before codegen. These tests drive the whole pipeline
// (parse → type-check → HIR lowering → LLVM → native binary) and assert on the
// program's exit code.
mod common;
use common::CompileTest;

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
    val f = identity(2.5)     // identity<f64> — distinct instance, not used in the result
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
fn non_copy_type_argument_is_rejected() {
    let test = CompileTest::new();
    // Generic type arguments are restricted to Copy types this phase; `string` is not.
    let source = r#"
func identity<T>(x: T) -> T {
    x
}

func main() -> i32 {
    val s = identity("hello")
    return 0
}
"#;
    let path = test.write_source("generic_non_copy.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a non-Copy type argument must be rejected"
    );
}
