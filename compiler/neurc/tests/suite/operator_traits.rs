// End-to-end tests for operator traits: operators on user types are sugar for
// trait method calls. A type that implements `Add`/`Sub`/…/`Neg`/`BitNot`/`PartialEq`/
// `Comparable`/`*Assign` gets the matching operator, dispatched to its impl method and
// monomorphized to a plain call: no vtable. These exercise the full pipeline:
// parse → type-check → HIR lowering → LLVM codegen → native run.
use crate::compile_harness::CompileTest;

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
    // `x += y` desugars to `x = x + y`, which dispatches through the `Add` impl.
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

#[test]
fn equality_without_a_partial_eq_impl_is_rejected() {
    // Accepting it produced a binary that the backend refused to build, aborting the
    // compiler with an internal error instead of reporting anything the programmer wrote.
    let test = CompileTest::new();
    let source = r#"
struct V { x: i32 }
func main() -> i32 {
    val a = V { x: 1 }
    val b = V { x: 2 }
    if a == b { return 1 }
    return 0
}
"#;
    let path = test.write_source("eq_without_impl.nr", source);
    let message = test
        .compile(&path)
        .expect_err("`==` on a struct with no PartialEq must be rejected");
    assert!(
        message.contains("PartialEq"),
        "the diagnostic must name the missing trait: {message}"
    );
    assert!(
        !message.contains("IntValue"),
        "the backend must never see the program: {message}"
    );
}

#[test]
fn equality_on_an_aggregate_is_rejected() {
    // Arrays, tuples and enums have no built-in equality; an enum gets `==` only from an
    // `impl PartialEq`.
    for (name, source) in [
        (
            "eq_array.nr",
            "func main() -> i32 {\n    val a = [1, 2]\n    val b = [1, 2]\n    if a == b { return 1 }\n    return 0\n}\n",
        ),
        (
            "eq_tuple.nr",
            "func main() -> i32 {\n    val a = (1, 2)\n    val b = (1, 2)\n    if a == b { return 1 }\n    return 0\n}\n",
        ),
        (
            "eq_enum.nr",
            "enum E { A, B }\nfunc main() -> i32 {\n    val a = E::A\n    val b = E::B\n    if a == b { return 1 }\n    return 0\n}\n",
        ),
    ] {
        let test = CompileTest::new();
        let path = test.write_source(name, source);
        let message = test.compile(&path).expect_err("expected a rejection");
        assert!(
            message.contains("cannot apply binary operator =="),
            "{name} must be rejected by the type checker: {message}"
        );
    }
}

#[test]
fn a_generic_body_comparing_an_aggregate_is_rejected_at_its_instantiation() {
    // A generic body is checked once as a template, so `a == b` on `T` is only known to be
    // unsupported when `T` arrives; lowering the instantiation is where that is caught.
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct V { x: i32 }
func same<T>(a: T, b: T) -> bool { return a == b }
func main() -> i32 {
    if same(V { x: 1 }, V { x: 1 }) { return 0 }
    return 1
}
"#;
    let path = test.write_source("eq_generic_instance.nr", source);
    let message = test.compile(&path).expect_err("expected a rejection");
    assert!(
        message.contains("cannot apply binary operator =="),
        "the instantiation must be rejected: {message}"
    );
    assert!(
        !message.contains("IntValue"),
        "the backend must never be asked to build it: {message}"
    );
}

/// A comparison method borrows its right operand, and a borrow needs storage. A right
/// operand that is not a variable reached codegen as a borrow of a temporary and was an
/// internal compiler error.
#[test]
fn regression_bug_081_a_comparison_against_a_temporary_runs() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct P { x: i32 }
impl PartialEq for P {
    func eq(&self, other: &P) -> bool { return self.x == other.x }
}
impl Comparable for P {
    func lt(&self, other: &P) -> bool { return self.x < other.x }
    func le(&self, other: &P) -> bool { return self.x <= other.x }
    func gt(&self, other: &P) -> bool { return self.x > other.x }
    func ge(&self, other: &P) -> bool { return self.x >= other.x }
}
struct W { p: P }
enum Light { Red, Green }
impl PartialEq for Light {
    func eq(&self, other: &Light) -> bool {
        match self {
            Light::Red => match other { Light::Red => true, Light::Green => false },
            Light::Green => match other { Light::Red => false, Light::Green => true }
        }
    }
}
func mk(x: i32) -> P { P { x: x } }
func main() -> i32 {
    val p = P { x: 1 }
    val arr = [P { x: 1 }, P { x: 2 }]
    val w = W { p: P { x: 2 } }
    mut n = 0
    if p == (P { x: 1 }) { n += 1 }
    if p == arr[0] { n += 2 }
    if mk(2) == w.p { n += 4 }
    if mk(0) < mk(1) { n += 8 }
    if p >= arr[1] { n += 100 }
    val l = Light::Green
    if l == Light::Green { n += 16 }
    n
}
"#;
    let exit = test
        .compile_and_run("operator_temporaries.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 31);
}

#[test]
fn bitwise_complement_dispatches_through_bit_not() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Mask { bits: u8 }
impl BitNot for Mask { type Output = Mask
    func bitnot(self) -> Mask { Mask { bits: ~self.bits } } }
func main() -> i32 {
    val m = ~Mask { bits: 15u8 }   // 0b1111_0000
    m.bits as i32
}
"#;
    let exit = test
        .compile_and_run("op_bitnot.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 240);
}

#[test]
fn a_trait_named_not_does_not_overload_the_complement() {
    // `~` is `BitNot`, and `!` is never overloadable, so `Not` is no operator trait.
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Mask { bits: u8 }
impl Not for Mask { type Output = Mask
    func not(self) -> Mask { self } }
func main() -> i32 {
    val m = ~Mask { bits: 15u8 }
    m.bits as i32
}
"#;
    let err = test
        .compile(&test.write_source("op_not.nr", source))
        .expect_err("`impl Not` must not provide `~`");
    assert!(err.contains("unknown trait 'Not'"), "{err}");
}

/// A non-`Copy` type, the shared opening of the ownership tests. Its `Vec` makes a double
/// free or a lost move observable at run time, not only to the checker.
const LIST: &str = r#"
struct List { items: Vec<i32> }
func list(first: i32) -> List {
    mut items: Vec<i32> = Vec::new()
    items.push(first)
    List { items: items }
}
impl List {
    func total(&self) -> i32 {
        mut total = 0
        for x in &self.items { total = total + *x }
        total
    }
}
func sum(l: &List) -> i32 { l.total() }
struct Holder { l: List }
impl Add for List { type Output = List
    func add(self, rhs: List) -> List {
        mut items = self.items
        for x in rhs.items { items.push(x) }
        List { items: items }
    }
}
impl Mul<&List> for List { type Output = List
    func mul(self, rhs: &List) -> List {
        mut items = self.items
        items.push(sum(rhs))
        List { items: items }
    }
}
impl Neg for List { type Output = i32
    func neg(self) -> i32 { 0 - sum(&self) } }
impl AddAssign<i32> for List {
    func add_assign(&mut self, rhs: i32) { self.items.push(rhs) } }
impl SubAssign for List {
    func sub_assign(&mut self, rhs: List) { self.items.push(0 - sum(&rhs)) } }
impl BitOrAssign<&List> for List {
    func bitor_assign(&mut self, rhs: &List) { self.items.push(sum(rhs)) } }
impl PartialEq for List {
    func eq(&self, rhs: &List) -> bool { self.total() == rhs.total() } }
impl Comparable for List {
    func lt(&self, rhs: &List) -> bool { self.total() < rhs.total() } }
func take(l: List) -> i32 { sum(&l) }
"#;

fn run_list(name: &str, body: &str) -> i32 {
    let test = CompileTest::new();
    test.compile_and_run(
        name,
        &format!("{LIST}\nfunc main() -> i32 {{\n{body}\n}}\n"),
    )
    .expect("compile/run failed")
}

fn list_error(name: &str, body: &str) -> String {
    let test = CompileTest::new();
    let source = format!("{LIST}\nfunc main() -> i32 {{\n{body}\n}}\n");
    test.compile(&test.write_source(name, &source))
        .expect_err("the program must be rejected")
}

#[test]
fn by_value_operators_move_non_copy_operands() {
    let exit = run_list(
        "op_move.nr",
        "val a = list(1)\nval b = list(2)\nval c = a + b\nval d = c + list(3)\n-d + 10",
    );
    assert_eq!(exit, 4);
}

#[test]
fn a_moved_operand_cannot_be_used_again() {
    for (name, body) in [
        (
            "op_moved_left.nr",
            "val a = list(1)\nval b = a + list(2)\ntake(a)",
        ),
        (
            "op_moved_right.nr",
            "val a = list(1)\nval b = list(2) + a\ntake(a)",
        ),
        (
            "op_moved_both.nr",
            "val a = list(1)\nval b = a + a\ntake(b)",
        ),
        ("op_moved_unary.nr", "val a = list(1)\nval b = -a\ntake(a)"),
    ] {
        let err = list_error(name, body);
        assert!(err.contains("use of moved value 'a'"), "{name}: {err}");
    }
}

#[test]
fn a_borrowed_non_copy_operand_cannot_be_moved_from() {
    for (name, body) in [
        (
            "op_borrow_left.nr",
            "val a = list(1)\nval r = &a\nval b = r + list(2)\n0",
        ),
        ("op_borrow_unary.nr", "val a = list(1)\nval r = &a\n-r"),
    ] {
        let err = list_error(name, body);
        assert!(err.contains("cannot move out of 'r'"), "{name}: {err}");
    }
}

#[test]
fn a_reference_rhs_leaves_the_right_operand_with_its_owner() {
    let exit = run_list(
        "op_ref_rhs.nr",
        "val a = list(1)\nval b = list(2)\nval c = a * &b\nsum(&c) + sum(&b)",
    );
    assert_eq!(exit, 5);
}

#[test]
fn the_right_operand_follows_the_method_signature() {
    // A borrow passed where the impl takes a value used to reach the backend as a call of
    // the wrong type.
    let err = list_error(
        "op_rhs_bare.nr",
        "val a = list(1)\nval b = list(2)\nval c = a * b\n0",
    );
    assert!(err.contains("type mismatch"), "{err}");
    let err = list_error(
        "op_rhs_ref.nr",
        "val a = list(1)\nval b = list(2)\nval c = a + &b\n0",
    );
    assert!(err.contains("type mismatch"), "{err}");
}

#[test]
fn a_literal_right_operand_takes_the_rhs_type() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Scale { k: f32 }
impl Mul<f32> for Scale { type Output = Scale
    func mul(self, rhs: f32) -> Scale { Scale { k: self.k * rhs } } }
impl PartialEq<i32> for Scale {
    func eq(&self, rhs: &i32) -> bool { self.k as i32 == *rhs } }
func main() -> i32 {
    val s = Scale { k: 2.0f32 } * 3.0
    val six = 6
    val r = &six
    mut score = 0
    if s == 6 { score = score + 1 }
    if s == six { score = score + 2 }
    if s == r { score = score + 4 }
    if s != 7 { score = score + 8 }
    score
}
"#;
    let exit = test
        .compile_and_run("op_literal_rhs.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 15);
}

#[test]
fn a_comparison_only_reads_its_operands() {
    let exit = run_list(
        "op_compare_reads.nr",
        r#"val a = list(1)
val b = list(2)
mut score = 0
if a == list(1) { score = score + 1 }
if list(2) == b { score = score + 2 }
if a != b { score = score + 4 }
if a < &b { score = score + 8 }
score + take(a) + take(b) - 3"#,
    );
    assert_eq!(exit, 15);
}

#[test]
fn a_compared_place_stays_with_its_owner() {
    // The operand needs storage to borrow, and that storage must be the place itself: a
    // local holding the value would be a second owner, freeing the element's buffer.
    let exit = run_list(
        "op_compare_places.nr",
        r#"mut lists: Vec<List> = Vec::new()
lists.push(list(1))
lists.push(list(2))
val h = Holder { l: list(2) }
mut score = 0
if lists[0] < lists[1] { score = score + 1 }
if lists[1] == h.l { score = score + 2 }
if h.l >= lists[1] { score = score + 4 }
{
    val r = &lists[0]
    if *r != lists[1] { score = score + 8 }
    if lists[1] != *r { score = score + 16 }
}
lists[0] += 1
score + sum(&lists[0]) + sum(&lists[1]) + take(h.l)"#,
    );
    assert_eq!(exit, 37);
}

#[test]
fn provided_comparison_methods_are_inherited() {
    let exit = run_list(
        "op_inherited.nr",
        r#"val a = list(1)
val b = list(2)
mut score = 0
if a != b { score = score + 1 }
if a <= b { score = score + 2 }
if a <= a { score = score + 4 }
if b > a { score = score + 8 }
if a > a { score = score + 100 }
if b >= a { score = score + 16 }
if a >= b { score = score + 100 }
if a.ne(&b) { score = score + 32 }
score"#,
    );
    assert_eq!(exit, 63);
}

#[test]
fn a_written_comparison_method_overrides_the_provided_one() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Odd { n: i32 }
impl PartialEq for Odd {
    func eq(&self, rhs: &Odd) -> bool { self.n == rhs.n }
    func ne(&self, rhs: &Odd) -> bool { true } }
func main() -> i32 {
    val a = Odd { n: 1 }
    if a != a { 1 } else { 0 }
}
"#;
    let exit = test
        .compile_and_run("op_override.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 1);
}

#[test]
fn provided_methods_reach_a_derived_partial_eq_and_a_builtin() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone, PartialEq)
struct Grade { n: i32 }
impl Comparable for Grade {
    func lt(&self, rhs: &Grade) -> bool { self.n < rhs.n } }
func main() -> i32 {
    val a = Grade { n: 1 }
    val b = Grade { n: 2 }
    mut score = 0
    if a <= a { score = score + 1 }
    if a.ne(&b) { score = score + 2 }
    if 3.lt(&4) { score = score + 4 }
    if 3.ge(&4) { score = score + 100 }
    score
}
"#;
    let exit = test
        .compile_and_run("op_derived_provided.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 7);
}

#[test]
fn comparison_operators_reach_a_comparable_parameter() {
    let test = CompileTest::new();
    let source = r#"
func smaller<T: Comparable>(a: T, b: T) -> T {
    if a < b { a } else { b }
}
func strictly<T: Comparable>(a: &T, b: &T) -> bool { a.le(b) && a.ne(b) }
@derive(Copy, Clone)
struct Rank { n: i32 }
impl PartialEq for Rank {
    func eq(&self, rhs: &Rank) -> bool { self.n == rhs.n } }
impl Comparable for Rank {
    func lt(&self, rhs: &Rank) -> bool { self.n < rhs.n } }
func main() -> i32 {
    val a = Rank { n: 4 }
    val b = Rank { n: 3 }
    mut score = smaller(a, b).n + smaller(10, 20)
    if strictly(&b, &a) { score = score + 100 }
    if strictly(&a, &a) { score = score + 100 }
    score
}
"#;
    let exit = test
        .compile_and_run("op_bound_compare.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 113);
}

#[test]
fn a_float_does_not_meet_a_comparable_bound() {
    let test = CompileTest::new();
    let source = r#"
func smaller<T: Comparable>(a: T, b: T) -> T {
    if a < b { a } else { b }
}
func main() -> i32 {
    val x = smaller(1.5, 2.5)
    0
}
"#;
    let err = test
        .compile(&test.write_source("op_float_bound.nr", source))
        .expect_err("no float is `Comparable`");
    assert!(
        err.contains("does not implement required trait 'Comparable'"),
        "{err}"
    );
}

#[test]
fn a_missing_required_operator_method_is_reported() {
    let test = CompileTest::new();
    let source = r#"
struct M { n: i32 }
impl PartialEq for M {
    func ne(&self, rhs: &M) -> bool { true } }
impl Comparable for M {
    func le(&self, rhs: &M) -> bool { true } }
impl AddAssign for M { }
func main() -> i32 { 0 }
"#;
    let err = test
        .compile(&test.write_source("op_missing.nr", source))
        .expect_err("required methods are required");
    for method in ["eq", "lt", "add_assign"] {
        assert!(
            err.contains(&format!("missing required method '{method}'")),
            "{err}"
        );
    }
}

#[test]
fn an_operator_method_of_the_wrong_shape_is_rejected() {
    for (name, item, detail) in [
        (
            "op_shape_ref.nr",
            "impl Add for M { type Output = M\n func add(&self, rhs: M) -> M { rhs } }",
            "receiver",
        ),
        (
            "op_shape_owned.nr",
            "impl AddAssign for M { func add_assign(self, rhs: M) { } }",
            "receiver",
        ),
        (
            "op_shape_ret.nr",
            "impl SubAssign for M { func sub_assign(&mut self, rhs: M) -> i32 { 1 } }",
            "returns nothing",
        ),
        (
            "op_shape_rhs.nr",
            "impl PartialEq for M { func eq(&self, rhs: M) -> bool { true } }",
            "borrows its right operand",
        ),
        (
            "op_shape_bool.nr",
            "impl PartialEq for M { func eq(&self, rhs: &M) -> i32 { 1 } }",
            "returns bool",
        ),
        (
            "op_shape_arity.nr",
            "impl Neg for M { type Output = M\n func neg(self, x: i32) -> M { self } }",
            "expected 0 parameter(s)",
        ),
    ] {
        let test = CompileTest::new();
        let source = format!("struct M {{ n: i32 }}\n{item}\nfunc main() -> i32 {{ 0 }}\n");
        let err = test
            .compile(&test.write_source(name, &source))
            .expect_err("the impl must be rejected");
        assert!(err.contains(detail), "{name}: {err}");
    }
}

#[test]
fn a_user_assign_impl_updates_every_place_form_in_place() {
    let exit = run_list(
        "op_assign_places.nr",
        r#"mut a = list(1)
a += 2
a -= list(3)
val other = list(4)
a |= &other
mut h = Holder { l: list(10) }
h.l += 5
mut lists: Vec<List> = Vec::new()
lists.push(list(20))
lists[0] += 7
mut pair = (list(30), 0)
pair.0 += 8
{
    val r = &mut a
    *r += 100
}
sum(&a) + sum(&h.l) + sum(&lists[0]) + sum(&pair.0) + sum(&other)"#,
    );
    assert_eq!(exit, 188);
}

#[test]
fn every_assign_trait_is_reached_by_its_operator() {
    let test = CompileTest::new();
    let mut items = String::new();
    let mut updates = String::new();
    for (index, (name, method, op)) in [
        ("AddAssign", "add_assign", "+="),
        ("SubAssign", "sub_assign", "-="),
        ("MulAssign", "mul_assign", "*="),
        ("DivAssign", "div_assign", "/="),
        ("RemAssign", "rem_assign", "%="),
        ("BitAndAssign", "bitand_assign", "&="),
        ("BitOrAssign", "bitor_assign", "|="),
        ("BitXorAssign", "bitxor_assign", "^="),
        ("ShlAssign", "shl_assign", "<<="),
    ]
    .into_iter()
    .enumerate()
    {
        let bit = 1 << index;
        items += &format!(
            "impl {name}<i32> for Mask {{ func {method}(&mut self, rhs: i32) {{ self.seen = self.seen + {bit} }} }}\n"
        );
        updates += &format!("m {op} 1\n");
    }
    let source = format!(
        "struct Mask {{ seen: i32 }}\n{items}func main() -> i32 {{\nmut m = Mask {{ seen: 0 }}\n{updates}m.seen - 256\n}}\n"
    );
    let exit = test
        .compile_and_run("op_assign_family.nr", &source)
        .expect("compile/run failed");
    assert_eq!(exit, 255);
}

#[test]
fn an_assign_impl_is_preferred_over_the_by_value_operator() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct V { x: i32 }
impl Add for V { type Output = V
    func add(self, rhs: V) -> V { V { x: 100 } } }
impl AddAssign for V {
    func add_assign(&mut self, rhs: V) { self.x = self.x + rhs.x } }
func main() -> i32 {
    mut a = V { x: 1 }
    a += V { x: 2 }
    val b = a + a
    a.x + b.x
}
"#;
    let exit = test
        .compile_and_run("op_assign_preferred.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 103);
}

#[test]
fn the_value_of_an_assign_is_evaluated_before_the_place() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct V { x: i32 }
impl AddAssign for V {
    func add_assign(&mut self, rhs: V) { self.x = self.x + rhs.x } }
impl V { func doubled(&self) -> V { V { x: self.x * 2 } } }
func index(log: &mut Vec<i32>) -> i32 {
    log.push(1)
    0
}
func value(log: &mut Vec<i32>) -> V {
    log.push(2)
    V { x: 1 }
}
func main() -> i32 {
    mut a = V { x: 3 }
    a += a.doubled()
    a += a
    mut log: Vec<i32> = Vec::new()
    mut cells = [V { x: 0 }]
    cells[index(&mut log)] += value(&mut log)
    a.x * 10 + log[0]
}
"#;
    let exit = test
        .compile_and_run("op_assign_order.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 182);
}

#[test]
fn a_non_copy_place_without_an_assign_impl_takes_the_desugaring() {
    let exit = run_list(
        "op_desugar_move.nr",
        "mut t = Holder { l: list(1) }\nt.l = t.l + list(2)\nmut i = 0\nmut a = list(1)\nwhile i < 3 {\n a = a + list(i)\n i = i + 1\n}\nsum(&a) + sum(&t.l)",
    );
    assert_eq!(exit, 7);
}

#[test]
fn an_assign_place_is_checked_like_a_receiver() {
    for (name, body, message) in [
        (
            "op_assign_immutable.nr",
            "val a = list(1)\na += 1\n0",
            "immutable",
        ),
        (
            "op_assign_moved.nr",
            "mut a = list(1)\ntake(a)\na += 1\n0",
            "use of moved value 'a'",
        ),
        (
            "op_assign_self.nr",
            "mut a = list(1)\na -= a\n0",
            "use of moved value 'a'",
        ),
        (
            "op_assign_borrowed.nr",
            "mut a = list(1)\nval r = &a\na += 1\nsum(r)",
            "already borrowed",
        ),
        (
            "op_assign_through_shared.nr",
            "mut a = list(1)\nval r = &a\n*r += 1\n0",
            "immutable",
        ),
        (
            "op_assign_rhs.nr",
            "mut a = list(1)\na += list(2)\n0",
            "type mismatch",
        ),
    ] {
        let err = list_error(name, body);
        assert!(err.contains(message), "{name}: {err}");
    }
}

#[test]
fn operators_reach_generic_instances_enums_and_newtypes() {
    let test = CompileTest::new();
    let source = r#"
struct Wrap<T> { x: T }
impl<T> Add for Wrap<T> { type Output = Wrap<T>
    func add(self, rhs: Wrap<T>) -> Wrap<T> { rhs } }
impl<T> PartialEq for Wrap<T> {
    func eq(&self, rhs: &Wrap<T>) -> bool { true } }
impl<T> AddAssign<i32> for Wrap<T> {
    func add_assign(&mut self, rhs: i32) { } }
impl<T> Neg for Wrap<T> { type Output = i32
    func neg(self) -> i32 { 9 } }

enum Bag { Full(Vec<i32>), Empty }
impl Add for Bag { type Output = i32
    func add(self, rhs: Bag) -> i32 {
        match self {
            Bag::Full(v) => v[0],
            Bag::Empty => 2
        }
    }
}
impl MulAssign for Bag {
    func mul_assign(&mut self, rhs: Bag) { } }

newtype Meters = i32
impl SubAssign for Meters {
    func sub_assign(&mut self, rhs: Meters) { } }

func bag() -> Bag {
    mut v: Vec<i32> = Vec::new()
    v.push(5)
    Bag::Full(v)
}

func main() -> i32 {
    val a = Wrap { x: 1 }
    val b = Wrap { x: 2 }
    mut c = a + b
    c += 4
    mut score = c.x
    val d = Wrap { x: 7 }
    if c != d { score = score + 100 }
    if c == d { score = score + 10 }
    score = score + -d

    val first = bag()
    mut second = Bag::Empty
    second *= bag()
    score = score + (first + second)

    mut m = Meters(1)
    m -= Meters(1)
    score
}
"#;
    let exit = test
        .compile_and_run("op_every_type.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 26);
}

#[test]
fn an_assign_inside_a_closure_captures_its_place_exclusively() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct V { x: i32 }
impl AddAssign<i32> for V {
    func add_assign(&mut self, rhs: i32) { self.x = self.x + rhs } }
func main() -> i32 {
    mut a = V { x: 3 }
    {
        mut bump = |d: i32| { a += d }
        bump(1)
        bump(2)
    }
    a.x
}
"#;
    let exit = test
        .compile_and_run("op_assign_closure.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 6);
}
