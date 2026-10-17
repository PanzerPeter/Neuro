// The derivable traits as declared traits: hand-written impls, bounds a generic body can
// use, and what the compiler supplies for builtins and derives.

use super::semantic_errors;
use crate::errors::TypeError;

/// The prelude's declarations of the four derivable traits it states as source. The unit
/// tests run the checker without the prelude, so each program carries them.
const TRAITS: &str = r#"
    trait Copy {}
    trait Clone { func clone(&self) -> Self }
    trait Debug { func fmt_debug(&self, out: &mut StringBuilder) }
    trait Hashable { func hash(&self) -> u64 }
"#;

fn errors_with_traits(source: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{TRAITS}{source}"))
}

fn assert_clean(source: &str) {
    let errors = errors_with_traits(source);
    assert!(errors.is_empty(), "expected no errors, got {errors:?}");
}

fn bound_failure(errors: &[TypeError], trait_name: &str) -> bool {
    errors.iter().any(
        |e| matches!(e, TypeError::TraitBoundNotSatisfied { trait_name: t, .. } if t == trait_name),
    )
}

#[test]
fn copy_bound_lets_the_body_copy() {
    assert_clean(
        r#"
        func dup<T: Copy>(v: T) -> (T, T) {
            val a = v
            val b = v
            (a, b)
        }
        @derive(Copy)
        struct P { x: i32 }
        func main() -> i32 {
            val (a, b) = dup(3)
            val (p, q) = dup(P { x: 1 })
            a + b + p.x + q.x
        }
        "#,
    );
}

#[test]
fn unbounded_parameter_still_moves() {
    let errors = errors_with_traits(
        r#"
        func dup<T>(v: T) -> (T, T) {
            val a = v
            val b = v
            (a, b)
        }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "got {errors:?}"
    );
}

#[test]
fn copy_bound_refuses_an_owning_argument() {
    let errors = errors_with_traits(
        r#"
        func dup<T: Copy>(v: T) -> (T, T) { (v, v) }
        func main() -> i32 {
            val pair = dup("text")
            0
        }
        "#,
    );
    assert!(bound_failure(&errors, "Copy"), "got {errors:?}");
}

/// A method call reads through a borrow, so in an instance where `T` is `&S` the body's
/// `v.clone()` would reach `S`'s own `clone`: a reference does not meet the bound.
#[test]
fn copy_bound_refuses_a_reference_argument() {
    let errors = errors_with_traits(
        r#"
        func dup<T: Copy>(v: T) -> (T, T) { (v, v) }
        func main() -> i32 {
            val n = 3
            val pair = dup(&n)
            0
        }
        "#,
    );
    assert!(bound_failure(&errors, "Copy"), "got {errors:?}");
}

#[test]
fn clone_bound_gives_the_body_clone_for_every_kind_of_implementor() {
    assert_clean(
        r#"
        func twice<T: Clone>(v: &T) -> T { v.clone() }
        @derive(Clone)
        struct Derived { x: i32 }
        struct Hand { x: i32 }
        impl Clone for Hand {
            func clone(&self) -> Self { Hand { x: self.x } }
        }
        func main() -> i32 {
            val n = 2
            val s = "s"
            val d = Derived { x: 1 }
            val h = Hand { x: 1 }
            val a = twice(&n)
            val b = twice(&s)
            val c = twice(&d)
            val e = twice(&h)
            a + c.x + e.x
        }
        "#,
    );
}

#[test]
fn copy_bound_reaches_clone() {
    assert_clean(
        r#"
        func again<T: Copy>(v: &T) -> T { v.clone() }
        func main() -> i32 {
            val n = 4
            again(&n)
        }
        "#,
    );
}

#[test]
fn clone_bound_refuses_a_struct_without_clone() {
    let errors = errors_with_traits(
        r#"
        func twice<T: Clone>(v: &T) -> T { v.clone() }
        struct Plain { x: i32 }
        func main() -> i32 {
            val p = Plain { x: 1 }
            val q = twice(&p)
            0
        }
        "#,
    );
    assert!(bound_failure(&errors, "Clone"), "got {errors:?}");
}

#[test]
fn partial_eq_bound_is_checked_at_the_call() {
    let errors = errors_with_traits(
        r#"
        func same<T: PartialEq>(a: T, b: T) -> bool { a == b }
        struct Plain { x: i32 }
        func main() -> i32 {
            val ok = same(1, 2)
            val bad = same(Plain { x: 1 }, Plain { x: 1 })
            0
        }
        "#,
    );
    let failures = errors
        .iter()
        .filter(|e| matches!(e, TypeError::TraitBoundNotSatisfied { .. }))
        .count();
    assert_eq!(
        failures, 1,
        "only the struct fails the bound, got {errors:?}"
    );
    assert!(bound_failure(&errors, "PartialEq"), "got {errors:?}");
}

#[test]
fn hand_written_copy_makes_its_struct_copy() {
    assert_clean(
        r#"
        struct P { x: i32 }
        impl Clone for P {
            func clone(&self) -> Self { P { x: self.x } }
        }
        impl Copy for P {}
        func main() -> i32 {
            val p = P { x: 1 }
            val a = p
            val b = p
            a.x + b.x
        }
        "#,
    );
}

#[test]
fn hand_written_copy_needs_copy_fields() {
    let errors = errors_with_traits(
        r#"
        struct Owns { s: string }
        impl Clone for Owns {
            func clone(&self) -> Self { Owns { s: "copy" } }
        }
        impl Copy for Owns {}
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::CopyDeriveNonCopyField { field_name, .. } if field_name == "s")
        ),
        "got {errors:?}"
    );
}

#[test]
fn hand_written_copy_needs_clone() {
    let errors = errors_with_traits(
        r#"
        struct P { x: i32 }
        impl Copy for P {}
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::MissingSupertraitImpl { trait_name, supertrait, .. }
                if trait_name == "Copy" && supertrait == "Clone"
        )),
        "got {errors:?}"
    );
}

#[test]
fn hand_written_copy_is_refused_on_an_enum() {
    let errors = errors_with_traits(
        r#"
        enum E { A, B }
        impl Copy for E {}
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidCopyImpl { type_name, .. } if type_name == "E")),
        "got {errors:?}"
    );
    assert!(
        !errors
            .iter()
            .any(|e| matches!(e, TypeError::MissingSupertraitImpl { .. })),
        "a refused impl needs no supertrait, got {errors:?}"
    );
}

#[test]
fn hand_written_copy_is_refused_beside_drop() {
    let errors = errors_with_traits(
        r#"
        struct P { x: i32 }
        impl Clone for P {
            func clone(&self) -> Self { P { x: self.x } }
        }
        impl Copy for P {}
        impl Drop for P {
            func drop(&mut self) {}
        }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DropTypeCannotBeCopy { .. })),
        "got {errors:?}"
    );
}

#[test]
fn deriving_and_implementing_the_same_trait_conflicts() {
    for (derive, trait_impl) in [
        (
            "Clone",
            "impl Clone for P { func clone(&self) -> Self { P { x: self.x } } }",
        ),
        (
            "Copy",
            "impl Clone for P { func clone(&self) -> Self { P { x: self.x } } }",
        ),
        (
            "Hashable",
            "impl Hashable for P { func hash(&self) -> u64 { 0 } }",
        ),
        (
            "Debug",
            "impl Debug for P { func fmt_debug(&self, out: &mut StringBuilder) {} }",
        ),
    ] {
        let errors = errors_with_traits(&format!(
            "@derive({derive})\nstruct P {{ x: i32 }}\n{trait_impl}\nfunc main() -> i32 {{ 0 }}"
        ));
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::DeriveConflictsWithImpl { .. })),
            "@derive({derive}) beside `{trait_impl}` must conflict, got {errors:?}"
        );
    }
}

#[test]
fn derived_hashable_refuses_a_float_field() {
    let errors = errors_with_traits(
        r#"
        @derive(Hashable)
        struct F { x: f32 }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DeriveFieldUnsupported { trait_name, field_name, .. }
                if trait_name == "Hashable" && field_name == "x"
        )),
        "got {errors:?}"
    );
}

/// A derive calls a field's own method, so a field whose trait is written by hand meets
/// every derived rule, the impls declared after the derives included.
#[test]
fn a_derive_accepts_a_field_implementing_the_trait_by_hand() {
    assert_clean(
        r#"
        @derive(Debug, Clone, PartialEq, Hashable)
        struct Wrap<T> { v: T }
        struct Inner { x: i32 }
        @derive(Debug, Clone, PartialEq, Hashable)
        struct Outer { inner: Inner, wrapped: Wrap<Inner>, n: u8 }
        impl Hashable for Inner {
            func hash(&self) -> u64 { self.x as u64 }
        }
        impl Debug for Inner {
            func fmt_debug(&self, out: &mut StringBuilder) {}
        }
        impl Clone for Inner {
            func clone(&self) -> Self { Inner { x: self.x } }
        }
        impl Copy for Inner {}
        impl PartialEq for Inner {
            func eq(&self, rhs: &Inner) -> bool { true }
        }
        func main() -> i32 { 0 }
        "#,
    );
}

/// `PartialEq<i32>` compares an `Inner` with an `i32`, not with an `Inner`, so the derived
/// `==` has no `eq` to call for the field.
#[test]
fn a_derived_partial_eq_refuses_a_field_compared_with_another_type() {
    let errors = errors_with_traits(
        r#"
        struct Inner { x: i32 }
        impl Clone for Inner {
            func clone(&self) -> Self { Inner { x: self.x } }
        }
        impl Copy for Inner {}
        impl PartialEq<i32> for Inner {
            func eq(&self, rhs: &i32) -> bool { true }
        }
        @derive(PartialEq)
        struct Outer { inner: Inner }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::DeriveFieldUnsupported { field_name, .. } if field_name == "inner")
        ),
        "got {errors:?}"
    );
}

#[test]
fn derived_hashable_accepts_a_derived_struct_field() {
    assert_clean(
        r#"
        @derive(Hashable)
        struct Inner { x: i32 }
        @derive(Hashable)
        struct Outer { inner: Inner, n: u8 }
        func main() -> i32 { 0 }
        "#,
    );
}

#[test]
fn derived_partial_eq_and_hashable_make_a_map_key() {
    assert_clean(
        r#"
        @derive(PartialEq, Hashable)
        struct K { a: i32, b: string }
        func main() -> i32 {
            mut m: HashMap<K, i32> = HashMap::new()
            m.insert(K { a: 1, b: "x" }, 2)
            0
        }
        "#,
    );
}

#[test]
fn hashable_impl_is_held_to_the_trait_signature() {
    let errors = errors_with_traits(
        r#"
        struct P { x: i32 }
        impl Hashable for P {
            func hash(&self) -> i32 { 1 }
        }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::TraitMethodSignatureMismatch { .. })),
        "got {errors:?}"
    );
}

#[test]
fn vec_of_a_copy_bounded_parameter_is_storable() {
    assert_clean(
        r#"
        func filled<T: Copy>(v: T) -> Vec<T> {
            mut out: Vec<T> = Vec::new()
            out.push(v)
            out
        }
        func main() -> i32 {
            val v = filled(3)
            v.len() as i32
        }
        "#,
    );
}

#[test]
fn vec_of_an_unbounded_parameter_is_refused() {
    let errors = errors_with_traits(
        r#"
        func filled<T>(v: T) -> Vec<T> {
            mut out: Vec<T> = Vec::new()
            out
        }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidCollectionElement { .. })),
        "got {errors:?}"
    );
}

#[test]
fn debug_bound_renders_in_a_debug_hole() {
    assert_clean(
        r#"
        func show<T: Debug>(v: &T) -> string { "{v:?}" }
        func main() -> i32 { 0 }
        "#,
    );
}

#[test]
fn unbounded_parameter_does_not_render() {
    let errors = errors_with_traits(
        r#"
        func show<T>(v: &T) -> string { "{v:?}" }
        func main() -> i32 { 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnformattableType { .. })),
        "got {errors:?}"
    );
}

#[test]
fn hand_written_debug_renders_under_debug_only() {
    let errors = errors_with_traits(
        r#"
        struct H { x: i32 }
        impl Debug for H {
            func fmt_debug(&self, out: &mut StringBuilder) { out.push_str("H") }
        }
        func main() -> i32 {
            val h = H { x: 1 }
            println("{h:?}")
            println("{h}")
            0
        }
        "#,
    );
    let refusals: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, TypeError::UnrenderableStruct { .. }))
        .collect();
    assert_eq!(
        refusals.len(),
        1,
        "only the hole without `:?` is refused, got {errors:?}"
    );
}

#[test]
fn supplied_methods_type_check_on_builtins_and_derives() {
    assert_clean(
        r#"
        @derive(Debug, Hashable)
        struct D { x: i32 }
        func main() -> i32 {
            val n = 3
            val copy = n.clone()
            val pair = (1, 2).clone()
            val h: u64 = "text".hash() + n.hash()
            val d = D { x: 1 }
            val dh: u64 = d.hash()
            mut out = StringBuilder::new()
            d.fmt_debug(&mut out)
            n.fmt_debug(&mut out)
            copy + pair.0
        }
        "#,
    );
}

#[test]
fn supplied_methods_are_refused_where_the_trait_is_not_implemented() {
    let errors = errors_with_traits(
        r#"
        struct Plain { x: i32 }
        func main() -> i32 {
            val p = Plain { x: 1 }
            val h = p.hash()
            val f = 1.5
            val g = f.hash()
            0
        }
        "#,
    );
    let missing = errors
        .iter()
        .filter(|e| matches!(e, TypeError::MethodNotFound { .. }))
        .count();
    assert_eq!(missing, 2, "got {errors:?}");
}
