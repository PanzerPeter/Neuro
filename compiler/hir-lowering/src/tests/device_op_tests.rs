// Device operations: in a program that moves a tensor to a device, each tensor operation a
// GPU can run is outlined into a function that runs where its operands live.
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
    assert_eq!(outlined(&program).len(), 6);
}

#[test]
fn integer_tensors_are_outlined_like_float_ones() {
    let program = lower(PROGRAM);
    let kept = callee(&program, "kept");
    assert_eq!(kept.params[0].ty.to_string(), "&Tensor<i32, [4]>");
    assert_eq!(kept.return_type.to_string(), "Tensor<i32, [4]>");
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

#[test]
fn a_sort_is_outlined_over_its_receiver() {
    // Arguments are positional here: `k` first, then the axis.
    let program = lower(
        r#"
enum Device {
    CPU,
    GPU(i32)
}

func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = Tensor::ones()
    val g = a.clone().to(Device::GPU(0))
    val sorted = g.sort()
    val order = (&g + &a).argsort(0)
    val (top, at) = g.topk(2)
    val ints: Tensor<i32, [4]> = Tensor::ones()
    val kept = ints.sort()
    return 0
}
"#,
    );
    let sorted = callee(&program, "sorted");
    assert_eq!(sorted.params[0].ty.to_string(), "&Tensor<f32, [2, 3]>");
    assert_eq!(sorted.return_type.to_string(), "Tensor<f32, [2, 3]>");
    // The receiver is a temporary, itself an outlined sum, so it is moved in.
    let order = callee(&program, "order");
    assert_eq!(order.params[0].ty.to_string(), "Tensor<f32, [2, 3]>");
    assert_eq!(order.return_type.to_string(), "Tensor<i32, [2, 3]>");
    let topk = outlined(&program)
        .into_iter()
        .find(|f| matches!(f.return_type, HirType::Tuple(_)))
        .expect("`.topk` is outlined with its pair");
    assert_eq!(
        topk.return_type.to_string(),
        "(Tensor<f32, [2, 2]>, Tensor<i32, [2, 2]>)"
    );
    let kept = callee(&program, "kept");
    assert_eq!(kept.return_type.to_string(), "Tensor<i32, [4]>");
    assert_eq!(outlined(&program).len(), 5);
}

const MORE: &str = r#"
enum Device {
    CPU,
    GPU(i32)
}

struct Layer {
    w: Tensor<f32, [2, 3]>
}

func twice(x: f32) -> f32 { x * 2.0f32 }

func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = Tensor::ones()
    val g = a.clone().to(Device::GPU(0))
    val k = 1u64
    val smooth = (&g + &a).exp() * 2.0f32
    val row = g[k, 0..2]
    val flipped = g.clone().t()
    val gram = einsum("ij,kj->ik", &g, &g)
    val trace = einsum("ij,ij->", &g, &g)
    val scale = 3.0f32
    val mapped = g.map(|x: f32| -> f32 { x * scale })
    val named = g.map(twice)
    val folded = g.reduce(0.0f32, |acc: f32, x: f32| -> f32 { acc + x })
    mut w = a.clone()
    w -= &g
    mut layer = Layer { w: a.clone() }
    layer.w -= &g
    return 0
}
"#;

#[test]
fn elementwise_math_joins_the_operator_tree_and_borrows_its_leaf() {
    let program = lower(MORE);
    let smooth = callee(&program, "smooth");
    let types: Vec<String> = smooth.params.iter().map(|p| p.ty.to_string()).collect();
    assert_eq!(
        types,
        ["&Tensor<f32, [2, 3]>", "&Tensor<f32, [2, 3]>", "f32"],
        "one function for `+`, `.exp()` and `*`"
    );
}

#[test]
fn a_slice_position_is_checked_where_the_call_evaluates_it() {
    let program = lower(MORE);
    let row = callee(&program, "row");
    let types: Vec<String> = row.params.iter().map(|p| p.ty.to_string()).collect();
    assert_eq!(types, ["&Tensor<f32, [2, 3]>", "u64"]);
    let init = binding_init(function_body(&program, "main"), "row");
    let HirExprKind::Call { args, .. } = &init.kind else {
        panic!("the slice is a call: {:?}", init.kind);
    };
    let HirExprKind::Block { stmts } = &args[1].kind else {
        panic!("the position is guarded: {:?}", args[1].kind);
    };
    assert!(matches!(
        stmts.as_slice(),
        [
            HirStmt::VarDecl { .. },
            HirStmt::If { .. },
            HirStmt::Expr(_)
        ]
    ));
}

#[test]
fn a_permute_takes_its_receiver_and_einsum_lends_its_operands() {
    let program = lower(MORE);
    let flipped = callee(&program, "flipped");
    assert_eq!(flipped.params[0].ty.to_string(), "Tensor<f32, [2, 3]>");
    let gram = callee(&program, "gram");
    assert_eq!(gram.params[0].ty.to_string(), "&Tensor<f32, [2, 3]>");
    assert_eq!(gram.return_type.to_string(), "Tensor<f32, [2, 2]>");
    let trace = callee(&program, "trace");
    assert_eq!(
        trace.return_type.to_string(),
        "Tensor<f32, [1]>",
        "a full contraction is boxed like a whole reduction"
    );
}

#[test]
fn a_traversal_keeps_its_closure_and_takes_its_captures() {
    let program = lower(MORE);
    let mapped = callee(&program, "mapped");
    let names: Vec<&str> = mapped.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["__operand0", "scale"]);
    let [HirStmt::Expr(body)] = mapped.body.as_slice() else {
        panic!("one tail expression");
    };
    let HirExprKind::TensorApply { callee, .. } = &body.kind else {
        panic!("the traversal itself: {:?}", body.kind);
    };
    assert!(matches!(callee.kind, HirExprKind::Closure { .. }));
    assert_eq!(callee_name(&program, "folded"), "outlined");
    // A function passed by name is a closure forwarding to it, so it is outlined too.
    assert_eq!(callee_name(&program, "named"), "outlined");
}

#[test]
fn a_traversal_over_a_function_local_stays_inline() {
    // Through a local the closure is a value only the run time knows.
    let program = lower(&MORE.replace(
        "val named = g.map(twice)",
        "val f = |x: f32| -> f32 { x }\n    val named = g.map(f)",
    ));
    assert_eq!(callee_name(&program, "named"), "inline");
}

#[test]
fn a_compound_assignment_writes_through_a_mutable_borrow() {
    let program = lower(MORE);
    let main = function_body(&program, "main");
    let calls: Vec<&HirFunction> = main
        .iter()
        .filter_map(|stmt| match stmt {
            HirStmt::Expr(expr) => match &expr.kind {
                HirExprKind::Call { callee, .. } => match &callee.kind {
                    HirExprKind::Variable(name) => {
                        outlined(&program).into_iter().find(|f| &f.name == name)
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    let [update] = calls.as_slice() else {
        panic!("only `w -= &g` is outlined, not the field: {calls:?}");
    };
    let types: Vec<String> = update.params.iter().map(|p| p.ty.to_string()).collect();
    assert_eq!(types, ["&Tensor<f32, [2, 3]>", "&mut Tensor<f32, [2, 3]>"]);
    assert_eq!(update.return_type, HirType::Void);
    assert!(main.iter().any(|stmt| matches!(
        stmt,
        HirStmt::TensorCompoundAssign {
            place: neuro_hir::HirPlace::Field { .. },
            ..
        }
    )));
}

/// "outlined" when `binding` is initialized by a call to an outlined function, whether its
/// value is read back from a boxed result or not.
fn callee_name(program: &HirProgram, binding: &str) -> &'static str {
    let init = binding_init(function_body(program, "main"), binding);
    let call = match &init.kind {
        HirExprKind::TensorIndex { object, .. } => object,
        _ => init,
    };
    match &call.kind {
        HirExprKind::Call { .. } => "outlined",
        _ => "inline",
    }
}
