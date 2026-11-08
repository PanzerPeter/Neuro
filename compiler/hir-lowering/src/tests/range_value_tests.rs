//! A range written as a value lowers to a literal of the prelude's `Range`, instantiated
//! at its bounds' type, and the walk's intrinsics lower to casts and a wrapping add.

use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirType};

/// The walk as the prelude writes it, cut down to what lowering reads. The lowering slice
/// sees no prelude, so each program brings its own.
const RANGE: &str = r#"
enum Option<T> { Some(T), None }

trait Iterator {
    type Item
    func next(&mut self) -> Option<Self::Item>
}

struct Range<T> { start: T, end: T, inclusive: bool, stride: u64, reversed: bool, done: bool }

impl<T: Comparable + Copy> Iterator for Range<T> {
    type Item = T
    func next(&mut self) -> Option<T> {
        if self.done {
            return Option::None
        }
        val value = self.start
        val left = self.end.__to_u64().wrapping_sub(self.start.__to_u64())
        self.start = self.start.__offset(self.stride)
        self.done = left < self.stride
        Option::Some(value)
    }
}
"#;

#[test]
fn a_range_value_lowers_to_a_range_literal_at_its_bound_type() {
    let program = lower(&format!(
        "{RANGE}{}",
        r#"
func main() -> i32 {
    val n: u64 = 3
    val r = 0..=n
    0
}
"#
    ));
    let init = binding_init(function_body(&program, "main"), "r");
    let HirExprKind::StructLiteral { name, fields, .. } = &init.kind else {
        panic!("r should be a Range literal, got {:?}", init.kind);
    };
    assert_eq!(init.ty, HirType::Struct(name.clone()));
    assert!(name.starts_with("Range$"), "{name}");
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        ["start", "end", "inclusive", "stride", "reversed", "done"]
    );
    assert_eq!(
        fields[0].value.ty,
        HirType::U64,
        "the literal start takes the end's type"
    );
    assert!(matches!(
        fields[2].value.kind,
        HirExprKind::Literal(shared_types::Literal::Boolean(true))
    ));
}

#[test]
fn the_walk_intrinsics_lower_to_casts_around_a_wrapping_add() {
    let program = lower(&format!(
        "{RANGE}{}",
        r#"
func main() -> i32 {
    mut r = (0 as i8)..5
    val first = r.next()
    0
}
"#
    ));
    let next = program
        .items
        .iter()
        .find_map(|item| match item {
            neuro_hir::HirItem::Impl(def) if def.type_name.starts_with("Range$") => {
                def.methods.iter().find(|m| m.name == "next")
            }
            _ => None,
        })
        .expect("the instance's next is lowered");
    let rendered = format!("{:?}", next.body);
    assert!(rendered.contains("wrapping_add"), "{rendered}");
    assert!(rendered.contains("Cast"), "{rendered}");
    assert!(!rendered.contains("__offset"), "{rendered}");
    assert!(!rendered.contains("__to_u64"), "{rendered}");
}
