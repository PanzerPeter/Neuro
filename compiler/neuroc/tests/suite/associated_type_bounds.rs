// End-to-end `Trait<Assoc = T>` bounds: a bound that says what a trait's associated
// type is, so a generic body can call a method whose signature names it. Each program
// compiles to a native binary and runs; the exit code encodes the computed result.

use crate::compile_harness::CompileTest;

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
    // The three places a bound is written (the parameter list, a `where` clause, and
    // argument-position `impl Trait`) are one form, so all three must carry it.
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
fn a_parameter_named_only_in_a_bound_is_inferred_from_the_impl() {
    // `V` appears in no argument, only in `Src<Item = V>`: the impl of the argument's
    // type says what `Item` is, and that solves `V`. Covers a generic impl, a chain
    // through two bounds, a call passed through from an enclosing bound, and a method.
    let test = CompileTest::new();
    let source = r#"
trait Src {
    type Item
    func get(&self) -> Self::Item
}
struct C { v: u32 }
impl Src for C {
    type Item = u32
    func get(&self) -> u32 { self.v }
}
struct W<T> { t: T }
impl<T: Copy> Src for W<T> {
    type Item = T
    func get(&self) -> T { self.t }
}
struct Nest { c: C }
impl Src for Nest {
    type Item = C
    func get(&self) -> C { C { v: self.c.v * 10 } }
}
func hold<V, T: Src<Item = V>>(s: &T) -> V { s.get() }
func deep<U, V: Src<Item = U>, T: Src<Item = V>>(s: &T) -> U { s.get().get() }
func outer<T: Src<Item = u32>>(s: &T) -> u32 { hold(s) }
struct K { k: i32 }
impl K {
    func via<V, T: Src<Item = V>>(&self, s: &T) -> V { s.get() }
}
func main() -> i32 {
    val c = C { v: 7 }
    val w = W { t: 5i64 }
    val n = Nest { c: C { v: 2 } }
    val k = K { k: 0 }
    return hold(&c) as i32 + hold(&w) as i32 + deep(&n) as i32 + outer(&c) as i32 + k.via(&c) as i32
}
"#;
    let exit = test
        .compile_and_run("bound_infer.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 46);
}

#[test]
fn an_explicit_argument_contradicting_the_impl_does_not_compile() {
    let test = CompileTest::new();
    let source = r#"
trait Src {
    type Item
    func get(&self) -> Self::Item
}
struct C { v: u32 }
impl Src for C {
    type Item = u32
    func get(&self) -> u32 { self.v }
}
func hold<V, T: Src<Item = V>>(s: &T) -> V { s.get() }
func main() -> i32 {
    val c = C { v: 7 }
    val x = hold::<i64, C>(&c)
    0
}
"#;
    let path = test.write_source("bound_infer_mismatch.nr", source);
    let err = test
        .compile(&path)
        .expect_err("`V = i64` contradicts the impl's `Item = u32`");
    assert!(
        err.contains("Item") && err.contains("u32"),
        "the diagnostic should name the associated type and the impl's binding; got: {err}"
    );
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
