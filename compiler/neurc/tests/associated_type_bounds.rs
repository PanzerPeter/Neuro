// End-to-end `Trait<Assoc = T>` bounds: a bound that says what a trait's associated
// type is, so a generic body can call a method whose signature names it. Each program
// compiles to a native binary and runs; the exit code encodes the computed result.

mod common;
use common::CompileTest;

#[test]
fn a_constrained_bound_dispatches_an_associated_signature() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func head<T: Source<Item = i32>>(src: &T) -> i32 {
    src.first() * 2
}

func main() -> i32 {
    val c = Counter { n: 21 }
    head(&c)
}
"#;
    let exit = test
        .compile_and_run("bound_assoc.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn every_bound_position_takes_the_constraint() {
    // The three places a bound is written — the parameter list, a `where` clause, and
    // argument-position `impl Trait` — are one form, so all three must carry it.
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func inline<T: Source<Item = i32>>(src: &T) -> i32 { src.first() }

func clause<T>(src: &T) -> i32 where T: Source<Item = i32> { src.first() }

func anonymous(src: &impl Source<Item = i32>) -> i32 { src.first() }

func main() -> i32 {
    val c = Counter { n: 5 }
    inline(&c) + clause(&c) + anonymous(&c)
}
"#;
    let exit = test
        .compile_and_run("bound_assoc_positions.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn a_constrained_bound_reaches_a_nested_associated_position() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func wrapped(&self) -> Option<Self::Item>
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func wrapped(&self) -> Option<Self::Item> { Some(self.n) }
}

func head<T: Source<Item = i32>>(src: &T) -> i32 {
    match src.wrapped() {
        Some(v) => v,
        None => 0
    }
}

func main() -> i32 {
    val c = Counter { n: 33 }
    head(&c)
}
"#;
    let exit = test
        .compile_and_run("bound_assoc_nested.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 33);
}

#[test]
fn a_return_position_constraint_states_what_the_caller_gets() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func make(seed: i32) -> impl Source<Item = i32> {
    Counter { n: seed + 1 }
}

func main() -> i32 {
    make(8).first()
}
"#;
    let exit = test
        .compile_and_run("bound_assoc_return.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn a_type_argument_binding_another_type_does_not_compile() {
    let test = CompileTest::new();
    let source = r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Tally { n: f64 }

impl Source for Tally {
    type Item = f64

    func first(&self) -> Self::Item { self.n }
}

func head<T: Source<Item = i32>>(src: &T) -> i32 { 0 }

func main() -> i32 {
    val t = Tally { n: 1.0 }
    head(&t)
}
"#;
    let path = test.write_source("bound_assoc_mismatch.nr", source);
    let err = test
        .compile(&path)
        .expect_err("an impl binding f64 must not satisfy an `Item = i32` bound");
    assert!(
        err.contains("Item") && err.contains("f64"),
        "the diagnostic should name the associated type and what the impl bound it to; got: {err}"
    );
}
