// End-to-end tests for operator traits (§3.10): operators on user types are sugar for
// trait method calls. A `Copy` struct that implements `Add`/`Sub`/…/`Neg`/`Not`/
// `PartialEq`/`Comparable` gets the matching operator, dispatched to its impl method and
// monomorphized to a plain call — no vtable. These exercise the full pipeline:
// parse → type-check → HIR lowering → LLVM codegen → native run.
mod common;
use common::CompileTest;

#[test]
fn arithmetic_and_unary_operators_dispatch() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }
impl Add for Vec2 { type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x + rhs.x, y: self.y + rhs.y } } }
impl Sub for Vec2 { type Output = Vec2
    func sub(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x - rhs.x, y: self.y - rhs.y } } }
impl Neg for Vec2 { type Output = Vec2
    func neg(self) -> Vec2 { Vec2 { x: -self.x, y: -self.y } } }
func main() -> i32 {
    val a = Vec2 { x: 10, y: 20 }
    val b = Vec2 { x: 3, y: 4 }
    val c = a + b            // (13, 24)
    val d = a - b            // (7, 16)
    val e = -b               // (-3, -4)
    c.x + c.y + d.x + d.y + e.x + e.y   // 13+24+7+16-3-4 = 53
}
"#;
    let exit = test
        .compile_and_run("op_arith.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 53);
}

#[test]
fn bitwise_operators_dispatch() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Flags { bits: i32 }
impl BitAnd for Flags { type Output = Flags
    func bitand(self, rhs: Flags) -> Flags { Flags { bits: self.bits & rhs.bits } } }
impl BitOr for Flags { type Output = Flags
    func bitor(self, rhs: Flags) -> Flags { Flags { bits: self.bits | rhs.bits } } }
func main() -> i32 {
    val a = Flags { bits: 6 }   // 0b110
    val b = Flags { bits: 3 }   // 0b011
    val both = a | b            // 0b111 = 7
    val common = a & b          // 0b010 = 2
    both.bits + common.bits     // 9
}
"#;
    let exit = test
        .compile_and_run("op_bitwise.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}

#[test]
fn comparison_operators_dispatch() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Money { cents: i32 }
impl PartialEq for Money {
    func eq(&self, rhs: &Money) -> bool { self.cents == rhs.cents }
    func ne(&self, rhs: &Money) -> bool { self.cents != rhs.cents } }
impl Comparable for Money {
    func lt(&self, rhs: &Money) -> bool { self.cents < rhs.cents }
    func le(&self, rhs: &Money) -> bool { self.cents <= rhs.cents }
    func gt(&self, rhs: &Money) -> bool { self.cents > rhs.cents }
    func ge(&self, rhs: &Money) -> bool { self.cents >= rhs.cents } }
func main() -> i32 {
    val a = Money { cents: 150 }
    val b = Money { cents: 275 }
    mut score = 0
    if a < b { score = score + 1 }
    if b > a { score = score + 2 }
    if a != b { score = score + 4 }
    if a <= a { score = score + 8 }
    if a == b { score = score + 100 }
    score   // 1+2+4+8 = 15
}
"#;
    let exit = test
        .compile_and_run("op_compare.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn compound_assignment_uses_operator_trait() {
    // `x += y` desugars to `x = x + y`, which dispatches through the `Add` impl (§3.10).
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }
impl Add for Vec2 { type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x + rhs.x, y: self.y + rhs.y } } }
func main() -> i32 {
    mut acc = Vec2 { x: 0, y: 0 }
    val step = Vec2 { x: 2, y: 5 }
    acc += step
    acc += step
    acc.x + acc.y   // (4 + 10) = 14
}
"#;
    let exit = test
        .compile_and_run("op_compound.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 14);
}
