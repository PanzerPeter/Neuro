// End-to-end trait declarations: default methods, trait-impl dispatch, and
// trait-bounded generics. Each program compiles to a native binary and runs; the exit
// code encodes the computed result.

mod common;
use common::CompileTest;

#[test]
fn default_method_dispatches_to_required_method() {
    let test = CompileTest::new();
    let source = r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
}

func main() -> i32 {
    val w = Widget { id: 21 }
    w.doubled()
}
"#;
    let exit = test
        .compile_and_run("trait_default_method.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn overriding_a_default_method_wins() {
    let test = CompileTest::new();
    let source = r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
    func doubled(&self) -> i32 { self.id + 1 }
}

func main() -> i32 {
    val w = Widget { id: 9 }
    w.doubled()
}
"#;
    let exit = test
        .compile_and_run("trait_override.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn trait_bounded_generic_dispatches_after_monomorphization() {
    let test = CompileTest::new();
    let source = r#"
trait Shape { func area(&self) -> i32 }

@derive(Copy)
struct Square { side: i32 }

@derive(Copy)
struct Rect { w: i32, h: i32 }

impl Shape for Square { func area(&self) -> i32 { self.side * self.side } }
impl Shape for Rect { func area(&self) -> i32 { self.w * self.h } }

func area_of<T: Shape>(s: &T) -> i32 { s.area() }

func main() -> i32 {
    val sq = Square { side: 5 }
    val r = Rect { w: 3, h: 4 }
    area_of(&sq) + area_of(&r)
}
"#;
    let exit = test
        .compile_and_run("trait_bound_generic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 37);
}

#[test]
fn one_type_implements_two_traits() {
    let test = CompileTest::new();
    let source = r#"
trait Named { func tag(&self) -> i32 }
trait Sized2 { func size(&self) -> i32 }

struct Item { tag_id: i32, bytes: i32 }

impl Named for Item { func tag(&self) -> i32 { self.tag_id } }
impl Sized2 for Item { func size(&self) -> i32 { self.bytes } }

func main() -> i32 {
    val it = Item { tag_id: 7, bytes: 100 }
    it.tag() + it.size()
}
"#;
    let exit = test
        .compile_and_run("trait_two_traits.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 107);
}
