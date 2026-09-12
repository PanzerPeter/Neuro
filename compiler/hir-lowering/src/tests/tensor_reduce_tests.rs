// An `axis:` label is resolved by the `argument-binding` pass, which runs before lowering
// in the pipeline but not in this slice's test harness, so the axis is written
// positionally here. The labelled spelling is covered end to end by `neurc`'s tests.

use super::{binding_init, function_body, lower};

use neuro_hir::{AxisNames, HirExprKind, HirReduceOp, HirType};

/// The node one reduction lowers to: which operation, which axis, and the type it hands
/// back.
fn reduce_of(src: &str, binding: &str) -> (HirReduceOp, Option<usize>, HirType) {
    let program = lower(src);
    let body = function_body(&program, "main");
    let init = binding_init(body, binding);
    let HirExprKind::TensorReduce { op, axis, .. } = &init.kind else {
        panic!(
            "'{binding}' should lower to a reduction, got {:?}",
            init.kind
        );
    };
    (*op, *axis, init.ty.clone())
}

#[test]
fn a_whole_tensor_reduction_lowers_to_the_element_type() {
    let (op, axis, ty) = reduce_of(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val total = m.sum()
    return 0
}
"#,
        "total",
    );
    assert_eq!(op, HirReduceOp::Sum);
    assert_eq!(axis, None);
    assert_eq!(ty, HirType::I32);
}

#[test]
fn an_axis_reduction_drops_that_axis_and_keeps_the_other_names() {
    let (op, axis, ty) = reduce_of(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [height: 2, width: 3]> = Tensor::<i32, [2, 3]>::ones()
    val rows = m.max(height)
    return 0
}
"#,
        "rows",
    );
    assert_eq!(op, HirReduceOp::Max);
    assert_eq!(axis, Some(0));
    assert_eq!(
        ty,
        HirType::Tensor {
            element: Box::new(HirType::I32),
            shape: vec![Some(3)],
            names: AxisNames(vec![Some("width".to_string())]),
        }
    );
}

#[test]
fn a_negative_axis_counts_from_the_end() {
    let (op, axis, _) = reduce_of(
        r#"
func main() -> i32 {
    val m: Tensor<f64, [2, 3, 4]> = Tensor::<f64, [2, 3, 4]>::ones()
    val last = m.mean(-1)
    return 0
}
"#,
        "last",
    );
    assert_eq!(op, HirReduceOp::Mean);
    assert_eq!(axis, Some(2));
}

/// A borrowed receiver reaches the same node: a reduction reads the buffer, so it does
/// not need to own it.
#[test]
fn a_borrowed_receiver_lowers_to_a_reduction() {
    let program = lower(
        r#"
func total(t: &Tensor<i32, [3]>) -> i32 {
    t.min()
}

func main() -> i32 {
    return 0
}
"#,
    );
    let body = function_body(&program, "total");
    let tail = body.last().expect("a tail statement");
    let neuro_hir::HirStmt::Expr(expr) = tail else {
        panic!("the body should end in an expression, got {tail:?}");
    };
    assert!(
        matches!(
            expr.kind,
            HirExprKind::TensorReduce {
                op: HirReduceOp::Min,
                axis: None,
                ..
            }
        ),
        "a borrowed receiver should reduce, got {:?}",
        expr.kind
    );
}
