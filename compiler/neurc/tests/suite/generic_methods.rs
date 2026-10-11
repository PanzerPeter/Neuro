// Generic methods: a method's own type parameters after its name, inferred from the
// arguments or supplied by a turbofish, on plain and generic types, enums and newtypes,
// and associated functions whose turbofish lists the type's parameters then their own.
use crate::compile_harness::CompileTest;

fn run(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

fn stdout_of(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source(name, source))
        .unwrap_or_else(|e| panic!("{name} should compile: {e}"));
    let out = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

fn rejected(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    test.compile(&path)
        .expect_err("the program must be rejected")
}

#[test]
fn a_method_infers_its_own_parameter_or_takes_a_turbofish() {
    let source = r#"
struct Picker {
    first: bool
}

impl Picker {
    func pick<U>(&self, a: U, b: U) -> U {
        if self.first { a } else { b }
    }
}

func main() -> i32 {
    val p = Picker { first: true }
    val small = p.pick(40, 1)
    val wide = p.pick::<i64>(2i64, 9i64)
    val flag = p.pick(true, false)
    if flag { small + wide as i32 } else { 0 }
}
"#;
    assert_eq!(run("pick.nr", source), 42);
}

#[test]
fn one_method_instance_per_argument_type_with_a_string_among_them() {
    let source = r#"
struct Tagger {
    n: i32
}

impl Tagger {
    func pass<U>(&self, u: U) -> U { u }
}

func main() -> i32 {
    val t = Tagger { n: 3 }
    println(t.pass("text"))
    println("{t.pass(2.5)}")
    t.pass(t.n)
}
"#;
    assert_eq!(stdout_of("tagger.nr", source), "text\n2.5\n");
    assert_eq!(run("tagger_exit.nr", source), 3);
}

#[test]
fn a_mut_self_generic_method_writes_through_to_the_receiver() {
    let source = r#"
struct Counter {
    hits: i32
}

impl Counter {
    func record<U>(&mut self, u: U) -> U {
        self.hits = self.hits + 1
        u
    }
}

func main() -> i32 {
    mut c = Counter { hits: 0 }
    val a = c.record(10)
    val b = c.record(true)
    if b { c.hits + a } else { 0 }
}
"#;
    assert_eq!(run("record.nr", source), 12);
}

#[test]
fn a_generic_impl_method_adds_its_own_parameter_to_the_types() {
    let source = r#"
struct Pair<A, B> {
    a: A,
    b: B
}

struct Cell<T> {
    value: T
}

impl<T> Cell<T> {
    func with<U>(self, u: U) -> Pair<T, U> {
        Pair { a: self.value, b: u }
    }
    func then<U>(self, u: U) -> Pair<T, U> {
        self.with::<U>(u)
    }
}

func main() -> i32 {
    val p = Cell { value: 40 }.then(2u8)
    val q = Cell { value: true }.with(1)
    if q.a { p.a + p.b as i32 } else { q.b }
}
"#;
    assert_eq!(run("cell_with.nr", source), 42);
}

#[test]
fn a_bound_on_the_methods_parameter_dispatches_in_its_body() {
    let source = r#"
trait Weight {
    func weight(&self) -> i32
}

struct Stone {
    kg: i32
}

impl Weight for Stone {
    func weight(&self) -> i32 { self.kg }
}

struct Scale {
    offset: i32
}

impl Scale {
    func read<U: Weight>(&self, item: &U) -> i32 {
        item.weight() + self.offset
    }
}

func main() -> i32 {
    val s = Scale { offset: 2 }
    val rock = Stone { kg: 40 }
    s.read(&rock)
}
"#;
    assert_eq!(run("scale.nr", source), 42);
}

#[test]
fn enums_newtypes_and_const_parameters_take_generic_methods() {
    let source = r#"
enum Light {
    Off,
    On(i32)
}

impl Light {
    func or<U>(&self, lit: U, dark: U) -> U {
        match self {
            Light::On(_) => lit,
            Light::Off => dark
        }
    }
}

newtype Meters = i32

impl Meters {
    func echo<U>(&self, u: U) -> U { u }
}

struct Sum {
    base: i32
}

impl Sum {
    func over<const N: u32>(&self, xs: [i32; N]) -> i32 where N > 1 {
        mut t = self.base
        for x in xs {
            t = t + x
        }
        t + N as i32
    }
}

func main() -> i32 {
    val l = Light::On(1)
    val m = Meters(5)
    val s = Sum { base: 20 }
    l.or(m.echo(10), 0) + s.over([3, 4, 2])
}
"#;
    assert_eq!(run("variety.nr", source), 42);
}

#[test]
fn associated_functions_take_their_own_parameters() {
    let source = r#"
struct Pair<A, B> {
    a: A,
    b: B
}

struct Unit {
    x: i32
}

impl Unit {
    func nothing<U>() -> Option<U> { Option::None }
}

struct Cell<T> {
    value: T
}

impl<T> Cell<T> {
    func make<U>(value: T, other: U) -> Pair<T, U> {
        Pair { a: value, b: other }
    }
    func empty<U>() -> Option<U> { Option::None }
}

func main() -> i32 {
    val none: Option<i64> = Unit::nothing::<i64>()
    val pair = Cell::make(40, 2i64)
    val other: Option<f64> = Cell::empty::<i32, f64>()
    match none {
        Option::Some(_) => 0,
        Option::None => pair.a + pair.b as i32
    }
}
"#;
    assert_eq!(run("assoc.nr", source), 42);
}

#[test]
fn a_generic_body_passes_its_parameter_through_a_method_turbofish() {
    let source = r#"
struct Relay {
    on: bool
}

impl Relay {
    func send<U>(&self, u: U, fallback: U) -> U {
        if self.on { u } else { fallback }
    }
}

func forward<T>(r: &Relay, x: T, y: T) -> T {
    r.send::<T>(x, y)
}

func main() -> i32 {
    val r = Relay { on: false }
    forward(&r, 1, 42)
}
"#;
    assert_eq!(run("relay.nr", source), 42);
}

#[test]
fn a_method_parameter_left_uninferred_is_refused() {
    let err = rejected(
        "uninferred.nr",
        r#"
struct S { v: i32 }
impl S {
    func none<U>(&self) -> Option<U> { Option::None }
}
func main() -> i32 {
    val s = S { v: 1 }
    val o = s.none()
    0
}
"#,
    );
    assert!(
        err.contains("generic parameter 'U' cannot be inferred"),
        "{err}"
    );
}

#[test]
fn a_turbofish_on_a_plain_method_is_refused_by_name() {
    let err = rejected(
        "plain_turbofish.nr",
        r#"
struct S { v: i32 }
impl S {
    func get(&self) -> i32 { self.v }
}
func main() -> i32 {
    val s = S { v: 1 }
    s.get::<i32>()
}
"#,
    );
    assert!(err.contains("but 'get' declares 0"), "{err}");
}

#[test]
fn an_abstract_method_parameter_moves() {
    let err = rejected(
        "dup.nr",
        r#"
struct S { v: i32 }
impl S {
    func dup<U>(&self, u: U) -> U {
        val a = u
        val b = u
        b
    }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("use of moved value 'u'"), "{err}");
}

#[test]
fn trait_methods_and_trait_impls_may_not_be_generic() {
    let declared = rejected(
        "trait_decl.nr",
        r#"
trait Show {
    func show<U>(&self, u: U) -> i32
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        declared.contains("a trait method declares no type parameters"),
        "{declared}"
    );
    let implemented = rejected(
        "trait_impl.nr",
        r#"
trait Show {
    func show(&self) -> i32
}
struct S { v: i32 }
impl Show for S {
    func show<U>(&self) -> i32 { self.v }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        implemented.contains("only an inherent `impl` method may be generic"),
        "{implemented}"
    );
}

#[test]
fn a_method_parameter_named_like_the_impls_is_refused() {
    let err = rejected(
        "shadow.nr",
        r#"
struct W<T> { v: T }
impl<T> W<T> {
    func f<T>(&self, t: T) -> i32 { 0 }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("method type parameter 'T' reuses a name"),
        "{err}"
    );
}

#[test]
fn a_method_calling_itself_with_a_growing_type_hits_the_depth_limit() {
    let err = rejected(
        "grow.nr",
        r#"
struct S { v: i32 }
impl S {
    func grow<U>(&self, u: U, n: i32) -> i32 {
        if n == 0 { 0 } else { self.grow((u, 1), n - 1) }
    }
}
func main() -> i32 {
    val s = S { v: 1 }
    s.grow(1, 3)
}
"#,
    );
    assert!(err.contains("nests generic instances more than"), "{err}");
}

#[test]
fn an_impl_trait_method_parameter_is_one_more_generic_parameter() {
    let source = r#"
trait Weight {
    func weight(&self) -> i32
}

struct Stone {
    kg: i32
}

impl Weight for Stone {
    func weight(&self) -> i32 { self.kg }
}

struct Scale {
    offset: i32
}

impl Scale {
    func read(&self, item: &impl Weight) -> i32 {
        item.weight() + self.offset
    }
}

func main() -> i32 {
    val s = Scale { offset: 2 }
    val rock = Stone { kg: 40 }
    s.read(&rock)
}
"#;
    assert_eq!(run("impl_trait_param.nr", source), 42);
}
