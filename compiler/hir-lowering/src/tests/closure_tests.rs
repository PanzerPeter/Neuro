use super::{binding_init, function_body, lower};
use neuro_hir::{HirExprKind, HirItem, HirType};

#[test]
fn closure_lowers_to_value_and_lifted_item() {
    let program = lower("func main() -> i32 { val base = 10\n val f = |x: i32| x + base\n f(5) }");
    let body = function_body(&program, "main");

    // The binding's initializer is a closure value referencing its lifted item and
    // capturing `base` (a Copy local) by value.
    let init = binding_init(body, "f");
    let (name, captures) = match &init.kind {
        HirExprKind::Closure { name, captures } => (name, captures),
        other => panic!("expected a closure value, got {:?}", other),
    };
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].name, "base");
    assert_eq!(captures[0].ty, HirType::I32);
    assert_eq!(
        init.ty,
        HirType::Function {
            params: vec![HirType::I32],
            ret: Box::new(HirType::I32),
        }
    );

    // A matching top-level closure item is emitted with the parameter and capture.
    let closure = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Closure(c) if &c.name == name => Some(c),
            _ => None,
        })
        .expect("a lifted closure item should be emitted");
    assert_eq!(closure.params.len(), 1);
    assert_eq!(closure.params[0].name, "x");
    assert_eq!(closure.return_type, HirType::I32);
    assert_eq!(closure.captures.len(), 1);
    assert_eq!(closure.captures[0].name, "base");
}

/// The capture names of the closure `binding` is initialized with in `main`.
fn capture_names(program: &neuro_hir::HirProgram, binding: &str) -> Vec<String> {
    let init = binding_init(function_body(program, "main"), binding);
    match &init.kind {
        HirExprKind::Closure { captures, .. } => captures.iter().map(|c| c.name.clone()).collect(),
        other => panic!("expected a closure value, got {:?}", other),
    }
}

#[test]
fn a_name_bound_inside_a_closure_hides_only_the_reads_in_its_scope() {
    // Each closure reads an enclosing local the body ALSO binds, somewhere the read
    // cannot see: after the read, in another match arm, or as an inner closure's
    // parameter. The read is still a capture.
    let program = lower(
        "func main() -> i32 {
            val x = 7
            val n = 4
            val v = 3
            val k: u64 = 1
            val later = |a: i32| -> i32 {
                val y = x + a
                val x = 100
                y + x
            }
            val arms = |a: i32| -> i32 {
                match a {
                    0 => n,
                    n => n * 2,
                }
            }
            val inner = |a: i32| -> i32 {
                val h = |v: i32| -> i32 { v + 1 }
                h(a) + v
            }
            val place = |a: i32| -> i32 {
                mut buf = [0, 0, 0]
                buf[k] = a
                buf[1]
            }
            later(1) + arms(0) + inner(1) + place(2)
        }",
    );
    assert_eq!(capture_names(&program, "later"), ["x"]);
    assert_eq!(capture_names(&program, "arms"), ["n"]);
    assert_eq!(capture_names(&program, "inner"), ["v"]);
    assert_eq!(capture_names(&program, "place"), ["k"]);
}
