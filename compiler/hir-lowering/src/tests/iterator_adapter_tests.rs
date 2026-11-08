//! `.map` / `.enumerate` on an iterator value: each lowers to a literal of the prelude
//! adapter it names, instantiated at the receiver's element type.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirType};

/// The protocol and the adapters, declared as the prelude declares them. The lowering
/// slice sees no prelude, so each program brings its own.
const PROTOCOL: &str = r#"
enum Option<T> { Some(T), None }

trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

struct Count { at: i32 }

impl Iterator for Count {
    type Item = i32
    func next(&mut self) -> Option<i32> { Option::Some(self.at) }
}

struct Map<I, A, B> { inner: I, f: mut (A) -> B }

impl<I: Iterator<Item = A>, A, B> Iterator for Map<I, A, B> {
    type Item = B
    func next(&mut self) -> Option<B> {
        match self.inner.next() {
            Option::Some(value) => Option::Some((self.f)(value)),
            Option::None => Option::None
        }
    }
}

struct Enumerate<I, A> { inner: I, position: u64 }

impl<I: Iterator<Item = A>, A> Iterator for Enumerate<I, A> {
    type Item = (u64, A)
    func next(&mut self) -> Option<(u64, A)> {
        match self.inner.next() {
            Option::Some(value) => Option::Some((self.position, value)),
            Option::None => Option::None
        }
    }
}
"#;

/// The adapter literal `binding` is initialized with: its instance name and field names.
fn adapter_literal(src: &str, binding: &str) -> (String, Vec<String>) {
    let program = lower(&format!("{PROTOCOL}{src}"));
    let init = binding_init(function_body(&program, "main"), binding);
    let HirExprKind::StructLiteral { name, fields, .. } = &init.kind else {
        panic!(
            "{binding} should be an adapter literal, got {:?}",
            init.kind
        );
    };
    assert_eq!(init.ty, HirType::Struct(name.clone()));
    (
        name.clone(),
        fields.iter().map(|f| f.name.clone()).collect(),
    )
}

#[test]
fn map_lowers_to_a_map_literal_at_the_function_result() {
    let (name, fields) = adapter_literal(
        r#"
func main() -> i32 {
    val m = Count { at: 1 }.map(|v| v > 0)
    0
}
"#,
        "m",
    );
    assert_eq!(name, "Map$g_5Count_i32_bool");
    assert_eq!(fields, ["inner", "f"]);
}

#[test]
fn enumerate_lowers_to_an_enumerate_literal_starting_at_zero() {
    let (name, fields) = adapter_literal(
        r#"
func main() -> i32 {
    val e = Count { at: 1 }.enumerate()
    0
}
"#,
        "e",
    );
    assert_eq!(name, "Enumerate$g_5Count_i32");
    assert_eq!(fields, ["inner", "position"]);
}
