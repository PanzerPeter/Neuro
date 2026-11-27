// A generic instantiated with an enclosing type parameter: `Option<T>` in a generic
// function's signature, a `Wrapper<U>` field in a generic struct, `Cell<T>` and `Self` in a
// generic impl, and the calls that infer through them.
use crate::compile_harness::{CompileTest, compile_rejected, run};

#[test]
fn a_generic_function_returns_and_takes_an_option_of_its_parameter() {
    let source = r#"
func first_some<T>(x: T) -> Option<T> {
    Option::Some(x)
}

func unwrap_or<U>(o: Option<U>, d: U) -> U {
    match o {
        Option::Some(v) => v,
        Option::None => d,
    }
}

func main() -> i32 {
    val a = unwrap_or(first_some(40), 0)
    val none: Option<i64> = Option::None
    a + unwrap_or(none, 2i64) as i32
}
"#;
    assert_eq!(run("option_of_t.nr", source), 42);
}

#[test]
fn a_string_travels_through_an_option_of_the_parameter() {
    let source = r#"
func boxed<T>(x: T) -> Option<T> {
    Option::Some(x)
}

func main() -> i32 {
    match boxed("hi") {
        Option::Some(t) => {
            println(t)
            0
        },
        Option::None => 1,
    }
}
"#;
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("boxed_string.nr", source))
        .expect("compile failed");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("run executable");
    assert_eq!(run.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"),
        "hi\n"
    );
}

#[test]
fn a_generic_body_hands_its_option_to_another_generic() {
    // Both callees name their parameter `T`, as the caller does: the caller's `Option<T>`
    // still binds the callee's `T` to the caller's.
    let source = r#"
func first_of<T>(cond: bool, x: T) -> Option<T> {
    if cond { Option::Some(x) } else { Option::None }
}

func or_zero<T>(o: Option<T>, zero: T) -> T {
    o ?? zero
}

func pick<T>(cond: bool, x: T, zero: T) -> T {
    or_zero(first_of(cond, x), zero)
}

func main() -> i32 {
    pick(true, 40, 0) + pick(false, 7, 2)
}
"#;
    assert_eq!(run("handed_on.nr", source), 42);
}

#[test]
fn a_generic_struct_holds_another_generic_at_its_parameter() {
    let source = r#"
struct Inner<T> {
    v: T,
}

struct Outer<U> {
    i: Inner<U>,
    n: i32,
}

func wrap<T>(x: T) -> Inner<T> {
    Inner { v: x }
}

func main() -> i32 {
    val o = Outer { i: Inner { v: 40 }, n: 2 }
    val w = wrap(true)
    if w.v { o.i.v + o.n } else { 0 }
}
"#;
    assert_eq!(run("struct_field.nr", source), 42);
}

#[test]
fn arguments_bind_through_an_instance_in_any_order() {
    let source = r#"
struct Pair<T, U> {
    a: T,
    b: U,
}

func swap<T, U>(p: Pair<T, U>) -> Pair<U, T> {
    Pair { a: p.b, b: p.a }
}

func second<X, Y>(p: Pair<Y, X>) -> X {
    p.b
}

func main() -> i32 {
    val q = swap(Pair { a: 2, b: true })
    val flag = q.a
    val r = swap(q)
    val n = r.a
    if flag && second(r) { 40 + n } else { 0 }
}
"#;
    assert_eq!(run("swap.nr", source), 42);
}

#[test]
fn a_generic_impl_names_its_own_instance_and_its_constructor_infers_it() {
    let source = r#"
struct Cell<T> {
    v: T,
}

impl<T> Cell<T> {
    func new(v: T) -> Cell<T> {
        Cell { v: v }
    }

    func same(self) -> Self {
        self
    }

    func maybe(self) -> Option<T> {
        Option::Some(self.v)
    }
}

func peel<U>(c: Cell<U>) -> Option<U> {
    c.maybe()
}

func main() -> i32 {
    val flag = Cell::new(true).same().maybe() ?? false
    val n = peel(Cell::new(42)) ?? 0
    if flag { n } else { 0 }
}
"#;
    assert_eq!(run("generic_impl.nr", source), 42);
}

#[test]
fn an_associated_function_takes_its_parameter_from_a_turbofish() {
    let source = r#"
struct Bag<T> {
    v: T,
}

impl<T> Bag<T> {
    func nothing() -> Option<T> {
        Option::None
    }
}

func main() -> i32 {
    Bag::nothing::<i32>() ?? 42
}
"#;
    assert_eq!(run("turbofish.nr", source), 42);
}

#[test]
fn a_generic_impl_conforms_to_a_trait_that_names_self() {
    let source = r#"
trait Again {
    func again(self) -> Self
    func wrap(self) -> Option<Self>
}

struct Cell<T> {
    v: T,
}

impl<T> Again for Cell<T> {
    func again(self) -> Self {
        self
    }

    func wrap(self) -> Option<Self> {
        Option::Some(self.again())
    }
}

func boxed<A: Again>(a: A) -> Option<A> {
    a.wrap()
}

func main() -> i32 {
    match boxed(Cell { v: 7 }) {
        Option::Some(e) => e.v * 6,
        Option::None => 0,
    }
}
"#;
    assert_eq!(run("trait_self.nr", source), 42);
}

#[test]
fn question_mark_propagates_out_of_a_result_of_the_parameter() {
    let source = r#"
func half<T>(x: T, ok: bool) -> Result<T, string> {
    if ok { Result::Ok(x) } else { Result::Err("odd") }
}

func chain<T>(x: T, ok: bool) -> Result<Option<T>, string> {
    val y = half(x, ok)?
    Result::Ok(Option::Some(y))
}

func main() -> i32 {
    val failed = match chain(1, false) {
        Result::Ok(_) => 100,
        Result::Err(_) => 0,
    }
    match chain(42, true) {
        Result::Ok(o) => failed + (o ?? 0),
        Result::Err(_) => 1,
    }
}
"#;
    assert_eq!(run("result_chain.nr", source), 42);
}

#[test]
fn an_option_of_the_parameter_moves_like_the_parameter() {
    let source = r#"
func dup<T>(o: Option<T>) -> (Option<T>, Option<T>) {
    (o, o)
}

func main() -> i32 { 0 }
"#;
    let err = compile_rejected("dup_option.nr", source);
    assert!(err.contains("use of moved value 'o'"), "{err}");
}

#[test]
fn an_instance_at_the_parameter_is_not_an_instance_at_a_concrete_type() {
    let source = r#"
func bad<T>(x: T) -> Option<T> {
    Option::Some(1)
}

func main() -> i32 { 0 }
"#;
    let err = compile_rejected("mismatch.nr", source);
    assert!(
        err.contains("return type mismatch") && err.contains("Option<i32>"),
        "{err}"
    );
}

#[test]
fn an_associated_function_with_no_argument_to_infer_from_asks_for_a_turbofish() {
    let source = r#"
struct Bag<T> {
    v: T,
}

impl<T> Bag<T> {
    func nothing() -> Option<T> {
        Option::None
    }
}

func main() -> i32 {
    val n = Bag::nothing()
    0
}
"#;
    let err = compile_rejected("no_infer.nr", source);
    assert!(
        err.contains("generic parameter 'T' cannot be inferred"),
        "{err}"
    );
}

#[test]
fn a_generic_that_feeds_itself_a_growing_type_is_refused_not_compiled_forever() {
    let source = r#"
func grow<T>(x: T, n: i32) -> i32 {
    if n == 0 { 0 } else { grow(Option::Some(x), n - 1) }
}

func main() -> i32 {
    grow(1, 3)
}
"#;
    let err = compile_rejected("grow.nr", source);
    assert!(err.contains("nests generic instances more than"), "{err}");
}

/// BUG-152: registering an instance's methods built the instance each signature named, so
/// `wrap` on `W<i32>` built `W<W<i32>>`, whose `wrap` built the next level without end.
#[test]
fn regression_a_method_returning_its_own_type_nested_in_itself_compiles() {
    let source = r#"
struct W<I> { inner: I }

impl<I> W<I> {
    func wrap(self) -> W<W<I>> { W { inner: self } }
}

func main() -> i32 {
    val w = W { inner: 3 }
    w.wrap().inner.inner
}
"#;
    assert_eq!(run("self_nesting.nr", source), 3);
}

#[test]
fn a_self_nesting_method_agrees_across_equivalent_forms() {
    let source = r#"
struct W<I> { inner: I }

impl<I> W<I> {
    func wrap(self) -> W<W<I>> { W { inner: self } }
    func get(self) -> I { self.inner }
    func both(x: I) -> W<W<I>> { W { inner: W { inner: x } } }
}

enum E<T> { One(T) }

impl<T> E<T> {
    func nest(self) -> E<E<T>> { E::One(self) }
}

func twice<T>(w: W<T>) -> W<W<W<T>>> { w.wrap().wrap() }

func echo(x: i32) -> i32 { x }

func main() -> i32 {
    val g = echo(W::both::<W<i32>>(W { inner: 6 }).inner.inner.inner)
    val a = W { inner: 1 }.wrap().inner.inner
    val bound = W { inner: 2 }.wrap()
    val b = bound.get().get()
    val c = if a == 1 { W { inner: 3 }.wrap().wrap().get().get().get() } else { 0 }
    val d = twice(W { inner: 4 }).inner.inner.inner
    val f = |x: i32| -> i32 { W { inner: x }.wrap().get().inner }
    val e = f(5)
    val h = match E::One(7).nest() {
        E::One(E::One(v)) => v,
    }
    a + b * 2 + c * 3 + d * 4 + e * 5 + g * 6 + h - 7
}
"#;
    // 1 + 4 + 9 + 16 + 25 + 36 + 0
    assert_eq!(run("self_nesting_forms.nr", source), 91);
}

#[test]
fn a_self_nesting_method_drops_every_level_once() {
    let source = r#"
struct D<I> { inner: I }

impl<I> D<I> {
    func wrap(self) -> D<D<I>> { D { inner: self } }
}

impl<I> Drop for D<I> {
    func drop(&mut self) {
        println("drop")
    }
}

struct S<I> { inner: I }

impl<I> S<I> {
    func wrap(self) -> S<S<I>> { S { inner: self } }
}

func main() -> i32 {
    val d = D { inner: 1 }.wrap().wrap()
    mut total: u64 = 0
    for i in 0..50 {
        val s = S { inner: "level {i}" }.wrap().wrap()
        total = total + s.inner.inner.inner.len()
    }
    println("{total}")
    0
}
"#;
    assert_eq!(
        crate::compile_harness::stdout_of("self_nesting_drop.nr", source),
        "390\ndrop\ndrop\ndrop\n"
    );
    assert_eq!(
        CompileTest::new().run_leak_checked("self_nesting_leaks.nr", source),
        0
    );
}
