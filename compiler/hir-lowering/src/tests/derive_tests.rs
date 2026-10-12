// Lowering for `@derive(PartialEq)` equality, the written name a monomorphized
// struct instance keeps for rendering, and the derivable traits' supplied methods.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirItem, HirType};

/// A derived comparison has no `eq` to dispatch to, so it stays a binary node the
/// backend expands over the fields, unlike a hand-written `impl PartialEq`, which
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
    assert!(instance.name.starts_with("W$g_"), "{}", instance.name);
}

/// The derivable traits as the prelude states them; lowering runs without the prelude.
const TRAITS: &str = r#"
trait Clone { func clone(&self) -> Self }
trait Debug { func fmt_debug(&self, out: &mut StringBuilder) }
trait Hashable { func hash(&self) -> u64 }
"#;

/// `.clone()` on a `Copy` value is the value itself, read through a borrowed receiver.
#[test]
fn clone_of_a_copy_value_is_the_value() {
    let program = lower(&format!(
        "{TRAITS}
func main() -> i32 {{
    val n = 4
    val r = &n
    val copy = r.clone()
    return copy
}}
"
    ));
    let init = binding_init(function_body(&program, "main"), "copy");
    assert_eq!(init.ty, HirType::I32);
    assert!(
        matches!(init.kind, HirExprKind::Deref { .. }),
        "a borrowed receiver is read through, got {:?}",
        init.kind
    );
}

/// A derived `Hashable` has no `hash` method: the call stays a `hash` call on the struct
/// for the backend to answer with the map's own key hash.
#[test]
fn hash_on_a_derived_struct_stays_a_builtin_call() {
    let program = lower(&format!(
        "{TRAITS}
@derive(Hashable)
struct K {{ x: i32 }}

func main() -> i32 {{
    val k = K {{ x: 1 }}
    val h = k.hash()
    return 0
}}
"
    ));
    let init = binding_init(function_body(&program, "main"), "h");
    assert_eq!(init.ty, HirType::U64);
    let HirExprKind::Call { callee, .. } = &init.kind else {
        panic!("expected a call, got {:?}", init.kind);
    };
    assert!(
        matches!(&callee.kind, HirExprKind::FieldAccess { field, .. } if field == "hash"),
        "got {:?}",
        callee.kind
    );
}

/// `{x:?}` on a hand-written `Debug` calls it through one helper per type, which renders
/// to a `string` the hole prints plainly.
#[test]
fn a_debug_hole_on_a_hand_written_impl_calls_one_helper() {
    let program = lower(&format!(
        "{TRAITS}
struct H {{ x: i32 }}

impl Debug for H {{
    func fmt_debug(&self, out: &mut StringBuilder) {{
        out.push_str(\"H\")
    }}
}}

func main() -> i32 {{
    val h = H {{ x: 1 }}
    println(\"{{h:?}} {{h:?}}\")
    return 0
}}
"
    ));
    let helpers: Vec<&neuro_hir::HirFunction> = program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(f) if f.name == "H__fmt_debug$string" => Some(f),
            _ => None,
        })
        .collect();
    assert_eq!(helpers.len(), 1, "one helper per type");
    assert_eq!(helpers[0].return_type, HirType::String);
}
