// End-to-end static and dynamic dispatch (§3.17).
//
// `impl Trait` is anonymous-generic sugar and is monomorphized away; `dyn Trait` is a
// runtime trait object dispatched through a vtable. Each program compiles to a native
// binary and runs; the exit code encodes the computed result.

mod common;
use common::CompileTest;

#[test]
fn impl_trait_argument_dispatches_statically() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

@derive(Copy, Clone)
struct Square { side: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}

func measure(s: &impl Shape) -> i32 { s.area() }

func main() -> i32 {
    val sq = Square { side: 6 }
    measure(&sq)
}
"#;
    let exit = test
        .compile_and_run("impl_trait_arg.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 36);
}

/// Each `impl Trait` parameter is its own anonymous type parameter, so one call may bind
/// two different concrete types — the defining difference from a single named `<T>`.
#[test]
fn two_impl_trait_parameters_bind_independently() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

@derive(Copy, Clone)
struct Square { side: i32 }
@derive(Copy, Clone)
struct Rect { w: i32, h: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}
impl Shape for Rect {
    func area(&self) -> i32 { self.w * self.h }
}

func total(a: &impl Shape, b: &impl Shape) -> i32 { a.area() + b.area() }

func main() -> i32 {
    val sq = Square { side: 3 }
    val rc = Rect { w: 4, h: 5 }
    total(&sq, &rc)
}
"#;
    let exit = test
        .compile_and_run("impl_trait_two_params.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 29);
}

/// The core of dynamic dispatch: one function, two concrete receiver types, selected at
/// runtime through the vtable.
#[test]
fn dyn_trait_dispatches_dynamically_across_types() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

@derive(Copy, Clone)
struct Square { side: i32 }
@derive(Copy, Clone)
struct Rect { w: i32, h: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}
impl Shape for Rect {
    func area(&self) -> i32 { self.w * self.h }
}

func measure(s: &dyn Shape) -> i32 { s.area() }

func main() -> i32 {
    val sq = Square { side: 3 }
    val rc = Rect { w: 4, h: 5 }
    measure(&sq) + measure(&rc)
}
"#;
    let exit = test
        .compile_and_run("dyn_trait_dispatch.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 29);
}

/// An implementor that omits a default method must still reach the inherited body
/// through its vtable slot, while an overriding implementor reaches its own.
#[test]
fn dyn_dispatch_honours_default_and_overridden_methods() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
    func sides(&self) -> i32 { 0 }
}

@derive(Copy, Clone)
struct Square { side: i32 }
@derive(Copy, Clone)
struct Blob { size: i32 }

impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
    func sides(&self) -> i32 { 4 }
}
impl Shape for Blob {
    func area(&self) -> i32 { self.size }
}

func describe(s: &dyn Shape) -> i32 { s.area() + s.sides() }

func main() -> i32 {
    val sq = Square { side: 2 }
    val bl = Blob { size: 10 }
    describe(&sq) + describe(&bl)
}
"#;
    let exit = test
        .compile_and_run("dyn_default_method.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 18);
}

/// `&mut dyn Trait` reaches a `&mut self` method and the mutation is visible to the
/// caller — the trait object forwards the receiver by pointer.
#[test]
fn mut_dyn_trait_mutates_through_the_vtable() {
    let test = CompileTest::new();
    let source = r#"
trait Counter {
    func bump(&mut self)
    func get(&self) -> i32
}

@derive(Copy, Clone)
struct Tally { n: i32 }

impl Counter for Tally {
    func bump(&mut self) { self.n = self.n + 5 }
    func get(&self) -> i32 { self.n }
}

func bump_twice(c: &mut dyn Counter) {
    c.bump()
    c.bump()
}

func main() -> i32 {
    mut t = Tally { n: 1 }
    bump_twice(&mut t)
    t.get()
}
"#;
    let exit = test
        .compile_and_run("dyn_mut_receiver.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 11);
}

/// A trait method taking arguments must forward them through the vtable thunk intact.
#[test]
fn dyn_dispatch_forwards_method_arguments() {
    let test = CompileTest::new();
    let source = r#"
trait Scaler {
    func scale(&self, factor: i32) -> i32
}

@derive(Copy, Clone)
struct Doubler { base: i32 }

impl Scaler for Doubler {
    func scale(&self, factor: i32) -> i32 { self.base * factor * 2 }
}

func apply(s: &dyn Scaler, f: i32) -> i32 { s.scale(f) }

func main() -> i32 {
    val d = Doubler { base: 3 }
    apply(&d, 7)
}
"#;
    let exit = test
        .compile_and_run("dyn_args.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

/// A trait object is unsized, so a bare `dyn Trait` has no representation and must be
/// rejected in favour of `&dyn Trait`.
#[test]
fn bare_dyn_trait_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

func measure(s: dyn Shape) -> i32 { s.area() }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("dyn_unsized.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "an unsized `dyn Trait` outside a reference must be rejected"
    );
}

/// Object safety: a vtable has a fixed layout, so a method with no `self` receiver
/// cannot be dispatched dynamically.
#[test]
fn trait_without_self_receiver_is_not_object_safe() {
    let test = CompileTest::new();
    let source = r#"
trait Maker {
    func build() -> i32
}

func use_it(m: &dyn Maker) -> i32 { 0 }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("dyn_not_object_safe.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a trait with a receiverless method must not be usable as `dyn`"
    );
}

/// Object safety: a method consuming `self` by value cannot go behind a trait object —
/// the spec's `Add`-style case (§3.17).
#[test]
fn trait_consuming_self_is_not_object_safe() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct P { v: i32 }

trait Consume {
    func take(self) -> i32
}

impl Consume for P {
    func take(self) -> i32 { self.v }
}

func use_it(c: &dyn Consume) -> i32 { 0 }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("dyn_owned_self.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a trait whose method consumes `self` must not be usable as `dyn`"
    );
}

/// A concrete type that does not implement the trait cannot be coerced into its
/// trait object.
#[test]
fn coercing_a_non_implementor_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

@derive(Copy, Clone)
struct Blob { n: i32 }

func measure(s: &dyn Shape) -> i32 { s.area() }

func main() -> i32 {
    val b = Blob { n: 1 }
    measure(&b)
}
"#;
    let path = test.write_source("dyn_non_implementor.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "a type with no matching impl must not coerce to the trait object"
    );
}

/// Return-position `impl Trait` resolves transparently to the body's concrete type, so
/// a body producing a type that does not implement the trait is rejected.
#[test]
fn impl_trait_return_requires_an_implementor() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

@derive(Copy, Clone)
struct Blob { n: i32 }

func make() -> impl Shape { Blob { n: 1 } }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("impl_return_bad.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "an `impl Trait` return whose concrete type lacks the impl must be rejected"
    );
}

/// `impl Trait` is anonymous-generic sugar for a *function* type parameter, so it has no
/// meaning in a field annotation and must be rejected there.
#[test]
fn impl_trait_outside_a_function_signature_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
trait Shape {
    func area(&self) -> i32
}

struct Holder { s: impl Shape }

func main() -> i32 { 0 }
"#;
    let path = test.write_source("impl_trait_in_field.nr", source);
    assert!(
        test.compile(&path).is_err(),
        "`impl Trait` in a struct field must be rejected"
    );
}
