// End-to-end tests for newtype declarations: `newtype Name = T` creates a
// distinct nominal type wrapping `T`. Construction is `Name(value)`, the inner
// value is read via `.0`, and (unlike a transparent `type` alias) the newtype is
// not interchangeable with its inner type. These tests exercise the full pipeline:
// parse → type-check → HIR lowering → LLVM codegen → native run.
use crate::compile_harness::CompileTest;

#[test]
fn newtype_construction_and_inner_access_run() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
func main() -> i32 {
    val m: Meters = Meters(29)
    m.0
}
"#;
    let exit = test
        .compile_and_run("newtype_basic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 29);
}

#[test]
fn newtype_crosses_function_boundaries() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
func add(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}
func main() -> i32 {
    val total: Meters = add(Meters(17), Meters(25))
    total.0
}
"#;
    let exit = test
        .compile_and_run("newtype_boundaries.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

#[test]
fn newtype_over_float_inner_runs() {
    // A newtype forwards Copy from its inner type; an f64 wrapper works end to end.
    let test = CompileTest::new();
    let source = r#"
newtype Celsius = f64
func main() -> i32 {
    val t: Celsius = Celsius(36.5)
    val raw: f64 = t.0
    raw as i32
}
"#;
    let exit = test
        .compile_and_run("newtype_float.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 36);
}

#[test]
fn newtype_as_struct_field_runs() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
newtype Seconds = i32
struct Trip {
    distance: Meters,
    duration: Seconds
}
func score(t: &Trip) -> i32 {
    t.distance.0 + t.duration.0
}
func main() -> i32 {
    val trip = Trip { distance: Meters(30), duration: Seconds(12) }
    score(&trip)
}
"#;
    let exit = test
        .compile_and_run("newtype_field.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 42);
}

/// A newtype takes `impl` blocks and trait impls as a struct does. Every `impl` on one
/// was refused as an unknown struct, so a newtype had no methods and no operators.
#[test]
fn regression_bug_078_a_newtype_takes_an_impl() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
impl Meters {
    func double(&self) -> Meters { Meters(self.0 * 2) }
    func zero() -> Meters { Meters(0) }
    func grow(&mut self, d: i32) { self = Meters(self.0 + d) }
}
// Not the inner `+`: the user's impl must be the one that runs.
impl Add for Meters {
    type Output = Meters
    func add(self, rhs: Meters) -> Meters { Meters(self.0 * 10 + rhs.0) }
}
impl PartialEq for Meters {
    func eq(&self, other: &Meters) -> bool { self.0 == other.0 }
}
trait Size { func size(&self) -> i32 }
impl Size for Meters { func size(&self) -> i32 { self.0 } }
func by_bound<T: Size>(t: &T) -> i32 { t.size() }
func by_dyn(t: &dyn Size) -> i32 { t.size() }
func bump(m: &mut Meters) { m.grow(10) }
@derive(Copy, Clone)
struct Pt { x: i32, y: i32 }
newtype Flipped = Pt
impl Flipped {
    func flip(&mut self) { self = Flipped(Pt { x: self.0.y, y: self.0.x }) }
}
func main() -> i32 {
    mut m = Meters(1) + Meters(2)
    if !(m == Meters(12)) { return 1 }
    m.grow(3)
    bump(&mut m)
    if m.double().0 != 50 { return 2 }
    if Meters::zero().0 != 0 { return 3 }
    if by_bound(&m) != 25 || by_dyn(&m) != 25 { return 4 }
    mut f = Flipped(Pt { x: 1, y: 9 })
    f.flip()
    f.0.x
}
"#;
    let exit = test
        .compile_and_run("newtype_impl.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

/// `.0` through a borrow of a newtype reads the inner value. The operand was the
/// newtype's address, which the arithmetic after it refused.
#[test]
fn regression_newtype_inner_read_through_a_borrow() {
    let test = CompileTest::new();
    let source = r#"
newtype Meters = i32
func f(m: &Meters) -> i32 { m.0 + 1 }
func main() -> i32 {
    val m = Meters(4)
    f(&m)
}
"#;
    let exit = test
        .compile_and_run("newtype_borrow_read.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}
