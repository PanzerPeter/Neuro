// Generic trait methods: a trait method's own type parameters, declared, implemented up to
// renaming, carried by a default body, and called through a bound.
use crate::compile_harness::{CompileTest, compile_rejected, run};

#[test]
fn a_call_through_a_bound_infers_or_takes_a_turbofish() {
    let source = r#"
trait Echo {
    func echo<U>(&self, u: U) -> U
    func base(&self) -> i32
    func marked<U: Copy>(&self, u: U) -> i32 { self.base() + 1 }
}

struct Plain { v: i32 }
struct Loud { v: i32 }

impl Echo for Plain {
    func echo<V>(&self, u: V) -> V { u }
    func base(&self) -> i32 { self.v }
}

impl Echo for Loud {
    func echo<U>(&self, u: U) -> U { u }
    func base(&self) -> i32 { self.v }
    func marked<W: Copy>(&self, u: W) -> i32 { 100 }
}

func total<T: Echo>(t: &T) -> i32 {
    val small = t.echo(40)
    val wide = t.echo::<i64>(2i64)
    val text = t.echo("x")
    if text == "x" { small + wide as i32 + t.marked(1.5) } else { 0 }
}

func main() -> i32 {
    val plain = Plain { v: 1 }
    val loud = Loud { v: 2 }
    if total(&plain) == 44 && total(&loud) == 142 { plain.echo(40) + loud.echo::<i32>(2) } else { 0 }
}
"#;
    assert_eq!(run("through_bound.nr", source), 42);
}

// The caller's `U` and the method's `U` are two parameters; `Self` here is the caller's.
#[test]
fn a_caller_parameter_named_like_the_methods_stays_apart() {
    let source = r#"
trait Join {
    func join<U>(&self, other: &Self, u: U) -> U
    func id(&self) -> i32
}

@derive(Copy)
struct A { v: i32 }

impl Join for A {
    func join<U>(&self, other: &Self, u: U) -> U { u }
    func id(&self) -> i32 { self.v }
}

func go<U: Join>(a: U, b: U) -> i32 {
    val x = a.join(&b, 41)
    x + a.id()
}

func main() -> i32 { go(A { v: 1 }, A { v: 2 }) }
"#;
    assert_eq!(run("collision.nr", source), 42);
}

#[test]
fn a_generic_impl_and_associated_types_reach_the_method() {
    let source = r#"
trait Src {
    type Item
    func get(&self) -> Self::Item
    func both<V>(&self, v: V) -> (Self::Item, V)
}

struct Cell<T> { v: T }

impl<T: Copy> Src for Cell<T> {
    type Item = T
    func get(&self) -> T { self.v }
    func both<W>(&self, w: W) -> (T, W) { (self.v, w) }
}

func go<T: Src<Item = i32>>(s: &T) -> i32 {
    val p = s.both(true)
    if p.1 { p.0 + s.get() } else { 0 }
}

func keep<V, T: Src<Item = V>>(s: &T, seed: V) -> V {
    val p = s.both(seed)
    p.1
}

func main() -> i32 {
    val c = Cell { v: 20 }
    go(&c) + keep(&c, 2)
}
"#;
    assert_eq!(run("generic_impl.nr", source), 42);
}

#[test]
fn an_own_bound_constrains_by_an_associated_type_on_struct_and_enum() {
    let source = r#"
trait Src {
    type Item
    func get(&self) -> Self::Item
    func merge<O: Src<Item = Self::Item>>(&self, o: &O) -> Self::Item
}

struct A { v: i32 }
enum E { One(i32), Two }

impl Src for A {
    type Item = i32
    func get(&self) -> i32 { self.v }
    func merge<P: Src<Item = i32>>(&self, o: &P) -> i32 { self.v + o.get() }
}

impl Src for E {
    type Item = i32
    func get(&self) -> i32 {
        match self {
            E::One(n) => n,
            E::Two => 2,
        }
    }
    func merge<O: Src<Item = Self::Item>>(&self, o: &O) -> Self::Item { self.get() * o.get() }
}

func go<T: Src<Item = i32>>(t: &T, a: &A) -> i32 { t.merge(a) }

func main() -> i32 {
    val a = A { v: 20 }
    val e = E::One(2)
    val two = E::Two
    go(&a, &a) + go(&e, &a) - go(&two, &a) + 2
}
"#;
    assert_eq!(run("own_bound.nr", source), 42);
}

#[test]
fn impl_trait_where_and_const_parameters_conform_and_dispatch() {
    let source = r#"
trait Name { func name(&self) -> i32 }

struct N { v: i32 }
impl Name for N { func name(&self) -> i32 { self.v } }

trait Reg {
    func add(&self, n: impl Name) -> i32
    func add2<U>(&self, n: U) -> i32
        where U: Name
    func head<const K: u32>(&self, xs: [i32; K]) -> i32 where K > 0
}

struct R { base: i32 }

impl Reg for R {
    func add<Z: Name>(&self, n: Z) -> i32 { self.base + n.name() }
    func add2(&self, n: impl Name) -> i32 { self.base + n.name() }
    func head<const M: u32>(&self, xs: [i32; M]) -> i32 where M > 0 { xs[0] }
}

func go<T: Reg>(t: T) -> i32 {
    t.add(N { v: 1 }) + t.add2(N { v: 2 }) + t.head([3, 4])
}

func main() -> i32 { go(R { base: 10 }) }
"#;
    assert_eq!(run("forms.nr", source), 26);
}

#[test]
fn a_where_predicate_is_checked_at_a_call_through_a_bound() {
    let err = compile_rejected(
        "where_call.nr",
        r#"
trait Reg { func head<const K: u32>(&self, xs: [i32; K]) -> i32 where K > 1 }
struct R { base: i32 }
impl Reg for R {
    func head<const M: u32>(&self, xs: [i32; M]) -> i32 where M > 1 { xs[0] }
}
func go<T: Reg>(t: T) -> i32 { t.head([3]) }
func main() -> i32 { go(R { base: 1 }) }
"#,
    );
    assert!(err.contains("where"), "{err}");
}

#[test]
fn closures_and_default_bodies_call_generic_methods() {
    let source = r#"
trait Mapper {
    func apply<U>(&self, f: (i32) -> U) -> U
    func twice<U: Copy>(&self, f: (i32) -> U) -> (U, U) {
        (self.apply(f), self.apply::<U>(f))
    }
    func nothing<U>(&self) -> Option<U> { Option::None }
}

struct S { v: i32 }

impl Mapper for S {
    func apply<U>(&self, f: (i32) -> U) -> U { f(self.v) }
}

func go<T: Mapper>(t: &T) -> i32 {
    val a = t.apply(|x| x + 1)
    val b = t.twice(|x| x * 2)
    val c = t.nothing::<i32>()
    a + b.0 + b.1 + (c ?? 1)
}

func main() -> i32 {
    val s = S { v: 10 }
    go(&s)
}
"#;
    assert_eq!(run("closures.nr", source), 52);
}

#[test]
fn receivers_of_every_mode_through_a_bound() {
    let source = r#"
trait Acc {
    func push<U: Copy>(&mut self, u: U, n: i32)
    func with<U>(self, u: U) -> Self
    func total(&self) -> i32
}

struct S { t: i32 }

impl Acc for S {
    func push<U: Copy>(&mut self, u: U, n: i32) { self.t += n }
    func with<U>(self, u: U) -> Self { S { t: self.t + 1 } }
    func total(&self) -> i32 { self.t }
}

func go<T: Acc>(t: T) -> i32 {
    mut m = t
    m.push(1.5, 3)
    m.push(true, 4)
    val n = m.with("s")
    n.total()
}

func main() -> i32 { go(S { t: 1 }) }
"#;
    assert_eq!(run("receivers.nr", source), 9);
}

#[test]
fn a_body_forwards_its_parameter_through_a_bound() {
    let source = r#"
trait G { func id<U>(&self, u: U) -> U }
struct S { v: i32 }
impl G for S { func id<U>(&self, u: U) -> U { u } }

func by_turbofish<V, T: G>(t: &T, v: V) -> V { t.id::<V>(v) }
func by_inference<U, T: G>(t: &T, v: U) -> Option<U> { Option::Some(t.id(v)) }

func main() -> i32 {
    val s = S { v: 1 }
    by_turbofish(&s, 40) + (by_inference(&s, 2) ?? 0)
}
"#;
    assert_eq!(run("forward.nr", source), 42);
}

#[test]
fn a_call_through_a_bound_reports_its_arguments() {
    let err = compile_rejected(
        "bad_calls.nr",
        r#"
trait Pl {
    func plain(&self) -> i32
    func generic<U: Copy>(&self, u: U) -> i32
}
struct NotCopy { v: StringBuilder }
func go<T: Pl>(t: T) -> i32 {
    val x = t.plain::<i32>()
    val y = t.generic(NotCopy { v: StringBuilder::new() })
    val z = t.generic::<i32, i32>(1)
    x + y + z
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("turbofish supplies 1 generic argument(s), but 'plain' declares 0"),
        "{err}"
    );
    assert!(
        err.contains("`NotCopy` for 'U' does not implement required trait 'Copy'"),
        "{err}"
    );
    assert!(
        err.contains("turbofish supplies 2 generic argument(s), but 'generic' declares 1"),
        "{err}"
    );
}

#[test]
fn an_argument_moves_into_a_call_through_a_bound() {
    let err = compile_rejected(
        "moved_arg.nr",
        r#"
trait Take { func take<U>(&self, u: U) -> i32 }
func go<T: Take>(t: &T) -> i32 {
    val s = StringBuilder::new()
    t.take(s) + t.take(s)
}
func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("use of moved value 's'"), "{err}");
}

#[test]
fn a_by_value_receiver_moves_through_a_bound() {
    for (name, call) in [("plain.nr", "t.eat()"), ("generic.nr", "t.eat_with(1)")] {
        let source = format!(
            r#"
trait P {{
    func eat(self) -> i32
    func eat_with<U>(self, u: U) -> i32
}}
func go<T: P>(t: T) -> i32 {{ {call} + {call} }}
func main() -> i32 {{ 0 }}
"#
        );
        let err = compile_rejected(name, &source);
        assert!(err.contains("use of moved value 't'"), "{err}");
    }
    let source = r#"
trait P { func eat(self) -> i32 }
@derive(Copy)
struct S { v: i32 }
impl P for S { func eat(self) -> i32 { self.v } }
func go<T: P + Copy>(t: T) -> i32 { t.eat() + t.eat() }
func main() -> i32 { go(S { v: 21 }) }
"#;
    assert_eq!(run("copy_receiver.nr", source), 42);
}

#[test]
fn a_trait_with_one_is_refused_as_dyn_and_others_keep_their_vtable() {
    let err = compile_rejected(
        "dyn_refused.nr",
        r#"
trait G { func f<U>(&self, u: U) -> U }
func take(g: &dyn G) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        err.contains("not object-safe") && err.contains("declares type parameters"),
        "{err}"
    );
    let source = r#"
trait L { func pick<'a>(&self, x: &'a i32, y: &'a i32) -> &'a i32 }
struct S { v: i32 }
impl L for S { func pick<'a>(&self, x: &'a i32, y: &'a i32) -> &'a i32 { x } }
func first(l: &dyn L) -> i32 {
    val a = 42
    val b = 1
    *l.pick(&a, &b)
}
func main() -> i32 {
    val s = S { v: 1 }
    first(&s)
}
"#;
    assert_eq!(run("lifetime_dyn.nr", source), 42);
}

#[test]
fn a_lang_item_impl_still_declares_none() {
    let err = compile_rejected(
        "lang_items.nr",
        r#"
struct S { v: i32 }
impl Drop for S { func drop<U>(&mut self) {} }
@derive(Copy)
struct V { x: i32 }
impl Add for V {
    type Output = V
    func add<U>(self, rhs: V) -> V { V { x: self.x + rhs.x } }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("`Drop` is compiler-known"), "{err}");
    assert!(err.contains("`Add` is compiler-known"), "{err}");
}

// The body of a default is copied into the impl, where the name would mean two parameters.
#[test]
fn a_default_method_parameter_named_like_the_impls_is_refused() {
    let err = compile_rejected(
        "default_shadow.nr",
        r#"
trait Tr { func f<T>(&self, t: T) -> T { t } }
struct C<T> { v: T }
impl<T> Tr for C<T> {}
func main() -> i32 { 0 }
"#,
    );
    assert!(err.contains("reuses a name its `impl`"), "{err}");
}

// Regression: a method's parameter shadows a `type` alias of the same name, as a function's
// does. The alias was expanded over it, so `U` meant `i32` in the method's signature.
#[test]
fn a_method_parameter_shadows_a_type_alias() {
    let source = r#"
type U = i32
trait Tr { func f<U>(&self, u: U) -> U }
struct S { v: i32 }
impl Tr for S { func f<U>(&self, u: U) -> U { u } }
struct P { v: i32 }
impl P { func g<U>(&self, u: U) -> U { u } }
func main() -> i32 {
    val s = S { v: 1 }
    val p = P { v: 1 }
    if s.f(true) && p.g(true) { 42 } else { 0 }
}
"#;
    assert_eq!(run("alias_shadow.nr", source), 42);
}

// Regression: a binding on a bound was compared unsubstituted, so a call with a concrete
// `Item` was refused against the parameter's name.
#[test]
fn a_bound_constrains_by_another_parameter_of_its_list() {
    let source = r#"
trait Src {
    type Item
    func get(&self) -> Self::Item
}
struct Cell { v: i32 }
impl Src for Cell {
    type Item = i32
    func get(&self) -> i32 { self.v }
}
func keep<V, T: Src<Item = V>>(s: &T, seed: V) -> V { seed }
func main() -> i32 {
    val c = Cell { v: 20 }
    keep(&c, 42)
}
"#;
    assert_eq!(run("bound_by_parameter.nr", source), 42);
    let err = compile_rejected(
        "bound_by_parameter_wrong.nr",
        &source.replace("keep(&c, 42)", "keep(&c, true)\n    0"),
    );
    assert!(err.contains("binds `Item` to i32"), "{err}");
}

// Regression: an imported trait's rename did not reach a bound on a method's own parameter.
#[test]
fn an_import_rename_reaches_a_methods_own_bound() {
    let test = CompileTest::new();
    test.write_source(
        "lib.nr",
        r#"
export trait Show { func show<U: Tag>(&self, u: U) -> i32 }
export trait Tag { func tag(&self) -> i32 }
export struct W { export v: i32 }
impl Tag for W { func tag(&self) -> i32 { self.v } }
impl Show for W { func show<U: Tag>(&self, u: U) -> i32 { self.v + u.tag() } }
"#,
    );
    let main = test.write_source(
        "main.nr",
        r#"
import lib::{Show as Sh, Tag as Tg, W}
struct Q { v: i32 }
impl Tg for Q { func tag(&self) -> i32 { self.v } }
impl Sh for Q { func show<U: Tg>(&self, u: U) -> i32 { self.v * u.tag() } }
struct P { v: i32 }
impl P { func g<U: Tg>(&self, u: U) -> i32 { u.tag() } }
func go<T: Sh>(t: &T) -> i32 { t.show(W { v: 2 }) }
func main() -> i32 {
    val w = W { v: 1 }
    val q = Q { v: 3 }
    val p = P { v: 0 }
    go(&w) + go(&q) + p.g(Q { v: 4 })
}
"#,
    );
    let exe = test.compile(&main).expect("the program should compile");
    assert_eq!(test.run_executable(&exe), Ok(13));
}
