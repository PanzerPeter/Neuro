// Lowering for `@derive(PartialEq)` equality and the written name a monomorphized
// struct instance keeps for rendering.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirItem, HirType};

/// A derived comparison has no `eq` to dispatch to, so it stays a binary node the
/// backend expands over the fields — unlike a hand-written `impl PartialEq`, which
/// lowers to an ordinary method call.
#[test]
fn derived_equality_stays_a_binary_node_typed_bool() {
    let program = lower(
        r#"
@derive(PartialEq)
struct P { x: i32, tag: string }

func main() -> i32 {
    val a = P { x: 1, tag: "a" }
    val b = P { x: 1, tag: "a" }
    val same = a == b
    return 0
}
"#,
    );
    let body = function_body(&program, "main");
    let init = binding_init(body, "same");
    assert_eq!(init.ty, HirType::Bool, "derived `==` yields bool");
    assert!(
        matches!(init.kind, HirExprKind::Binary { .. }),
        "derived `==` must not lower to a method call, got {:?}",
        init.kind
    );
}

/// A monomorphized instance carries the template's name so the derived debug rendering
/// can print what the programmer wrote rather than the mangled instance key.
#[test]
fn generic_instance_keeps_its_written_name() {
    let program = lower(
        r#"
struct W<T> { value: T }

func main() -> i32 {
    val w = W { value: 4 }
    return w.value
}
"#,
    );
    let instance = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Struct(s) if s.name != s.written_name => Some(s),
            _ => None,
        })
        .expect("the instantiation should emit a mangled struct");
    assert_eq!(instance.written_name, "W");
    assert!(instance.name.starts_with("W_g_"), "{}", instance.name);
}
