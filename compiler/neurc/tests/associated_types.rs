// End-to-end associated types: a trait declares `type Item`, each impl binds it, and
// `Self::Item` names the binding in signatures and bodies. Each program compiles to a
// native binary and runs; the exit code encodes the computed result.

mod common;
use common::CompileTest;

#[test]
fn an_impl_binds_the_associated_type_its_trait_declares() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n * 2 }
}

func main() -> i32 {
    val c = Counter { n: 21 }
    c.first()
}
"#;
    let exit = test
        .compile_and_run("assoc_binding.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn two_impls_bind_the_same_associated_type_differently() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }
struct Flag { on: bool }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

impl Source for Flag {
    type Item = bool

    func first(&self) -> Self::Item { self.on }
}

func main() -> i32 {
    val c = Counter { n: 5 }
    val f = Flag { on: true }
    if f.first() { c.first() * 3 } else { 0 }
}
"#;
    let exit = test
        .compile_and_run("assoc_two_impls.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn an_associated_type_carries_through_a_generic_enum() {
    // The canonical iterator signature: the associated type sits inside
    // `Option<...>`, so the binding has to reach a nested position, not only a
    // bare annotation.
    let test = CompileTest::new();
    let source = r#"
trait Iterator {
    type Item

    func next(&mut self) -> Option<Self::Item>
}

struct Countdown { remaining: i32 }

impl Iterator for Countdown {
    type Item = i32

    func next(&mut self) -> Option<Self::Item> {
        if self.remaining <= 0 {
            return None
        }
        self.remaining = self.remaining - 1
        Some(self.remaining)
    }
}

func main() -> i32 {
    mut c = Countdown { remaining: 4 }
    mut total = 0
    mut running = true
    while running {
        val step = c.next()
        match step {
            Some(v) => { total = total + v }
            None => { running = false }
        }
    }
    total
}
"#;
    let exit = test
        .compile_and_run("assoc_option.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}

#[test]
fn a_default_method_reaches_the_associated_type_of_each_impl() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
    func twice(&self) -> Self::Item { self.first() }
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n + 1 }
}

func main() -> i32 {
    val c = Counter { n: 9 }
    c.twice()
}
"#;
    let exit = test
        .compile_and_run("assoc_default_method.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 10);
}

#[test]
fn an_unbound_associated_type_does_not_compile() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    func first(&self) -> i32 { self.n }
}

func main() -> i32 { 0 }
"#;
    let path = test.write_source("assoc_unbound.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an impl that binds nothing must be rejected");
    assert!(
        err.contains("Item"),
        "the diagnostic should name the unbound associated type; got: {err}"
    );
}
