use super::{binding_init, function_body, lower};
use crate::{lower_program, LoweringError};

use neuro_hir::{AxisNames, HirExpr, HirExprKind, HirItem, HirMathOp, HirStmt, HirType};

/// The node one math call lowers to: its function, its exponent's type, and its own type.
fn math_of(src: &str, binding: &str) -> (HirMathOp, Option<HirType>, HirType) {
    let program = lower(src);
    let init = binding_init(function_body(&program, "main"), binding);
    let HirExprKind::Math { op, exponent, .. } = &init.kind else {
        panic!("'{binding}' should lower to math, got {:?}", init.kind);
    };
    (
        *op,
        exponent.as_ref().map(|e| e.ty.clone()),
        init.ty.clone(),
    )
}

#[test]
fn a_scalar_receiver_lowers_to_math_of_its_own_type() {
    let src = r#"
func main() -> i32 {
    val x: f64 = 2.0
    val y = x.exp()
    return 0
}
"#;
    assert_eq!(math_of(src, "y"), (HirMathOp::Exp, None, HirType::F64));
}

#[test]
fn a_tensor_pow_keeps_the_shape_and_types_its_exponent_as_the_element() {
    let src = r#"
func main() -> i32 {
    val t: Tensor<f32, [rows: 2]> = Tensor::<f32, [2]>::ones()
    val p = t.pow(2.0)
    return 0
}
"#;
    let tensor = HirType::Tensor {
        element: Box::new(HirType::F32),
        shape: vec![Some(2)],
        names: AxisNames(vec![Some("rows".to_string())]),
    };
    assert_eq!(
        math_of(src, "p"),
        (HirMathOp::Pow, Some(HirType::F32), tensor)
    );
}

/// The function of each top-level binding of `body` that is math, in order. The
/// derivative binds every value it computes, so this is every math node it emits.
fn math_ops(body: &[HirStmt]) -> Vec<HirMathOp> {
    body.iter()
        .filter_map(|stmt| match stmt {
            HirStmt::VarDecl {
                init:
                    Some(HirExpr {
                        kind: HirExprKind::Math { op, .. },
                        ..
                    }),
                ..
            } => Some(*op),
            _ => None,
        })
        .collect()
}

fn reverse_body(src: &str) -> Vec<HirStmt> {
    lower(src)
        .items
        .into_iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name == "__loss__rev" => Some(f.body),
            _ => None,
        })
        .expect("the derivative is generated")
}

#[test]
fn abs_differentiates_through_sign_and_pow_through_a_lowered_power() {
    let body = reverse_body(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val a = w.abs()
    val p = w.pow(3.0)
    return Tensor::scalar(a.sum() + p.sum())
}
"#,
    );
    let ops = math_ops(&body);
    // The forward replay, then the reverse pass's own: one `sign` and one `x^(p - 1)`.
    assert_eq!(
        ops,
        [
            HirMathOp::Abs,
            HirMathOp::Pow,
            HirMathOp::Pow,
            HirMathOp::Sign
        ]
    );
}

#[test]
fn exp_and_tanh_read_their_own_value_instead_of_recomputing_it() {
    let body = reverse_body(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val e = w.exp()
    val t = w.tanh()
    return Tensor::scalar(e.sum() + t.sum())
}
"#,
    );
    assert_eq!(math_ops(&body), [HirMathOp::Exp, HirMathOp::Tanh]);
}

#[test]
fn half_precision_math_is_refused_at_the_call() {
    let src = r#"
@grad(wrt: [w])
func loss(w: &mut Tensor<f32, [2]>, h: &Tensor<bf16, [2]>) -> Tensor<f32, []> {
    val r = h.sqrt()
    return Tensor::scalar(w.sum())
}
"#;
    let ast = syntax_parsing::parse(src).expect("source should parse");
    let error = lower_program(&ast).expect_err("half precision has no derivative");
    let LoweringError::NotDifferentiable { span, .. } = error else {
        panic!("expected NotDifferentiable, got {error:?}");
    };
    assert_eq!(span.start, src.find("h.sqrt()").expect("call in source"));
}
