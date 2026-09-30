// Device operations: in a program that moves a tensor to a device, each float tensor
// operation a GPU can run is outlined into a function that runs where its operands live.
// This harness runs no prelude and no argument binding, so `Device` is declared here and
// the reduction axis is written positionally.

use neuro_hir::{HirExprKind, HirFunction, HirItem, HirProgram, HirStmt, HirTarget, HirType};

use super::{binding_init, function_body, lower};

const PROGRAM: &str = r#"
enum Device {
    CPU,
    GPU(i32)
}

func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = Tensor::ones()
    val b: Tensor<f32, [2, 3]> = Tensor::ones()
    val w: Tensor<f32, [3, 4]> = Tensor::ones()
    val g = a.clone().to(Device::GPU(0))
    val fused = (&g + &b) * 2.0f32
    val product = &a @ &w
    val rows = g.sum(1)
    val total = (&a + &b).max()
    val ints: Tensor<i32, [4]> = Tensor::ones()
    val kept = &ints + &ints
    return 0
}
"#;

fn outlined(program: &HirProgram) -> Vec<&HirFunction> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(f) if f.target == HirTarget::FollowsOperands => Some(f),
            _ => None,
        })
        .collect()
}

/// The outlined function a call in `main` names.
fn callee<'a>(program: &'a HirProgram, binding: &str) -> &'a HirFunction {
    let init = binding_init(function_body(program, "main"), binding);
    let call = match &init.kind {
        HirExprKind::TensorIndex { object, .. } => object,
        _ => init,
    };
    let HirExprKind::Call { callee, .. } = &call.kind else {
        panic!("`{binding}` is not a call: {:?}", init.kind);
    };
    let HirExprKind::Variable(name) = &callee.kind else {
        panic!("`{binding}` calls no named function");
    };
    outlined(program)
        .into_iter()
        .find(|f| &f.name == name)
        .expect("the callee is an outlined function")
}

#[test]
fn an_operator_tree_becomes_one_function_over_its_operands() {
    let program = lower(PROGRAM);
    let fused = callee(&program, "fused");
    let types: Vec<String> = fused.params.iter().map(|p| p.ty.to_string()).collect();
    assert_eq!(
        types,
        ["&Tensor<f32, [2, 3]>", "&Tensor<f32, [2, 3]>", "f32"],
        "each leaf is a parameter of its own type, borrows kept as borrows"
    );
    let [HirStmt::Expr(body)] = fused.body.as_slice() else {
        panic!("one tail expression");
    };
    let HirExprKind::Binary { left, .. } = &body.kind else {
        panic!("the tree is kept whole");
    };
    assert!(matches!(left.kind, HirExprKind::Binary { .. }));
    assert_eq!(callee(&program, "product").params.len(), 2);
}

#[test]
fn a_reduction_borrows_a_named_receiver() {
    let program = lower(PROGRAM);
    let rows = callee(&program, "rows");
    assert_eq!(rows.params[0].ty.to_string(), "&Tensor<f32, [2, 3]>");
    assert_eq!(rows.return_type.to_string(), "Tensor<f32, [2]>");
}

#[test]
fn a_whole_reduction_returns_one_element_the_call_site_reads() {
    let program = lower(PROGRAM);
    let init = binding_init(function_body(&program, "main"), "total");
    assert!(matches!(init.kind, HirExprKind::TensorIndex { .. }));
    assert_eq!(init.ty, HirType::F32);
    let total = callee(&program, "total");
    assert_eq!(total.return_type.to_string(), "Tensor<f32, [1]>");
    // The receiver is a temporary, itself an outlined sum, so it is moved in.
    assert_eq!(total.params[0].ty.to_string(), "Tensor<f32, [2, 3]>");
    assert_eq!(outlined(&program).len(), 5);
}

#[test]
fn integer_tensors_stay_inline() {
    let program = lower(PROGRAM);
    let kept = binding_init(function_body(&program, "main"), "kept");
    assert!(matches!(kept.kind, HirExprKind::Binary { .. }));
}

#[test]
fn a_program_that_moves_no_tensor_is_left_alone() {
    let program = lower(&PROGRAM.replace(".to(Device::GPU(0))", ""));
    assert!(outlined(&program).is_empty());
    let fused = binding_init(function_body(&program, "main"), "fused");
    assert!(matches!(fused.kind, HirExprKind::Binary { .. }));
}

#[test]
fn a_reduction_over_a_field_stays_inline() {
    // A backend borrows only a binding, and moving the field out would free it twice.
    let program = lower(
        r#"
enum Device {
    CPU,
    GPU(i32)
}

struct Layer {
    w: Tensor<f32, [2, 3]>
}

func main() -> i32 {
    val layer = Layer { w: Tensor::<f32, [2, 3]>::ones().to(Device::GPU(0)) }
    val total = layer.w.sum()
    return 0
}
"#,
    );
    let total = binding_init(function_body(&program, "main"), "total");
    assert!(matches!(total.kind, HirExprKind::TensorReduce { .. }));
    assert!(outlined(&program).is_empty());
}
