// End-to-end tests for generic structs and generic inherent impls.
//
// A generic struct / impl is a template: each distinct set of concrete type arguments
// produces its own specialized struct and methods, monomorphized before codegen at
// zero runtime cost. These tests drive the whole pipeline
// (parse → type-check → HIR lowering → LLVM → native binary) and assert on the exit code.
mod common;
use common::CompileTest;

#[test]
fn generic_struct_literal_infers_and_reads_field() {
    let test = CompileTest::new();
    // `Pair<T, U>` inferred from the field values; the field read is concrete i32.
    let source = r#"
struct Pair<T, U> {
    first: T,
    second: U
}

func main() -> i32 {
    val p = Pair { first: 40, second: 2.5 }
    return p.first + 2       // 40 + 2 = 42
}
"#;
    let exit = test
        .compile_and_run("gstruct_pair.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn generic_struct_annotation_crosses_function_boundary() {
    let test = CompileTest::new();
    // A `&Pair<i32, i32>` parameter annotation resolves to the same monomorphized
    // instance the literal produces, so the borrow reads its fields across the call.
    let source = r#"
struct Pair<T, U> {
    first: T,
    second: U
}

func sum(p: &Pair<i32, i32>) -> i32 {
    p.first + p.second
}

func main() -> i32 {
    val p = Pair { first: 30, second: 12 }
    return sum(&p)           // 30 + 12 = 42
}
"#;
    let exit = test
        .compile_and_run("gstruct_boundary.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn generic_impl_method_monomorphized_per_instance() {
    let test = CompileTest::new();
    // `Cell<T>::get` / `set` instantiated at both i32 and bool: a `&mut self` method
    // mutates the receiver in place, and a distinct bool instance coexists.
    let source = r#"
struct Cell<T> {
    value: T
}

impl<T> Cell<T> {
    func get(&self) -> T {
        self.value
    }
    func set(&mut self, v: T) {
        self.value = v
    }
}

func main() -> i32 {
    mut c = Cell { value: 10 }
    c.set(40)
    val a = c.get()          // 40

    val flag = Cell { value: true }
    val on = flag.get()      // bool instance

    if on {
        return a + 2         // 42
    }
    return 0
}
"#;
    let exit = test
        .compile_and_run("gstruct_cell.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn bare_generic_struct_without_arguments_is_rejected() {
    let test = CompileTest::new();
    // A generic struct is usable only with type arguments; the bare name is an error.
    let source = r#"
struct Box<T> {
    v: T
}

func take(b: Box) -> i32 {
    0
}

func main() -> i32 {
    return 0
}
"#;
    let path = test.write_source("gstruct_bare.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a generic struct used without type arguments must be rejected"
    );
}

#[test]
fn non_copy_struct_type_argument_is_rejected() {
    let test = CompileTest::new();
    // Struct type arguments are restricted to Copy types this phase; `string` is not.
    let source = r#"
struct Box<T> {
    v: T
}

func main() -> i32 {
    val b = Box { v: "hi" }
    return 0
}
"#;
    let path = test.write_source("gstruct_non_copy.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a non-Copy struct type argument must be rejected"
    );
}
