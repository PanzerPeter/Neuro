// `impl` blocks and trait impls on enums: inherent methods over every receiver form,
// associated functions, user traits through a bound and a trait object, operator traits,
// generic enums, the iterator protocol, and the two rejections the enum target adds.
use crate::compile_harness::CompileTest;

const LIGHT: &str = r#"
enum Light {
    Red,
    Yellow,
    Green
}

impl Light {
    func start() -> Light {
        return Light::Red
    }

    func code(&self) -> i32 {
        match self {
            Light::Red => 1,
            Light::Yellow => 2,
            Light::Green => 3
        }
    }

    func advance(&mut self) {
        self = match self {
            Light::Red => Light::Green,
            Light::Green => Light::Yellow,
            Light::Yellow => Light::Red
        }
    }

    func is_stop(self) -> bool {
        return self.code() == 1
    }
}
"#;

fn run(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

#[test]
fn inherent_methods_cover_every_receiver_and_an_associated_function() {
    let source = format!(
        "{LIGHT}
func main() -> i32 {{
    mut l = Light::start()
    val first = l.code()
    l.advance()
    val second = l.code()
    l.advance()
    mut stop = 0
    if l.is_stop() {{ stop = 100 }}
    return first * 10 + second + stop + l.code() * 1000
}}
"
    );
    // Red (1), then Green (3), then Yellow (2), which is not a stop.
    assert_eq!(run("enum_inherent.nr", &source), (13 + 2000) % 256);
}

#[test]
fn a_trait_impl_dispatches_statically_dynamically_and_through_a_default() {
    let source = format!(
        "{LIGHT}
trait Describe {{
    func id(&self) -> i32
    func twice(&self) -> i32 {{
        return self.id() * 2
    }}
}}

impl Describe for Light {{
    func id(&self) -> i32 {{
        return self.code() + 10
    }}
}}

func through_bound<T: Describe>(x: T) -> i32 {{
    return x.twice()
}}

func through_object(x: &dyn Describe) -> i32 {{
    return x.id()
}}

func main() -> i32 {{
    val g = Light::Green
    return through_bound(g) + through_object(&g)
}}
"
    );
    // Green's id is 13: 26 through the default method, 13 through the vtable.
    assert_eq!(run("enum_trait.nr", &source), 39);
}

#[test]
fn an_operator_impl_gives_an_enum_equality() {
    let source = format!(
        "{LIGHT}
impl PartialEq for Light {{
    func eq(&self, other: &Light) -> bool {{
        return self.code() == other.code()
    }}
}}

func main() -> i32 {{
    val a = Light::Yellow
    val b = Light::Yellow
    val c = Light::Red
    mut r = 0
    if a == b {{ r = r + 1 }}
    if !(a == c) {{ r = r + 10 }}
    return r
}}
"
    );
    assert_eq!(run("enum_eq.nr", &source), 11);
}

#[test]
fn a_generic_enum_impl_is_monomorphized_per_instance() {
    let source = r#"
enum Tree<T> {
    Leaf(T),
    Empty
}

struct Holder {
    t: Tree<i32>
}

impl<T> Tree<T> {
    func or(self, fallback: T) -> T {
        match self {
            Tree::Leaf(v) => v,
            Tree::Empty => fallback
        }
    }
}

trait Weigh {
    func weight(&self) -> i32
}

impl<T> Weigh for Tree<T> {
    func weight(&self) -> i32 {
        match self {
            Tree::Leaf(_) => 1,
            Tree::Empty => 0
        }
    }
}

func heavy<W: Weigh>(w: W) -> i32 {
    return w.weight() * 50
}

func main() -> i32 {
    val h = Holder { t: Tree::Leaf(7) }
    val f: Tree<f64> = Tree::Empty
    val half = f.or(2.5) * 2.0
    return h.t.or(0) + half as i32 + heavy(h.t) + heavy(f)
}
"#;
    assert_eq!(run("enum_generic_impl.nr", source), 7 + 5 + 50);
}

#[test]
fn an_enum_drives_a_for_loop_through_its_iterator_impl() {
    let source = r#"
enum Countdown {
    Left(i32),
    Done
}

impl Iterator for Countdown {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        match self {
            Countdown::Left(n) => {
                if n == 0 {
                    self = Countdown::Done
                    return Option::None
                }
                self = Countdown::Left(n - 1)
                return Option::Some(n)
            },
            Countdown::Done => Option::None
        }
    }
}

func main() -> i32 {
    mut total = 0
    for x in Countdown::Left(4) {
        total = total + x
    }
    return total
}
"#;
    assert_eq!(run("enum_iterator.nr", source), 10);
}

#[test]
fn an_impl_member_named_like_a_variant_is_rejected() {
    let test = CompileTest::new();
    let error = test
        .check(
            "enum_clash.nr",
            r#"
enum Shape {
    Circle(f64),
    Dot
}

impl Shape {
    func Circle(r: f64) -> Shape {
        return Shape::Dot
    }
}

func main() -> i32 {
    return 0
}
"#,
        )
        .expect_err("a method sharing a variant's name must be refused");
    assert!(
        error.contains("is both a variant of enum 'Shape'"),
        "the diagnostic must name the clash: {error}"
    );
}

#[test]
fn an_enum_drop_impl_is_rejected() {
    let test = CompileTest::new();
    let error = test
        .check(
            "enum_drop.nr",
            r#"
enum Handle {
    Open(i32),
    Closed
}

impl Drop for Handle {
    func drop(&mut self) {}
}

func main() -> i32 {
    return 0
}
"#,
        )
        .expect_err("only a struct runs a user destructor");
    assert!(
        error.contains("an enum may not implement `Drop`"),
        "the diagnostic must say why: {error}"
    );
}
