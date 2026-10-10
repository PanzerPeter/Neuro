// A generic instantiated with an enclosing type parameter: `Option<T>` in a generic
// function's signature, a `Wrapper<U>` field in a generic struct, `Cell<T>` and `Self` in a
// generic impl, and the calls that infer through them.
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
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hi\n");
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
    let err = rejected("dup_option.nr", source);
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
    let err = rejected("mismatch.nr", source);
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
    let err = rejected("no_infer.nr", source);
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
    let err = rejected("grow.nr", source);
    assert!(err.contains("nests generic instances more than"), "{err}");
}
