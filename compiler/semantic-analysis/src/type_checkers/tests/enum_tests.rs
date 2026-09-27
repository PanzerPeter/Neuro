#[allow(unused_imports)]
use super::{make_function, make_ident, make_type, semantic_errors};
use crate::errors::TypeError;

#[test]
fn generic_enum_tuple_variant_infers_its_type_argument() {
    // `Opt::Some(7)` determines `T = i32` from its payload, and the match arm binds the
    // payload at that concrete type, so `v + 1` is i32 arithmetic.
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val o = Opt::Some(7)
    match o {
        Opt::Some(v) => v + 1,
        Opt::None => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_unit_variant_takes_its_arguments_from_the_annotation() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val none: Opt<i32> = Opt::None
    match none {
        Opt::Some(v) => v,
        Opt::None => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_partial_payload_completes_from_the_return_type() {
    // `Res::Err(1)` binds only `E`; `T` comes from the declared return type, which is
    // the only context a tail `if` branch has.
    let errors = semantic_errors(
        r#"
enum Res<T, E> { Ok(T), Err(E) }
func divide(a: i32, b: i32) -> Res<i32, i32> {
    if b == 0 { Res::Err(1) } else { Res::Ok(a / b) }
}
func main() -> i32 {
    match divide(9, 3) {
        Res::Ok(v) => v,
        Res::Err(e) => e
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn generic_enum_unit_variant_without_context_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val x = Opt::None
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericEnumNotInferable { .. })),
        "a unit variant with nothing to infer from must be rejected; got {errors:?}"
    );
}

#[test]
fn bare_generic_enum_without_arguments_is_rejected() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func take(o: Opt) -> i32 { 0 }
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::GenericEnumNeedsArgs { .. })),
        "a generic enum used without type arguments must be rejected; got {errors:?}"
    );
}

#[test]
fn non_copy_generic_enum_payload_is_accepted() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func main() -> i32 {
    val s: Opt<string> = Opt::Some("held")
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a monomorphized payload may be non-Copy; got {errors:?}"
    );
}

#[test]
fn generic_enum_instances_are_distinct_types() {
    let errors = semantic_errors(
        r#"
enum Opt<T> { Some(T), None }
func take(o: Opt<i64>) -> i32 { 0 }
func main() -> i32 {
    val narrow: Opt<i32> = Opt::None
    return take(narrow)
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "two instances of one generic enum must not be interchangeable; got {errors:?}"
    );
}

#[test]
fn generic_enum_struct_variant_infers_from_its_fields() {
    let errors = semantic_errors(
        r#"
enum Shape<T> { Circle { radius: T }, Empty }
func main() -> i32 {
    val c = Shape::Circle { radius: 2 }
    match c {
        Shape::Circle { radius } => radius,
        Shape::Empty => 0
    }
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn an_enum_takes_an_inherent_impl() {
    // Every receiver form and an associated function, which a path call reaches instead of
    // a variant construction because no variant shares its name.
    let errors = semantic_errors(
        r#"
enum Light { Red, Green }
impl Light {
    func start() -> Light { Light::Red }
    func code(&self) -> i32 {
        match self {
            Light::Red => 1,
            Light::Green => 2
        }
    }
    func flip(&mut self) { self = Light::Green }
    func is_red(self) -> bool { self.code() == 1 }
}
func main() -> i32 {
    mut l = Light::start()
    l.flip()
    if l.is_red() { return 0 }
    l.code()
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn an_enum_trait_impl_satisfies_a_bound_and_a_trait_object() {
    let errors = semantic_errors(
        r#"
trait Code { func code(&self) -> i32 }
enum Light { Red, Green }
impl Code for Light {
    func code(&self) -> i32 {
        match self {
            Light::Red => 1,
            Light::Green => 2
        }
    }
}
func stat<T: Code>(x: T) -> i32 { x.code() }
func dynamic(x: &dyn Code) -> i32 { x.code() }
func main() -> i32 {
    val l = Light::Green
    stat(l) + dynamic(&l)
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn an_enum_operator_impl_gives_it_equality() {
    let errors = semantic_errors(
        r#"
enum Light { Red, Green }
impl Light {
    func rank(&self) -> i32 {
        match self {
            Light::Red => 0,
            Light::Green => 1
        }
    }
}
impl PartialEq for Light {
    func eq(&self, other: &Light) -> bool { self.rank() == other.rank() }
}
func main() -> i32 {
    val a = Light::Red
    val b = Light::Red
    if a == b { return 1 }
    0
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn a_generic_enum_impl_reaches_an_instance_declared_before_it() {
    // `Holder` names `Tree<i32>` in the declaration pass, before the impl is registered,
    // so the instance has to be given the impl's methods after the fact.
    let errors = semantic_errors(
        r#"
enum Tree<T> { Leaf(T), Empty }
struct Holder { t: Tree<i32> }
impl<T> Tree<T> {
    func is_leaf(&self) -> bool {
        match self {
            Tree::Leaf(_) => true,
            Tree::Empty => false
        }
    }
}
func main() -> i32 {
    val h = Holder { t: Tree::Leaf(7) }
    val f: Tree<f64> = Tree::Empty
    if h.t.is_leaf() && !f.is_leaf() { return 1 }
    0
}
"#,
    );
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

#[test]
fn an_impl_member_may_not_share_a_variant_name() {
    let source = r#"
enum Shape { Circle(f64), Dot }
impl Shape {
    func Circle(r: f64) -> Shape { Shape::Dot }
}
func main() -> i32 { 0 }
"#;
    let errors = semantic_errors(source);
    let at = source
        .find("Circle(r")
        .expect("the method name is in the source");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::ImplMemberNamesVariant { name, span, .. }
                if name == "Circle" && span.start == at
        )),
        "expected the clash at the method name; got {errors:?}"
    );
}

#[test]
fn an_enum_may_not_implement_drop() {
    // Only a struct binding runs a user destructor, so the impl would never be called.
    let errors = semantic_errors(
        r#"
enum Handle { Open(i32), Closed }
impl Drop for Handle {
    func drop(&mut self) { }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::InvalidDropImpl { type_name, .. } if type_name == "Handle")
        ),
        "expected the Drop impl on an enum to be refused; got {errors:?}"
    );
}

#[test]
fn a_method_an_enum_impl_does_not_declare_is_not_found() {
    let errors = semantic_errors(
        r#"
enum Light { Red, Green }
impl Light {
    func code(&self) -> i32 { 1 }
}
func main() -> i32 {
    val l = Light::Red
    l.missing()
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::MethodNotFound { method_name, .. } if method_name == "missing")),
        "expected an unknown method on the enum; got {errors:?}"
    );
}

fn moved(errors: &[TypeError]) -> bool {
    errors
        .iter()
        .any(|e| matches!(e, TypeError::UseOfMovedValue { .. }))
}

/// An enum holding an owner moves, as a tuple holding one does. Every enum used to be
/// treated as `Copy`, so a second use of one owning a `Vec` freed the buffer twice.
#[test]
fn regression_bug_082_an_enum_holding_an_owner_moves() {
    let errors = semantic_errors(
        r#"
enum Bag { Items(Vec<i32>), Empty }
func count(b: Bag) -> i32 { match b { Bag::Items(v) => v.len() as i32, Bag::Empty => 0 } }
func main() -> i32 {
    val b = Bag::Empty
    val a = count(b)
    return a + count(b)
}
"#,
    );
    assert!(moved(&errors), "an owning enum moves; got {errors:?}");
}

#[test]
fn an_enum_of_copy_payloads_still_copies() {
    let errors = semantic_errors(
        r#"
enum Pt { At(i32, i32), Nowhere }
func x(p: Pt) -> i32 { match p { Pt::At(a, _) => a, Pt::Nowhere => 0 } }
func main() -> i32 {
    val p = Pt::At(1, 2)
    return x(p) + x(p)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a Copy-payload enum copies; got {errors:?}"
    );
}

/// A payload is stored inline, so an enum that holds itself has no size. The backend's
/// layout recursed without end on one and overflowed the compiler's stack.
#[test]
fn regression_an_enum_that_holds_itself_is_refused() {
    for source in [
        "enum L { Cons(i32, L), Nil }\nfunc main() -> i32 { 0 }",
        "struct S { e: E }\nenum E { X(S), N }\nfunc main() -> i32 { 0 }",
        "enum A { X((i32, B)), N }\nenum B { Y([A; 2]), M }\nfunc main() -> i32 { 0 }",
    ] {
        let errors = semantic_errors(source);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::RecursiveEnum { .. })),
            "{source}: expected RecursiveEnum, got {errors:?}"
        );
    }
    let errors = semantic_errors("enum Ok1 { A(Vec<i32>), B(&i32) }\nfunc main() -> i32 { 0 }");
    assert!(
        errors.is_empty(),
        "indirection ends the cycle; got {errors:?}"
    );
}

/// `match self` in a `&self` method may read a payload but not take an owner out of
/// it: the value belongs to the caller. Binding the `Vec` by value freed it twice.
#[test]
fn regression_a_match_may_not_take_an_owner_out_of_a_borrow() {
    let errors = semantic_errors(
        r#"
enum Bag { Items(Vec<i32>), Empty }
impl Bag {
    func size(&self) -> i32 { match self { Bag::Items(v) => v.len() as i32, Bag::Empty => 0 } }
    func full(&self) -> bool { match self { Bag::Items(_) => true, Bag::Empty => false } }
}
func main() -> i32 { 0 }
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::CannotMoveOutOfBorrow { .. })),
        "binding the Vec out of `&self` is refused; got {errors:?}"
    );
    assert_eq!(errors.len(), 1, "`full` binds nothing; got {errors:?}");
}

/// A `match` on a borrowed enum tests the referent, as `match self` and `match *d` do.
#[test]
fn regression_a_match_reads_through_a_borrowed_enum() {
    let errors = semantic_errors(
        r#"
enum Dir { Up, Down(i32) }
func f(d: &Dir) -> i32 { match d { Dir::Up => 0, Dir::Down(n) => n } }
func main() -> i32 {
    val x = Dir::Down(7)
    return f(&x)
}
"#,
    );
    assert!(
        errors.is_empty(),
        "matching `&Dir` is matching `Dir`; got {errors:?}"
    );
}
