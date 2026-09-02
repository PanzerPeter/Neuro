#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn trait_impl_and_default_method_type_check() {
    // A trait with a required and a default method; the impl provides only the
    // required one and inherits the default. Dispatching both must type-check.
    let errors = semantic_errors(
        r#"
trait Describable {
    func value(&self) -> i32
    func doubled(&self) -> i32 { self.value() * 2 }
}

struct Widget { id: i32 }

impl Describable for Widget {
    func value(&self) -> i32 { self.id }
}

func main() -> i32 {
    val w = Widget { id: 21 }
    w.doubled()
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a conforming trait impl with an inherited default must type-check; got {errors:?}"
    );
}

#[test]
fn missing_required_trait_method_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Shape { func area(&self) -> i32 }
struct S { x: i32 }
impl Shape for S { }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MissingTraitMethod { .. })),
        "an impl omitting a required method must be rejected; got {errors:?}"
    );
}

#[test]
fn unknown_trait_in_impl_is_rejected() {
    let errors = semantic_errors(
        r#"
struct S { x: i32 }
impl Bogus for S { func f(&self) -> i32 { self.x } }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownTrait { .. })),
        "implementing an undeclared trait must be rejected; got {errors:?}"
    );
}

#[test]
fn trait_method_signature_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Shape { func area(&self) -> i32 }
struct S { x: i32 }
impl Shape for S { func area(&self) -> i64 { 0i64 } }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TraitMethodSignatureMismatch { .. })),
        "a mismatched trait-method signature must be rejected; got {errors:?}"
    );
}

#[test]
fn generic_trait_bound_dispatch_type_checks() {
    // A generic function bounded by a trait may call the trait's methods on the
    // type parameter, and the call site satisfies the bound.
    let errors = semantic_errors(
        r#"
trait Shape { func area(&self) -> i32 }
@derive(Copy)
struct Square { side: i32 }
impl Shape for Square { func area(&self) -> i32 { self.side * self.side } }
func total<T: Shape>(s: &T) -> i32 { s.area() }
func main() -> i32 {
    val sq = Square { side: 5 }
    total(&sq)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a satisfied trait bound with in-body dispatch must type-check; got {errors:?}"
    );
}

#[test]
fn unsatisfied_trait_bound_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Shape { func area(&self) -> i32 }
@derive(Copy)
struct Plain { x: i32 }
func total<T: Shape>(s: &T) -> i32 { s.area() }
func main() -> i32 {
    val p = Plain { x: 1 }
    total(&p)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TraitBoundNotSatisfied { .. })),
        "calling a bounded generic with a non-implementing type must be rejected; got {errors:?}"
    );
}

// ---- Operator traits ----

/// A `Vec2` with the arithmetic, unary, and comparison operator impls used below.
const VEC2_OPS: &str = r#"
@derive(Copy, Clone)
struct Vec2 { x: i32, y: i32 }
impl Add for Vec2 { type Output = Vec2
    func add(self, rhs: Vec2) -> Vec2 { Vec2 { x: self.x + rhs.x, y: self.y + rhs.y } } }
impl Neg for Vec2 { type Output = Vec2
    func neg(self) -> Vec2 { Vec2 { x: -self.x, y: -self.y } } }
impl PartialEq for Vec2 {
    func eq(&self, rhs: &Vec2) -> bool { self.x == rhs.x && self.y == rhs.y }
    func ne(&self, rhs: &Vec2) -> bool { self.x != rhs.x || self.y != rhs.y } }
"#;

#[test]
fn operator_traits_dispatch_on_user_type() {
    let src = format!(
        "{VEC2_OPS}\nfunc main() -> i32 {{\n  val a = Vec2 {{ x: 1, y: 2 }}\n  val b = Vec2 {{ x: 3, y: 4 }}\n  val c = a + b\n  val d = -c\n  if a == b {{ return 1 }}\n  if a != b {{ return c.x + d.x }}\n  0\n}}"
    );
    assert!(
        semantic_errors(&src).is_empty(),
        "operator-trait dispatch on a Copy struct must type-check: {:?}",
        semantic_errors(&src)
    );
}

#[test]
fn operator_trait_impl_on_non_copy_is_rejected() {
    let errors = semantic_errors(
        r#"
struct NC { v: i32 }
impl Add for NC { type Output = NC
    func add(self, rhs: NC) -> NC { NC { v: self.v + rhs.v } } }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::OperatorTraitRequiresCopy { .. })),
        "an operator impl on a non-Copy struct must be rejected; got {errors:?}"
    );
}

#[test]
fn comparable_without_partialeq_is_rejected() {
    let errors = semantic_errors(
        r#"
@derive(Copy, Clone)
struct M { v: i32 }
impl Comparable for M {
    func lt(&self, rhs: &M) -> bool { self.v < rhs.v }
    func le(&self, rhs: &M) -> bool { self.v <= rhs.v }
    func gt(&self, rhs: &M) -> bool { self.v > rhs.v }
    func ge(&self, rhs: &M) -> bool { self.v >= rhs.v } }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MissingSupertraitImpl { .. })),
        "Comparable without PartialEq must be rejected; got {errors:?}"
    );
}

#[test]
fn associated_output_mismatch_is_rejected() {
    let errors = semantic_errors(
        r#"
@derive(Copy, Clone)
struct V { v: i32 }
impl Add for V { type Output = bool
    func add(self, rhs: V) -> V { V { v: self.v + rhs.v } } }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssociatedTypeMismatch { .. })),
        "a `type Output` not matching the method return must be rejected; got {errors:?}"
    );
}

#[test]
fn operator_without_impl_is_still_rejected() {
    let errors = semantic_errors(
        r#"
@derive(Copy, Clone)
struct P { v: i32 }
func main() -> i32 {
    val a = P { v: 1 }
    val b = P { v: 2 }
    val c = a + b
    0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })),
        "using `+` on a struct without an Add impl must be rejected; got {errors:?}"
    );
}

#[test]
fn associated_type_binding_resolves_self_paths() {
    // The iterator shape: the trait names `Item`, the impl says what it is, and both
    // signatures spell the position `Self::Item`.
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func main() -> i32 {
    val c = Counter { n: 7 }
    c.first()
}
"#,
    );
    assert!(
        errors.is_empty(),
        "an impl binding its associated type must type-check; got {errors:?}"
    );
}

#[test]
fn an_impl_may_spell_the_associated_position_concretely() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> i32 { self.n }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors.is_empty(),
        "the binding's type and `Self::Item` name the same type; got {errors:?}"
    );
}

#[test]
fn an_impl_must_bind_every_declared_associated_type() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> i32
}

struct Counter { n: i32 }

impl Source for Counter {
    func first(&self) -> i32 { self.n }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MissingAssociatedType { .. })),
        "an unbound associated type must be rejected; got {errors:?}"
    );
}

#[test]
fn an_impl_may_not_bind_an_undeclared_associated_type() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32
    type Extra = bool

    func first(&self) -> Self::Item { self.n }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownAssociatedType { .. })),
        "a binding the trait never declared must be rejected; got {errors:?}"
    );
}

#[test]
fn a_signature_disagreeing_with_the_bound_associated_type_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> bool { true }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TraitMethodSignatureMismatch { .. })),
        "the trait's `Self::Item` is this impl's `i32`; got {errors:?}"
    );
}

#[test]
fn a_self_path_outside_an_impl_is_rejected() {
    let errors = semantic_errors(
        r#"
func first() -> Self::Item { 1 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnboundAssociatedType { .. })),
        "`Self::Item` with no implementing type must be rejected; got {errors:?}"
    );
}

#[test]
fn a_trait_with_an_associated_type_is_not_object_safe() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func take(s: &dyn Source) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TraitNotObjectSafe { .. })),
        "a trait object leaves the associated type unspecified; got {errors:?}"
    );
}

#[test]
fn dispatching_an_associated_signature_through_a_bound_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func read<T: Source>(s: &T) -> i32 {
    val v = s.first()
    0
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnconstrainedAssociatedType { .. })),
        "a bare `T: Source` bound does not say what `Self::Item` is; got {errors:?}"
    );
}

#[test]
fn a_constrained_bound_types_an_associated_signature() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func read<T: Source<Item = i32>>(s: &T) -> i32 {
    s.first()
}

func main() -> i32 {
    val c = Counter { n: 4 }
    read(&c)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "`Source<Item = i32>` says what `Self::Item` is; got {errors:?}"
    );
}

#[test]
fn a_constrained_bound_types_a_nested_associated_position() {
    // The binding has to reach a nested position, not only a bare annotation.
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func pair(&self) -> (Self::Item, i32)
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func pair(&self) -> (Self::Item, i32) { (self.n, 1) }
}

func read<T: Source<Item = i32>>(s: &T) -> i32 {
    val p = s.pair()
    p.0 + p.1
}

func main() -> i32 {
    val c = Counter { n: 4 }
    read(&c)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the binding resolves `(Self::Item, i32)` to `(i32, i32)`; got {errors:?}"
    );
}

#[test]
fn a_type_argument_binding_a_different_associated_type_is_rejected() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Tally { n: f64 }

impl Source for Tally {
    type Item = f64

    func first(&self) -> Self::Item { self.n }
}

func read<T: Source<Item = i32>>(s: &T) -> i32 {
    0
}

func main() -> i32 {
    val t = Tally { n: 1.0 }
    read(&t)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssociatedTypeBoundMismatch { .. })),
        "`Tally` binds `Item` to f64, which the bound forbids; got {errors:?}"
    );
}

#[test]
fn a_bound_may_only_constrain_a_declared_associated_type() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

func read<T: Source<Bogus = i32>>(s: &T) -> i32 { 0 }

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownAssociatedType { .. })),
        "`Bogus` is not a member of `Source`; got {errors:?}"
    );
}

#[test]
fn a_constraint_carries_through_a_second_bounded_parameter() {
    // The inner call has no impl to read — its argument is the outer parameter — so the
    // outer bound's own constraint is what must answer for it.
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Counter { n: i32 }

impl Source for Counter {
    type Item = i32

    func first(&self) -> Self::Item { self.n }
}

func read<T: Source<Item = i32>>(s: &T) -> i32 { s.first() }

func relay<U: Source<Item = i32>>(s: &U) -> i32 { read(s) }

func main() -> i32 {
    val c = Counter { n: 4 }
    relay(&c)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "both bounds constrain `Item` to i32; got {errors:?}"
    );
}

#[test]
fn a_return_position_constraint_must_match_the_concrete_impl() {
    let errors = semantic_errors(
        r#"
trait Source {
    type Item

    func first(&self) -> Self::Item
}

@derive(Copy)
struct Tally { n: f64 }

impl Source for Tally {
    type Item = f64

    func first(&self) -> Self::Item { self.n }
}

func make() -> impl Source<Item = i32> {
    Tally { n: 1.0 }
}

func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::AssociatedTypeBoundMismatch { .. })),
        "`-> impl Source<Item = i32>` promises what `Tally` does not bind; got {errors:?}"
    );
}
