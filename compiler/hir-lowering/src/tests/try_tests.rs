//! The `?` desugar: the shape of the `match` it lowers to and the failure arm that
//! rebuilds the propagated value against the ENCLOSING function's return instance.
//! No `HirExprKind` is added for `?`, so these tests assert on `Match`.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirBindingSource, HirExprKind, HirMatchTest, HirStmt, HirType};

/// The two fallible enums, declared exactly as the prelude declares them. The lowering
/// slice sees no prelude, so each program brings its own.
const FALLIBLE_DECLS: &str = "enum Option<T> { Some(T), None }\n\
                              enum Result<T, E> { Ok(T), Err(E) }\n";

#[test]
fn try_lowers_to_a_two_arm_match() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func fallible() -> Result<i32, char> {{ Result::Ok(1) }}
         func caller() -> Result<i32, char> {{
            val a = fallible()?
            Result::Ok(a)
         }}
         func main() -> i32 {{ 0 }}"
    ));
    let init = binding_init(function_body(&program, "caller"), "a");

    assert_eq!(init.ty, HirType::I32, "`?` types to the unwrapped payload");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`?` should desugar to a match, got {:?}", init.kind);
    };
    assert_eq!(arms.len(), 2);

    // Arm 0 tests the `Ok` tag and binds payload slot 0, which it then yields.
    assert_eq!(arms[0].tests, vec![HirMatchTest::Tag { tag: 0 }]);
    assert_eq!(arms[0].bindings.len(), 1);
    assert_eq!(
        arms[0].bindings[0].source,
        HirBindingSource::EnumPayload { slot: 0 }
    );
    let HirExprKind::Variable(read) = &arms[0].body.kind else {
        panic!("the success arm should read its binding");
    };
    assert_eq!(read, &arms[0].bindings[0].name);

    // Arm 1 is unconditional and never falls through: it returns.
    assert_eq!(arms[1].tests, vec![HirMatchTest::Wildcard]);
    assert!(arms[1].guard.is_none());
    let HirExprKind::Block { stmts } = &arms[1].body.kind else {
        panic!(
            "the failure arm should be a block, got {:?}",
            arms[1].body.kind
        );
    };
    assert!(matches!(stmts.as_slice(), [HirStmt::Return { .. }]));
}

#[test]
fn result_failure_arm_forwards_the_err_payload() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func fallible() -> Result<i32, char> {{ Result::Err('x') }}
         func caller() -> Result<i32, char> {{
            val a = fallible()?
            Result::Ok(a)
         }}
         func main() -> i32 {{ 0 }}"
    ));
    let init = binding_init(function_body(&program, "caller"), "a");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`?` should desugar to a match");
    };

    // The error is bound out of slot 0 at its own type and handed straight back —
    // `?` forwards it as-is, with no conversion.
    assert_eq!(arms[1].bindings.len(), 1);
    assert_eq!(arms[1].bindings[0].ty, HirType::Char);
    let HirExprKind::Block { stmts } = &arms[1].body.kind else {
        panic!("the failure arm should be a block");
    };
    let [HirStmt::Return {
        value: Some(value), ..
    }] = stmts.as_slice()
    else {
        panic!("the failure arm should return a value");
    };
    let HirExprKind::EnumConstruct {
        variant, payload, ..
    } = &value.kind
    else {
        panic!("the propagated value should be an enum construction");
    };
    assert_eq!(variant, "Err");
    assert_eq!(payload.len(), 1);
    let HirExprKind::Variable(name) = &payload[0].kind else {
        panic!("the Err payload should be the bound error");
    };
    assert_eq!(name, &arms[1].bindings[0].name);
}

#[test]
fn option_failure_arm_returns_none_without_binding() {
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func maybe() -> Option<i32> {{ Option::Some(1) }}
         func caller() -> Option<i32> {{
            val a = maybe()?
            Option::Some(a)
         }}
         func main() -> i32 {{ 0 }}"
    ));
    let init = binding_init(function_body(&program, "caller"), "a");
    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`?` should desugar to a match");
    };

    // `None` carries nothing, so the failure arm binds nothing either.
    assert!(arms[1].bindings.is_empty());
    let HirExprKind::Block { stmts } = &arms[1].body.kind else {
        panic!("the failure arm should be a block");
    };
    let [HirStmt::Return {
        value: Some(value), ..
    }] = stmts.as_slice()
    else {
        panic!("the failure arm should return a value");
    };
    let HirExprKind::EnumConstruct {
        variant, payload, ..
    } = &value.kind
    else {
        panic!("the propagated value should be an enum construction");
    };
    assert_eq!(variant, "None");
    assert!(payload.is_empty());
}

#[test]
fn propagated_value_targets_the_callers_return_instance() {
    // The operand is a `Result<u8, char>` but the function returns `Result<i32, char>`:
    // the forwarded `Err` must be built as the caller's instance, not the operand's.
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func narrow() -> Result<u8, char> {{ Result::Ok(1u8) }}
         func caller() -> Result<i32, char> {{
            val a = narrow()?
            Result::Ok(1)
         }}
         func main() -> i32 {{ 0 }}"
    ));
    let init = binding_init(function_body(&program, "caller"), "a");
    assert_eq!(init.ty, HirType::U8, "the payload keeps the operand's type");

    let HirExprKind::Match { arms, .. } = &init.kind else {
        panic!("`?` should desugar to a match");
    };
    let HirExprKind::Block { stmts } = &arms[1].body.kind else {
        panic!("the failure arm should be a block");
    };
    let [HirStmt::Return {
        value: Some(value), ..
    }] = stmts.as_slice()
    else {
        panic!("the failure arm should return a value");
    };
    let HirExprKind::EnumConstruct { enum_name, .. } = &value.kind else {
        panic!("the propagated value should be an enum construction");
    };
    let HirType::Enum(expected) = &value.ty else {
        panic!("the propagated value should be an enum");
    };
    assert_eq!(enum_name, expected);
    assert_ne!(
        enum_name, "Result",
        "a generic instance is mangled, never the bare base name"
    );
}

#[test]
fn chained_try_binds_distinct_names() {
    // Two propagations in one body must not shadow each other's payload binding.
    let program = lower(&format!(
        "{FALLIBLE_DECLS}
         func fallible(n: i32) -> Result<i32, char> {{ Result::Ok(n) }}
         func caller() -> Result<i32, char> {{
            val a = fallible(1)?
            val b = fallible(2)?
            Result::Ok(a + b)
         }}
         func main() -> i32 {{ 0 }}"
    ));
    let body = function_body(&program, "caller");
    let first = binding_init(body, "a");
    let second = binding_init(body, "b");

    let (HirExprKind::Match { arms: a, .. }, HirExprKind::Match { arms: b, .. }) =
        (&first.kind, &second.kind)
    else {
        panic!("both `?` uses should desugar to matches");
    };
    assert_ne!(a[0].bindings[0].name, b[0].bindings[0].name);
    assert_ne!(a[1].bindings[0].name, b[1].bindings[0].name);
}
