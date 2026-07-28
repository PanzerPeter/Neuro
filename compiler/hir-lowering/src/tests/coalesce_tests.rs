//! The `??` desugar: the shape of the `match` it lowers to, and the nesting a chain
//! produces. No `HirExprKind` is added for `??`, so these tests assert on `Match`.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirBindingSource, HirExprKind, HirMatchTest, HirType};

/// The two fallible enums, declared exactly as the prelude declares them. The lowering
/// slice sees no prelude, so each program brings its own.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

#[test]
fn coalesce_lowers_to_a_two_arm_match() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func maybe() -> Option<i32> {{ Option::Some(1) }}
         func main() -> i32 {{
            val a = maybe() ?? 9
            a
         }}"
    ));
    let init = binding_init(function_body(&program, "main"), "a");

    assert_eq!(init.ty, HirType::I32, "`??` types to the unwrapped payload");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`??` should desugar to a match, got {:?}", init.kind);
    };
    assert_eq!(arms.len(), 2);

    // Arm 0 tests the success tag and binds payload slot 0, which it then returns.
    assert_eq!(arms[0].tests, vec![HirMatchTest::Tag { tag: 0 }]);
    assert_eq!(arms[0].bindings.len(), 1);
    assert_eq!(
        arms[0].bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    );
    assert_eq!(arms[0].bindings[0].ty, HirType::I32);
    let HirExprKind::Variable(read) = &arms[0].body.kind else {
        panic!("the success arm should read its binding");
    };
    assert_eq!(read, &arms[0].bindings[0].name);

    // Arm 1 is the unconditional fallback. Living in its own arm is what makes it lazy.
    assert_eq!(arms[1].tests, vec![HirMatchTest::Wildcard]);
    assert!(arms[1].bindings.is_empty());
    assert!(arms[1].guard.is_none());
}

#[test]
fn result_coalesce_tests_the_ok_tag() {
    // `Ok` is `Result`'s first variant, so the success test is tag 0 — and the `Err`
    // payload appears nowhere in the desugar, which is how `??` discards it.
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func fallible() -> Result<i32, char> {{ Result::Err('x') }}
         func main() -> i32 {{
            val a = fallible() ?? 9
            a
         }}"
    ));
    let init = binding_init(function_body(&program, "main"), "a");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`??` should desugar to a match");
    };
    assert_eq!(arms[0].tests, vec![HirMatchTest::Tag { tag: 0 }]);
    assert_eq!(arms[0].bindings[0].ty, HirType::I32);
}

#[test]
fn a_chain_nests_in_the_fallback_arm() {
    // Right-to-left associativity means the second `??` lands inside the first's
    // fallback arm — the reason each fallback is only reached after the one before it.
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func maybe(n: i32) -> Option<i32> {{ Option::Some(n) }}
         func main() -> i32 {{
            val a = maybe(1) ?? maybe(2) ?? 3
            a
         }}"
    ));
    let init = binding_init(function_body(&program, "main"), "a");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`??` should desugar to a match");
    };
    let HirExprKind::Match { arms: inner, .. } = &arms[1].body.kind else {
        panic!("the fallback of a chain should itself be a match");
    };
    assert_eq!(inner.len(), 2);
    // Distinct payload bindings, so the inner unwrap cannot shadow the outer one.
    assert_ne!(arms[0].bindings[0].name, inner[0].bindings[0].name);
}
