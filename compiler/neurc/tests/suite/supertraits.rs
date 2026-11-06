// Supertraits: `trait C: A + B`, the impls it requires, a bound reaching their methods, and
// a trait object's vtable carrying them.
use crate::compile_harness::CompileTest;

fn run(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

fn rejected(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let path = test.write_source(name, source);
    test.compile(&path)
        .expect_err("the program must be rejected")
}

const CHAIN: &str = r#"
trait Named {
    func name(&self) -> string
}

trait Sized2: Named {
    func size(&self) -> i32
    func label(&self) -> string { "{self.name()}:{self.size()}" }
}

trait Boxed: Sized2 {
    func depth(&self) -> i32
}

struct Crate { n: i32 }

impl Boxed for Crate {
    func depth(&self) -> i32 { 3 }
}

impl Sized2 for Crate {
    func size(&self) -> i32 { self.n }
}

impl Named for Crate {
    func name(&self) -> string { "crate" }
}
"#;

#[test]
fn a_bound_reaches_every_supertrait_and_a_default_body_calls_one() {
    let source = format!(
        r#"{CHAIN}
func describe<T: Boxed>(t: &T) -> i32 {{
    if t.label() == "crate:4" {{ t.size() + t.depth() + t.name().len() as i32 }} else {{ 0 }}
}}

func main() -> i32 {{
    val c = Crate {{ n: 4 }}
    describe(&c)
}}
"#
    );
    assert_eq!(run("chain.nr", &source), 12);
}

#[test]
fn a_subtrait_bound_satisfies_a_supertrait_bound() {
    let source = format!(
        r#"{CHAIN}
func size_of<T: Sized2>(t: &T) -> i32 {{ t.size() }}
func name_len<T: Named>(t: &T) -> i32 {{ t.name().len() as i32 }}
func both<T: Boxed>(t: &T) -> i32 {{ size_of(t) + name_len(t) }}
func opaque(t: &impl Boxed) -> i32 {{ t.size() }}

func main() -> i32 {{
    val c = Crate {{ n: 4 }}
    both(&c) + opaque(&c)
}}
"#
    );
    assert_eq!(run("forwarded.nr", &source), 13);
}

#[test]
fn an_impl_without_a_supertrait_is_refused_by_name() {
    let source = r#"
trait A { func a(&self) -> i32 }
trait B: A {}
trait C: B {}
struct S { v: i32 }
impl C for S {}
impl B for S {}
func main() -> i32 { 0 }
"#;
    let err = rejected("missing.nr", source);
    assert!(
        err.contains("`impl B for S` requires `impl A for S`"),
        "{err}"
    );
    assert!(!err.contains("`impl C for S`"), "B is implemented: {err}");
}

#[test]
fn an_enum_and_a_newtype_implement_a_subtrait() {
    let source = r#"
trait Base { func base(&self) -> i32 }
trait Top: Base { func top(&self) -> i32 { self.base() + 1 } }

enum Mode { Fast, Slow }
newtype Meters = i32

impl Base for Mode {
    func base(&self) -> i32 {
        match self {
            Mode::Fast => 10,
            Mode::Slow => 20
        }
    }
}
impl Top for Mode {}
impl Base for Meters {
    func base(&self) -> i32 { self.0 }
}
impl Top for Meters {}

func read<T: Top>(t: &T) -> i32 { t.base() + t.top() }

func main() -> i32 {
    val mode = Mode::Fast
    val meters = Meters(5)
    read(&mode) + read(&meters)
}
"#;
    assert_eq!(run("nominal.nr", source), 32);
}

#[test]
fn an_unknown_supertrait_is_refused() {
    let err = rejected("unknown.nr", "trait C: Nope {}\nfunc main() -> i32 { 0 }\n");
    assert!(err.contains("Nope"), "{err}");
}

#[test]
fn a_supertrait_cycle_is_refused() {
    let err = rejected(
        "cycle.nr",
        r#"
trait A: B { func a(&self) -> i32 }
trait B: C {}
trait C: A {}
trait D: D {}
trait Off: A {}
struct S { v: i32 }
impl Off for S {}
func take<T: Off>(t: T) -> i32 { t.a() }
func main() -> i32 { 0 }
"#,
    );
    for name in ["A", "B", "C", "D"] {
        assert!(
            err.contains(&format!("trait '{name}' is its own supertrait")),
            "{err}"
        );
    }
    assert!(!err.contains("trait 'Off' is its own"), "{err}");
}

#[test]
fn the_list_spans_lines_and_a_trailing_plus_is_a_parse_error() {
    let source = r#"
trait A { func a(&self) -> i32 }
trait B { func b(&self) -> i32 }
trait C:
    A +
    B
{
    func c(&self) -> i32 { self.a() + self.b() }
}
struct S { v: i32 }
impl A for S { func a(&self) -> i32 { 1 } }
impl B for S { func b(&self) -> i32 { 2 } }
impl C for S {}
func main() -> i32 { S { v: 0 }.c() }
"#;
    assert_eq!(run("lines.nr", source), 3);
    rejected(
        "plus.nr",
        "trait A {}\ntrait C: A + {}\nfunc main() -> i32 { 0 }\n",
    );
}

const SOURCE_TRAIT: &str = r#"
trait Source {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

trait Numbers: Source<Item = u32> {
    func twice(&mut self) -> u32 {
        match self.next() {
            Some(v) => v * 2,
            None => 0
        }
    }
}

struct Count { at: u32 }

impl Source for Count {
    type Item = u32
    func next(&mut self) -> Option<u32> {
        self.at = self.at + 1
        Some(self.at)
    }
}
impl Numbers for Count {}
"#;

// The binding on the supertrait is what types `next` through a bare `T: Numbers`.
#[test]
fn a_supertrait_binding_constrains_the_bound_and_the_impl() {
    let source = format!(
        r#"{SOURCE_TRAIT}
func head<T: Numbers>(t: &mut T) -> u32 {{
    match t.next() {{
        Some(v) => v + t.twice(),
        None => 0
    }}
}}

func plain<T: Source<Item = u32>>(t: &mut T) -> u32 {{
    match t.next() {{
        Some(v) => v,
        None => 0
    }}
}}
func forward<T: Numbers>(t: &mut T) -> u32 {{ plain(t) }}

func main() -> i32 {{
    mut c = Count {{ at: 0 }}
    val first = head(&mut c)
    (first + forward(&mut c)) as i32
}}
"#
    );
    assert_eq!(run("binding.nr", &source), 8);
}

#[test]
fn an_impl_whose_binding_differs_is_refused() {
    let source = r#"
trait Source {
    type Item
    func next(&mut self) -> Option<Self::Item>
}
trait Numbers: Source<Item = u32> {}
struct Text { at: i32 }
impl Source for Text {
    type Item = string
    func next(&mut self) -> Option<string> { None }
}
impl Numbers for Text {}
func main() -> i32 { 0 }
"#;
    let err = rejected("binding_differs.nr", source);
    assert!(err.contains("Item"), "{err}");
}

#[test]
fn a_binding_may_name_self_and_an_alias() {
    let source = r#"
type Whole = i32

trait Holder {
    type Held
    func held(&self) -> Self::Held
}

trait SelfHolder: Holder<Item2 = Self> {}
func main() -> i32 { 0 }
"#;
    let err = rejected("unknown_assoc.nr", source);
    assert!(err.contains("Item2"), "{err}");

    let source = r#"
type Whole = i32

trait Holder {
    type Held
    func held(&self) -> Self::Held
}

trait WholeHolder: Holder<Held = Whole> {}
trait Twin: Holder<Held = Self> {}

@derive(Copy)
struct P { v: i32 }

struct W { v: i32 }

impl Holder for W {
    type Held = i32
    func held(&self) -> i32 { self.v }
}
impl WholeHolder for W {}

impl Holder for P {
    type Held = P
    func held(&self) -> P { P { v: self.v } }
}
impl Twin for P {}

func read<T: WholeHolder>(t: &T) -> i32 { t.held() + 1 }
func again<T: Twin>(t: &T) -> T { t.held() }

func main() -> i32 {
    val w = W { v: 40 }
    val p = P { v: 1 }
    read(&w) + again(&p).v
}
"#;
    assert_eq!(run("binding_self.nr", source), 42);
}

#[test]
fn a_binding_may_name_the_subtraits_own_associated_type() {
    let source = r#"
trait Source {
    type Item
    func next(&mut self) -> Self::Item
}
trait Mapped: Source<Item = Self::Out> {
    type Out
}
struct One { v: i32 }
impl Source for One {
    type Item = i32
    func next(&mut self) -> i32 { self.v }
}
impl Mapped for One {
    type Out = i32
}
func read<T: Mapped<Out = i32>>(t: &mut T) -> i32 { t.next() + 2 }
func main() -> i32 {
    mut o = One { v: 40 }
    read(&mut o)
}
"#;
    assert_eq!(run("binding_own.nr", source), 42);

    let differs = r#"
trait Source {
    type Item
    func next(&mut self) -> Self::Item
}
trait Mapped: Source<Item = Self::Out> {
    type Out
}
struct One { v: i32 }
impl Source for One {
    type Item = i32
    func next(&mut self) -> i32 { self.v }
}
impl Mapped for One {
    type Out = string
}
func main() -> i32 { 0 }
"#;
    let err = rejected("binding_own_differs.nr", differs);
    assert!(err.contains("Item"), "{err}");
}

#[test]
fn a_subtrait_does_not_inherit_associated_types() {
    let prefix = r#"
trait Source {
    type Item
    func next(&mut self) -> Self::Item
}
"#;
    let err = rejected(
        "inherit_sig.nr",
        &format!(
            "{prefix}trait Sub: Source {{ func f(&self) -> Self::Item }}\nfunc main() -> i32 {{ 0 }}\n"
        ),
    );
    assert!(err.contains("Item"), "{err}");
    let err = rejected(
        "inherit_bound.nr",
        &format!(
            "{prefix}trait Sub: Source {{}}\nfunc f<T: Sub<Item = i32>>(t: &T) -> i32 {{ 0 }}\nfunc main() -> i32 {{ 0 }}\n"
        ),
    );
    assert!(err.contains("Item"), "{err}");
}

#[test]
fn drop_is_a_supertrait_and_stays_out_of_the_table() {
    let source = r#"
trait Held: Drop {
    func v(&self) -> i32
}
struct S { n: i32 }
impl Drop for S {
    func drop(&mut self) { println("gone") }
}
impl Held for S {
    func v(&self) -> i32 { self.n }
}
func read(h: &dyn Held) -> i32 { h.v() }
func main() -> i32 {
    val s = S { n: 42 }
    read(&s)
}
"#;
    assert_eq!(run("drop_super.nr", source), 42);

    let missing = r#"
trait Held: Drop {}
struct S { n: i32 }
impl Held for S {}
func main() -> i32 { 0 }
"#;
    let err = rejected("drop_missing.nr", missing);
    assert!(err.contains("requires `impl Drop for S`"), "{err}");
}

#[test]
fn a_subtrait_redeclaring_a_method_cannot_be_implemented_beside_it() {
    let source = r#"
trait A { func name(&self) -> i32 }
trait B: A { func name(&self) -> i32 }
struct S { v: i32 }
impl A for S { func name(&self) -> i32 { 1 } }
impl B for S { func name(&self) -> i32 { 2 } }
func main() -> i32 { 0 }
"#;
    let err = rejected("redeclared.nr", source);
    assert!(err.contains("already defined"), "{err}");
}

// A subtrait's own bound may bring a binding the supertrait list leaves open.
#[test]
fn a_binding_written_beside_the_subtrait_reaches_the_supertrait() {
    let source = r#"
trait Source {
    type Item
    func next(&mut self) -> Self::Item
}
trait Named: Source {
    func tag(&self) -> i32
}
struct One { v: i32 }
impl Source for One {
    type Item = i32
    func next(&mut self) -> i32 { self.v }
}
impl Named for One {
    func tag(&self) -> i32 { 2 }
}
func read<T: Named + Source<Item = i32>>(t: &mut T) -> i32 { t.next() + t.tag() }
func bare<T: Named>(t: &mut T) -> i32 { t.tag() }
func main() -> i32 {
    mut o = One { v: 40 }
    read(&mut o) + bare(&mut o) - 2
}
"#;
    assert_eq!(run("beside.nr", source), 42);

    let unconstrained = r#"
trait Source {
    type Item
    func next(&mut self) -> Self::Item
}
trait Named: Source {}
func bad<T: Named>(t: &mut T) -> i32 { t.next() }
func main() -> i32 { 0 }
"#;
    let err = rejected("unconstrained.nr", unconstrained);
    assert!(err.contains("Item"), "{err}");
}

#[test]
fn a_copy_supertrait_makes_the_parameter_copy() {
    let source = r#"
trait Mark: Copy {
    func v(&self) -> i32
}

@derive(Copy)
struct P { n: i32 }

impl Mark for P {
    func v(&self) -> i32 { self.n }
}

func twice<T: Mark>(t: T) -> i32 {
    val a = t
    val b = t
    val c = t.clone()
    a.v() + b.v() + c.v()
}

func main() -> i32 { twice(P { n: 14 }) }
"#;
    assert_eq!(run("copy_super.nr", source), 42);
}

#[test]
fn a_clone_supertrait_needs_a_clone_and_a_derive_provides_it() {
    let source = r#"
trait Dup: Clone {
    func v(&self) -> i32
    func both(&self) -> i32 { self.clone().v() + self.v() }
}

@derive(Clone)
struct P { n: i32, m: i32 }

impl Dup for P {
    func v(&self) -> i32 { self.n + self.m }
}

func make<T: Dup>(t: &T) -> T { t.clone() }

func main() -> i32 {
    val p = P { n: 19, m: 2 }
    make(&p).both()
}
"#;
    assert_eq!(run("clone_super.nr", source), 42);

    let missing = r#"
trait Dup: Clone {}
struct P { n: i32 }
impl Dup for P {}
func main() -> i32 { 0 }
"#;
    let err = rejected("clone_missing.nr", missing);
    assert!(err.contains("requires `impl Clone for P`"), "{err}");
}

#[test]
fn a_comparison_supertrait_gives_the_parameter_its_operators() {
    let source = r#"
trait Ranked: Comparable {
    func score(&self) -> i32
}

@derive(Copy, PartialEq)
struct P { n: i32 }

impl Comparable for P {
    func lt(&self, rhs: &P) -> bool { self.n < rhs.n }
}
impl Ranked for P {
    func score(&self) -> i32 { self.n }
}

trait Same: PartialEq {
    func same(&self, rhs: &Self) -> bool { self.eq(rhs) }
}
impl Same for P {}

func best<T: Ranked>(a: T, b: T) -> i32 {
    if a == b || a.eq(&b) { a.score() } else { b.score() }
}

func alike<T: Same>(a: &T, b: &T) -> bool { a.same(b) }

func main() -> i32 {
    val x = P { n: 2 }
    val y = P { n: 40 }
    best(x, y) + (if alike(&x, &x) { 2 } else { 0 })
}
"#;
    assert_eq!(run("comparison_super.nr", source), 42);
}

#[test]
fn an_operator_supertrait_requires_its_impl() {
    let source = r#"
trait Sum: Add {}

@derive(Copy)
struct P { n: i32 }

impl Sum for P {}
func main() -> i32 { 0 }
"#;
    let err = rejected("operator_super.nr", source);
    assert!(err.contains("requires `impl Add for P`"), "{err}");
}

#[test]
fn copy_requires_clone_as_written_in_the_prelude() {
    let source = r#"
struct P { n: i32 }
impl Copy for P {}
func main() -> i32 { 0 }
"#;
    let err = rejected("copy_clone.nr", source);
    assert!(err.contains("requires `impl Clone for P`"), "{err}");

    let source = r#"
struct P { n: i32 }
impl Clone for P {
    func clone(&self) -> P { P { n: self.n + 1 } }
}
impl Copy for P {}
func again<T: Copy>(t: &T) -> T { t.clone() }
func main() -> i32 { again(&P { n: 41 }).n }
"#;
    assert_eq!(run("copy_ok.nr", source), 42);
}

#[test]
fn a_by_value_supertrait_method_moves_through_the_bound() {
    let source = r#"
trait Eat { func eat(self) -> i32 }
trait Hungry: Eat {}
struct S { s: string }
impl Eat for S {
    func eat(self) -> i32 { self.s.len() as i32 }
}
impl Hungry for S {}
func twice<T: Hungry>(t: T) -> i32 { t.eat() + t.eat() }
func main() -> i32 { 0 }
"#;
    let err = rejected("moved.nr", source);
    assert!(err.contains("moved"), "{err}");
}

#[test]
fn a_trait_object_calls_supertrait_methods() {
    let source = r#"
trait Named {
    func name(&self) -> string
    func bump(&mut self)
}

trait Shape: Named {
    func area(&self) -> i32
}

struct Square { side: i32 }
struct Line { len: i32, hits: i32 }

impl Named for Square {
    func name(&self) -> string { "sq" }
    func bump(&mut self) { self.side = self.side + 1 }
}
impl Shape for Square {
    func area(&self) -> i32 { self.side * self.side }
}
impl Shape for Line {
    func area(&self) -> i32 { self.len + self.hits }
}
impl Named for Line {
    func name(&self) -> string { "line" }
    func bump(&mut self) { self.hits = self.hits + 1 }
}

func read(s: &dyn Shape) -> i32 { s.area() + s.name().len() as i32 }

func poke(s: &mut dyn Shape) {
    s.bump()
    s.bump()
}

func main() -> i32 {
    mut sq = Square { side: 3 }
    mut line = Line { len: 4, hits: 0 }
    poke(&mut sq)
    poke(&mut line)
    read(&sq) + read(&line)
}
"#;
    assert_eq!(run("dyn_super.nr", source), 37);
}

#[test]
fn a_diamond_fills_one_slot_per_method() {
    let source = r#"
trait Root { func root(&self) -> i32 }
trait L: Root { func l(&self) -> i32 }
trait R: Root { func r(&self) -> i32 }
trait Both: L + R { func own(&self) -> i32 }
struct S { v: i32 }
impl Root for S { func root(&self) -> i32 { self.v } }
impl L for S { func l(&self) -> i32 { 2 } }
impl R for S { func r(&self) -> i32 { 3 } }
impl Both for S { func own(&self) -> i32 { 4 } }
func all(b: &dyn Both) -> i32 { b.root() * 10 + b.l() + b.r() + b.own() }
func top<T: Both>(t: &T) -> i32 { t.root() }
func main() -> i32 {
    val s = S { v: 3 }
    all(&s) + top(&s) + 0
}
"#;
    assert_eq!(run("diamond.nr", source), 42);
}

// A derive has no method until a coercion asks for one; the object still fills its slot.
#[test]
fn a_derived_supertrait_fills_its_vtable_slot() {
    let source = r#"
trait Keyed: Hashable + Debug {
    func id(&self) -> i32
}

@derive(Hashable, Debug)
struct K { v: i32 }

impl Keyed for K {
    func id(&self) -> i32 { self.v }
}

func same(k: &dyn Keyed, hash: u64) -> bool {
    mut out = StringBuilder::new()
    k.fmt_debug(&mut out)
    k.hash() == hash && out.to_string().len() == 10
}

func main() -> i32 {
    val k = K { v: 7 }
    if same(&k, k.hash()) { k.id() } else { 0 }
}
"#;
    assert_eq!(run("dyn_derived.nr", source), 7);
}

// No coercion exists here, so the derived slot stays empty and no table may be asked for.
#[test]
fn an_impl_never_coerced_needs_no_derived_method() {
    let source = r#"
trait Keyed: Hashable {
    func id(&self) -> i32
}

@derive(Hashable)
struct K { v: i32 }

impl Keyed for K {
    func id(&self) -> i32 { self.v }
}

func main() -> i32 { K { v: 7 }.id() }
"#;
    assert_eq!(run("no_coercion.nr", source), 7);
}

#[test]
fn object_safety_reads_through_supertraits() {
    for (name, list, reason) in [
        ("clone.nr", "Clone", "method 'clone'"),
        ("copy.nr", "Copy", "supertrait 'Clone'"),
        ("eq.nr", "PartialEq", "operator trait"),
        ("by_value.nr", "Eat", "by value"),
    ] {
        let source = format!(
            r#"
trait Eat {{ func eat(self) -> i32 }}
trait Sub: {list} {{}}
func read(s: &dyn Sub) -> i32 {{ 0 }}
func main() -> i32 {{ 0 }}
"#
        );
        let err = rejected(name, &source);
        assert!(
            err.contains("object-safe") || err.contains("object safe"),
            "{err}"
        );
        assert!(err.contains(reason), "{err}");
    }
}

// The trait in the signature is registered before the one it reaches is.
#[test]
fn object_safety_does_not_depend_on_declaration_order() {
    let source = r#"
trait First {
    func use_it(&self, s: &dyn Second) -> i32
}
trait Second: Third {}
trait Third { func eat(self) -> i32 }
func main() -> i32 { 0 }
"#;
    let err = rejected("order.nr", source);
    assert!(err.contains("by value"), "{err}");
}

#[test]
fn a_subtrait_object_does_not_coerce_to_its_supertrait() {
    let source = r#"
trait A { func a(&self) -> i32 }
trait B: A {}
func up(a: &dyn A) -> i32 { a.a() }
func down(b: &dyn B) -> i32 { up(b) }
func main() -> i32 { 0 }
"#;
    let err = rejected("upcast.nr", source);
    assert!(err.contains("dyn A") && err.contains("dyn B"), "{err}");
}

const GENERIC_IMPLS: &str = r#"
trait Tag { func tag(&self) -> i32 }

trait Base { func base(&self) -> i32 }
trait Top: Base { func top(&self) -> i32 }

struct Wrap<T> { v: T }

struct Tagged { n: i32 }
impl Tag for Tagged {
    func tag(&self) -> i32 { self.n }
}

impl<T: Tag> Base for Wrap<T> {
    func base(&self) -> i32 { self.v.tag() }
}
impl<T> Top for Wrap<T> {
    func top(&self) -> i32 { 1 }
}

func read<T: Top>(t: &T) -> i32 { t.base() + t.top() }
"#;

#[test]
fn a_generic_impl_holds_per_instance() {
    let source = format!(
        r#"{GENERIC_IMPLS}
func view(t: &dyn Top) -> i32 {{ t.base() }}

func main() -> i32 {{
    val w = Wrap {{ v: Tagged {{ n: 20 }} }}
    read(&w) + view(&w) - 19
}}
"#
    );
    assert_eq!(run("per_instance.nr", &source), 22);
}

#[test]
fn an_instance_short_of_a_supertrait_does_not_meet_the_bound() {
    let source = format!(
        r#"{GENERIC_IMPLS}
func main() -> i32 {{
    val w = Wrap {{ v: 1 }}
    read(&w)
}}
"#
    );
    let err = rejected("short_instance.nr", &source);
    assert!(err.contains("Top"), "{err}");

    let source = format!(
        r#"{GENERIC_IMPLS}
func view(t: &dyn Top) -> i32 {{ 0 }}
func main() -> i32 {{
    val w = Wrap {{ v: 1 }}
    view(&w)
}}
"#
    );
    let err = rejected("short_object.nr", &source);
    assert!(err.contains("dyn Top"), "{err}");
}

// A derive on a template holds per instance, so a `Copy` promise must not outlive it.
#[test]
fn an_instance_that_is_not_copy_does_not_meet_a_copy_subtrait() {
    let source = r#"
trait Mark: Copy {}

@derive(Copy)
struct Cell<T> { v: T }

impl<T> Mark for Cell<T> {}

func twice<T: Mark>(t: T) -> (T, T) { (t, t) }

func main() -> i32 {
    val pair = twice(Cell { v: 1 })
    val c = Cell { v: "text" }
    val other = twice(c)
    pair.0.v
}
"#;
    let err = rejected("copy_instance.nr", source);
    assert!(err.contains("Mark"), "{err}");
}

#[test]
fn a_string_argument_through_a_supertrait_slot_is_released_once() {
    let source = r#"
trait Sink { func eat(&self, s: string) -> i32 }
trait Log: Sink {}

struct Keep { n: i32 }
struct Drop2 { n: i32 }

impl Sink for Keep {
    func eat(&self, s: string) -> i32 { s.len() as i32 + self.n }
}
impl Log for Keep {}

impl Sink for Drop2 {
    func eat(&self, s: string) -> i32 {
        val owned = s
        owned.len() as i32
    }
}
impl Log for Drop2 {}

func feed(l: &dyn Log) -> i32 {
    mut total = 0
    for i in 0..3 {
        total += l.eat("n{i}")
    }
    total
}

func main() -> i32 {
    val keep = Keep { n: 1 }
    val other = Drop2 { n: 0 }
    feed(&keep) + feed(&other)
}
"#;
    assert_eq!(run("dyn_string.nr", source), 15);
}

#[test]
fn an_imported_trait_renamed_reaches_the_supertrait_list() {
    let test = CompileTest::new();
    test.write_source(
        "lib.nr",
        r#"
export trait Base {
    func base(&self) -> i32
}
"#,
    );
    let main = test.write_source(
        "main.nr",
        r#"
import lib::{Base as Root}

trait Top: Root {
    func top(&self) -> i32 { self.base() + 1 }
}

struct S { v: i32 }
impl Root for S {
    func base(&self) -> i32 { self.v }
}
impl Top for S {}

func read<T: Top>(t: &T) -> i32 { t.top() + t.base() }

func main() -> i32 {
    val s = S { v: 20 }
    read(&s) + 1
}
"#,
    );
    let exe = test.compile(&main).expect("the program should compile");
    assert_eq!(test.run_executable(&exe).expect("it should run"), 42);
}
