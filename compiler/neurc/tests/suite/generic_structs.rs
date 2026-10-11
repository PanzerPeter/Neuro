// End-to-end tests for generic structs and generic inherent impls.
//
// A generic struct / impl is a template: each distinct set of concrete type arguments
// produces its own specialized struct and methods, monomorphized before codegen at
// zero runtime cost. These tests drive the whole pipeline
// (parse → type-check → HIR lowering → LLVM → native binary) and assert on the exit code.
use crate::compile_harness::CompileTest;

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
    func get(self) -> T {
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
fn regression_generic_struct_literal_takes_its_instance_from_context() {
    let test = CompileTest::new();
    // An annotation or a return type names the instance, so each literal field is typed
    // by it, the way `Option::Some(5)` is under `Option<i64>`. Every form below was
    // refused as `W<i32>`, or as an `i32` literal out of range, before the fix.
    let source = r#"
struct W<A, B> { a: A, b: B }

func tail(flag: bool) -> W<i64, u8> {
    if flag { if true { W { a: 7000000000, b: 200 } } else { W { a: 0, b: 0 } } } else { W { a: 1, b: 1 } }
}

func early() -> W<i64, u8> {
    return W { a: 7000000000, b: 200 }
}

func main() -> i32 {
    val annotated: W<i64, u8> = W { a: 7000000000, b: 200 }
    val nested: W<W<i64, u8>, u8> = W { a: W { a: 7000000000, b: 200 }, b: 1 }
    val arm: W<i64, u8> = match 1 { 1 => W { a: 7000000000, b: 200 }, _ => W { a: 0, b: 0 } }
    if annotated.a != 7000000000 || annotated.b != 200 { return 1 }
    if tail(true).a != 7000000000 || tail(true).b != 200 { return 2 }
    if early().a != 7000000000 { return 3 }
    if nested.a.a != 7000000000 || nested.a.b != 200 { return 4 }
    if arm.a != 7000000000 { return 5 }
    val inferred = W { a: 5, b: 2.5 }
    val default_i32: i32 = inferred.a
    return default_i32 + 37     // 42
}
"#;
    let exit = test
        .compile_and_run("gstruct_context.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn generic_struct_literal_contradicting_its_annotation_is_rejected() {
    let test = CompileTest::new();
    // The context types literals only: a typed `i32` value still infers `W<i32>`.
    let source = r#"
struct W<A> { a: A }

func main() -> i32 {
    val v: i32 = 3
    val x: W<i64> = W { a: v }
    return 0
}
"#;
    let path = test.write_source("gstruct_context_mismatch.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an i32 field value under a W<i64> annotation must be rejected");
    assert!(err.contains("expected W<i64>, found W<i32>"), "{err}");
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
    let err = test
        .compile(&path)
        .expect_err("a generic struct used without type arguments must be rejected");
    assert!(
        err.contains("generic struct 'Box' requires type arguments"),
        "{err}"
    );
}

#[test]
fn non_copy_struct_type_argument_is_held_and_moved() {
    let test = CompileTest::new();
    let source = r#"
struct Box<T> {
    v: T
}

func unwrap(b: Box<string>) -> string {
    b.v
}

func main() -> i32 {
    val b = Box { v: "hi" }
    unwrap(b).len() as i32
}
"#;
    let exit = test
        .compile_and_run("gstruct_non_copy.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 2);

    let moved = r#"
struct Box<T> {
    v: T
}

func take(b: Box<string>) -> i32 { 0 }

func main() -> i32 {
    val b = Box { v: "hi" }
    val first = take(b)
    take(b)
}
"#;
    let path = test.write_source("gstruct_non_copy_move.nr", moved);
    assert!(
        test.compile(&path).is_err(),
        "a non-Copy type argument must make the instance move, not copy"
    );
}

#[test]
fn regression_bug_108_generic_impl_bounds_are_checked_at_the_call() {
    // `impl<T: Show> Cell<T>`: a call on a `Cell<i32>` is refused at the call, not left
    // to lowering. A where clause, a type parameter passed on without the bound, an
    // associated function, and a bounded trait impl are refused the same way.
    let test = CompileTest::new();
    for source in [
        r#"
trait Show {
    func show(&self) -> i32
}

struct Cell<T> {
    v: T,
}

impl<T: Show> Cell<T> {
    func get(&self) -> i32 {
        self.v.show()
    }
}

func main() -> i32 {
    val c = Cell { v: 3 }
    c.get()
}
"#,
        r#"
trait Show { func show(&self) -> i32 }
struct Cell<T> { v: T }
impl<T> Cell<T> where T: Show {
    func get(&self) -> i32 { self.v.show() }
    func make(v: T) -> Cell<T> { Cell { v } }
}
func through<U>(c: &Cell<U>) -> i32 { c.get() }
func main() -> i32 {
    val c = Cell::make(3)
    0
}
"#,
        r#"
trait Show { func show(&self) -> i32 }
struct Cell<T> { v: T }
impl<T: Show> Show for Cell<T> { func show(&self) -> i32 { self.v.show() } }
func use_show<S: Show>(s: &S) -> i32 { s.show() }
func main() -> i32 {
    val a = Cell { v: 3 }
    use_show(&a)
}
"#,
    ] {
        let err = test
            .check("bounds.nr", source)
            .expect_err("a type argument missing the impl's bound must be refused");
        assert!(
            err.contains("does not implement required trait 'Show'"),
            "{err}"
        );
        assert!(!err.contains("lowering"), "{err}");
    }
}

#[test]
fn regression_bug_108_an_instance_that_never_calls_a_bounded_method_compiles() {
    // An unbounded `Cell<i32>` is fine while nothing calls `get`; the bounded impl is
    // not emitted for it.
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "bounds_ok.nr",
            r#"
trait Show { func show(&self) -> i32 }
@derive(Copy, Clone)
struct N { v: i32 }
impl Show for N { func show(&self) -> i32 { self.v } }
struct Cell<T> { v: T }
impl<T: Show> Cell<T> {
    func get(&self) -> i32 { self.v.show() }
    func twice(&self) -> i32 { self.get() + self.get() }
    func make(v: T) -> Cell<T> { Cell { v } }
}
func via<U: Show>(c: &Cell<U>) -> i32 { c.get() }
func main() -> i32 {
    val c = Cell { v: N { v: 3 } }
    val d = Cell::make(N { v: 4 })
    val unused = Cell { v: 5 }
    c.get() + c.twice() + via(&d) + unused.v - 1
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 17);
    let exit = test
        .compile_and_run(
            "bounds_conformance.nr",
            r#"
trait Show { func show(&self) -> i32 }
@derive(Copy, Clone)
struct N { v: i32 }
impl Show for N { func show(&self) -> i32 { self.v } }
struct Cell<T> { v: T }
impl<T: Show> Show for Cell<T> { func show(&self) -> i32 { self.v.show() * 2 } }
enum Maybe<T> { Yes(T), No }
impl<T: Show> Maybe<T> {
    func get(self) -> i32 { match self { Maybe::Yes(x) => x.show(), Maybe::No => 0 } }
}
func use_show<S: Show>(s: &S) -> i32 { s.show() }
func main() -> i32 {
    val a = Cell { v: N { v: 2 } }
    val b = Cell { v: Cell { v: N { v: 1 } } }
    val m: Maybe<N> = Maybe::Yes(N { v: 6 })
    val unused: Maybe<i32> = Maybe::No
    use_show(&a) + use_show(&b) + m.get()
}
"#,
        )
        .expect("should compile and run");
    assert_eq!(exit, 14);
}
