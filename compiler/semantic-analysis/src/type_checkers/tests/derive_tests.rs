// `@derive(...)` argument validation and the `Debug` / `PartialEq` derives.

use super::semantic_errors;
use crate::errors::TypeError;

/// A name outside the derivable set is a diagnostic, not the silent no-op it used to be:
/// a program that writes it would otherwise compile against behavior it does not have.
#[test]
fn unknown_derive_argument_is_rejected() {
    let errors = semantic_errors(
        r#"
        @derive(Bogus)
        struct P { x: i32 }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownDerive { name, .. } if name == "Bogus")),
        "an unknown derive must be rejected, got {errors:?}"
    );
}

/// `Hashable` is in the spec's derivable set but nothing generates it yet, so it gets
/// its own diagnostic rather than being lumped in with a misspelling.
#[test]
fn pending_derive_argument_reports_as_unimplemented() {
    let errors = semantic_errors(
        r#"
        @derive(Hashable)
        struct P { x: i32 }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::UnimplementedDerive { name, .. } if name == "Hashable")
        ),
        "a pending derive must report as unimplemented, got {errors:?}"
    );
}

#[test]
fn repeated_derive_argument_is_rejected() {
    let errors = semantic_errors(
        r#"
        @derive(Copy, Copy)
        struct P { x: i32 }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DuplicateDerive { name, .. } if name == "Copy")),
        "a repeated derive must be rejected, got {errors:?}"
    );
}

#[test]
fn copy_and_clone_derives_still_pass_validation() {
    let errors = semantic_errors(
        r#"
        @derive(Copy, Clone)
        struct P { x: i32 }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn derived_partial_eq_accepts_equality() {
    let errors = semantic_errors(
        r#"
        @derive(PartialEq)
        struct P { x: i32, tag: string }
        func main() -> i32 {
            val a = P { x: 1, tag: "a" }
            val b = P { x: 1, tag: "a" }
            if a == b { return 1 }
            return 0
        }
        "#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// The derive is the only thing that changes: without it the BUG-015 diagnostic stands.
#[test]
fn equality_without_the_derive_is_still_rejected() {
    let errors = semantic_errors(
        r#"
        struct P { x: i32 }
        func main() -> i32 {
            val a = P { x: 1 }
            val b = P { x: 1 }
            if a == b { return 1 }
            return 0
        }
        "#,
    );
    assert!(
        errors.iter().any(
            |e| matches!(e, TypeError::MissingPartialEqImpl { type_name, .. } if type_name == "P")
        ),
        "got {errors:?}"
    );
}

/// The derived comparison is emitted inline over the fields and never calls a method, so
/// a nested struct has to carry the derive too.
#[test]
fn derived_partial_eq_requires_comparable_fields() {
    let errors = semantic_errors(
        r#"
        struct Q { x: i32 }
        @derive(PartialEq)
        struct P { q: Q }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DeriveFieldUnsupported { trait_name, field_name, .. }
                if trait_name == "PartialEq" && field_name == "q"
        )),
        "got {errors:?}"
    );
}

#[test]
fn derived_debug_renders_under_the_debug_specifier() {
    let errors = semantic_errors(
        r#"
        @derive(Debug)
        struct P { x: i32 }
        func main() -> i32 {
            val p = P { x: 1 }
            val s = "{p:?}"
            return s.len() as i32
        }
        "#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

/// A struct has no `Display` form, so the bare hole is an error even with the derive.
#[test]
fn derived_debug_does_not_render_without_the_specifier() {
    let errors = semantic_errors(
        r#"
        @derive(Debug)
        struct P { x: i32 }
        func main() -> i32 {
            val p = P { x: 1 }
            val s = "{p}"
            return s.len() as i32
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnrenderableStruct { name, .. } if name == "P")),
        "got {errors:?}"
    );
}

#[test]
fn debug_hole_without_the_derive_is_rejected() {
    let errors = semantic_errors(
        r#"
        struct P { x: i32 }
        func main() -> i32 {
            val p = P { x: 1 }
            val s = "{p:?}"
            return s.len() as i32
        }
        "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UnrenderableStruct { name, .. } if name == "P")),
        "got {errors:?}"
    );
}

#[test]
fn derived_debug_requires_renderable_fields() {
    let errors = semantic_errors(
        r#"
        struct Q { x: i32 }
        @derive(Debug)
        struct P { q: Q }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DeriveFieldUnsupported { trait_name, field_name, .. }
                if trait_name == "Debug" && field_name == "q"
        )),
        "got {errors:?}"
    );
}

/// The derive compares fields inline and the impl routes through a method; the operator
/// dispatch consults the impl first, so keeping both would silently outrank the derive.
#[test]
fn derive_and_hand_written_impl_conflict() {
    let errors = semantic_errors(
        r#"
        @derive(Copy, Clone, PartialEq)
        struct P { x: i32 }
        impl PartialEq for P {
            func eq(&self, rhs: &P) -> bool { self.x == rhs.x }
            func ne(&self, rhs: &P) -> bool { self.x != rhs.x }
        }
        func main() -> i32 { return 0 }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DeriveConflictsWithImpl { trait_name, struct_name, .. }
                if trait_name == "PartialEq" && struct_name == "P"
        )),
        "got {errors:?}"
    );
}

/// A generic template's fields are type parameters, which no derive rule can judge —
/// the concrete substitution is the first point at which it can, so the template passes
/// and the instantiation is what reports.
#[test]
fn generic_instance_validates_its_concrete_fields() {
    let errors = semantic_errors(
        r#"
        struct Q { x: i32 }
        @derive(PartialEq)
        struct W<T> { value: T }
        func main() -> i32 {
            val w = W { value: Q { x: 1 } }
            return 0
        }
        "#,
    );
    assert!(
        errors.iter().any(|e| matches!(
            e,
            TypeError::DeriveFieldUnsupported { trait_name, struct_name, .. }
                if trait_name == "PartialEq" && struct_name == "W"
        )),
        "got {errors:?}"
    );
}
