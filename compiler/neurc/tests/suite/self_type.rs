// `Self` as a type: inside an `impl` it names the type being extended, in signatures,
// annotations, struct literals, paths and patterns; inside a `trait` it names the
// implementing type, which conformance, bounded calls and object safety each answer.
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
fn self_names_the_extended_struct_in_signatures_and_bodies() {
    let source = r#"
struct Point { x: i32, y: i32 }

impl Point {
    func origin() -> Self { Self { x: 0, y: 0 } }

    func shifted(&self, by: i32) -> Self {
        val moved: Self = Self { x: self.x + by, y: self.y + by * 2 }
        moved
    }

    func sum(&self, other: &Self) -> i32 { self.x + self.y + other.x + other.y }
}

func main() -> i32 {
    val p = Point::origin().shifted(3)
    val q = Point { x: 1, y: 1 }
    p.sum(&q)
}
"#;
    assert_eq!(run("self_struct.nr", source), 11);
}

#[test]
fn self_names_the_extended_enum_in_paths_and_patterns() {
    let source = r#"
enum Shape {
    Square(i32),
    Rect { w: i32, h: i32 },
    Empty
}

impl Shape {
    func unit() -> Self { Self::Square(1) }

    func wide(w: i32) -> Self { Self::Rect { w: w, h: 2 } }

    func area(&self) -> i32 {
        match self {
            Self::Square(s) => s * s,
            Self::Rect { w, h } => w * h,
            Self::Empty => 0
        }
    }

    func side_or(&self, fallback: i32) -> i32 {
        val Self::Square(s) = self else { return fallback }
        s
    }
}

func main() -> i32 {
    val shapes = Shape::unit().area() + Shape::wide(5).area() + Shape::Empty.area()
    shapes + Shape::unit().side_or(9) + Shape::Empty.side_or(9)
}
"#;
    assert_eq!(run("self_enum.nr", source), 21);
}

#[test]
fn self_in_a_generic_impl_is_the_instance_the_receiver_has() {
    // `Self` here is `Cell<T>`, the type `self` already has, so returning `self` and
    // building one with `Self { .. }` both type, per instance.
    let source = r#"
struct Cell<T> { value: T }

impl<T> Cell<T> {
    func get(self) -> T { self.value }

    func same(self) -> Self { self }

    func rewrap(self) -> Self { Self { value: self.value } }
}

func main() -> i32 {
    val c = Cell { value: 40 }
    val d = Cell { value: true }
    if d.same().get() { c.same().rewrap().get() + 2 } else { 0 }
}
"#;
    assert_eq!(run("self_generic_impl.nr", source), 42);
}

const SCALABLE: &str = r#"
trait Scalable {
    func scaled(&self, by: i32) -> Self
    func bigger(&self, other: &Self) -> bool

    func doubled_twice(&self) -> Self {
        val once: Self = self.scaled(2)
        once.scaled(2)
    }
}

struct Meters { v: i32 }
struct Grams { g: i32 }

impl Scalable for Meters {
    func scaled(&self, by: i32) -> Self { Self { v: self.v * by } }
    func bigger(&self, other: &Self) -> bool { self.v > other.v }
}

impl Scalable for Grams {
    func scaled(&self, by: i32) -> Grams { Grams { g: self.g * by } }
    func bigger(&self, other: &Grams) -> bool { self.g > other.g }
}

func larger<T: Scalable>(a: &T, by: i32) -> T {
    val grown = a.scaled(by)
    if grown.bigger(a) { grown } else { a.scaled(1) }
}
"#;

#[test]
fn a_trait_signature_naming_self_is_met_by_either_spelling() {
    let source = format!(
        "{SCALABLE}
func main() -> i32 {{
    val m = Meters {{ v: 3 }}.scaled(2)
    val g = Grams {{ g: 5 }}.scaled(3)
    val small = Grams {{ g: 1 }}
    if g.bigger(&small) {{ m.v + g.g }} else {{ 0 }}
}}
"
    );
    assert_eq!(run("self_trait_conformance.nr", &source), 21);
}

#[test]
fn a_bounded_call_returns_the_type_parameter_for_self() {
    let source = format!(
        "{SCALABLE}
func main() -> i32 {{
    val four = Meters {{ v: 4 }}
    val two = Grams {{ g: 2 }}
    val m = larger(&four, 5)
    val g = larger(&two, 3)
    m.v + g.g
}}
"
    );
    assert_eq!(run("self_bounded_call.nr", &source), 26);
}

#[test]
fn a_default_method_naming_self_is_typed_per_implementor() {
    let source = format!(
        "{SCALABLE}
func main() -> i32 {{
    Meters {{ v: 2 }}.doubled_twice().v + Grams {{ g: 10 }}.doubled_twice().g
}}
"
    );
    assert_eq!(run("self_default_method.nr", &source), 48);
}

#[test]
fn an_impl_returning_another_type_for_self_is_rejected() {
    let source = r#"
trait Scalable {
    func scaled(&self, by: i32) -> Self
}

struct Meters { v: i32 }

impl Scalable for Meters {
    func scaled(&self, by: i32) -> i32 { self.v * by }
}

func main() -> i32 { 0 }
"#;
    let err = rejected("self_mismatch.nr", source);
    assert!(
        err.contains("scaled") && err.contains("Meters"),
        "the diagnostic should name the method and the type Self stands for; got: {err}"
    );
}

#[test]
fn a_trait_naming_self_is_not_object_safe() {
    let source = r#"
trait Scalable {
    func scaled(&self, by: i32) -> Self
}

struct Meters { v: i32 }

impl Scalable for Meters {
    func scaled(&self, by: i32) -> Self { Self { v: self.v * by } }
}

func grow(s: &dyn Scalable) -> i32 { 0 }

func main() -> i32 { 0 }
"#;
    let err = rejected("self_dyn.nr", source);
    assert!(
        err.contains("object-safe") && err.contains("Self"),
        "the diagnostic should say the trait names `Self`; got: {err}"
    );
}

#[test]
fn self_outside_an_impl_or_trait_is_rejected() {
    let source = r#"
func make() -> Self { 0 }

func main() -> i32 { 0 }
"#;
    let err = rejected("self_free_function.nr", source);
    assert!(
        err.contains("`Self`") && err.contains("impl"),
        "the diagnostic should say where `Self` is valid; got: {err}"
    );
}
