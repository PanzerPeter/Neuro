#[allow(unused_imports)]
use super::{
    binding_init, enum_names, function_body, function_names, impl_method_names, lower, struct_names,
};
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
