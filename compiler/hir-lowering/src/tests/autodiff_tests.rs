// Reverse-mode derivative generation for `@grad` functions.
//
// These pin the shape of what the transform emits. Whether the numbers it computes are
// right is `tools/grad_differential.py`'s job, against finite differences of compiled code.

use super::{function_names, lower};
use crate::{lower_program, LoweringError};
use neuro_hir::{HirExprKind, HirItem, HirProgram, HirStmt, HirType};

const WEIGHTED_LOSS: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [2, 3]>, b: &mut Tensor<f32, [3]>, scale: f32) -> Tensor<f32, []> {
    val shifted = w + b
    val total = shifted.sum()
    return Tensor::scalar(total * scale)
}
"#;

fn item_struct<'a>(program: &'a HirProgram, name: &str) -> &'a neuro_hir::HirStruct {
    program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Struct(s) if s.name == name => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| panic!("struct '{name}' not found"))
}

fn item_function<'a>(program: &'a HirProgram, name: &str) -> &'a neuro_hir::HirFunction {
    program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("function '{name}' not found"))
}

fn tensor(extents: &[usize]) -> HirType {
    HirType::Tensor {
        element: Box::new(HirType::F32),
        shape: neuro_hir::static_shape(extents),
        names: neuro_hir::AxisNames::default(),
    }
}

fn lowering_error(src: &str) -> LoweringError {
    let ast = syntax_parsing::parse(src).expect("source should parse");
    lower_program(&ast).expect_err("the transform should refuse this body")
}

#[test]
fn the_bundle_holds_one_owned_gradient_per_tensor_parameter() {
    let program = lower(WEIGHTED_LOSS);
    let bundle = item_struct(&program, "GradsOf_loss");
    let fields: Vec<_> = bundle
        .fields
        .iter()
        .map(|field| (field.name.as_str(), field.ty.clone()))
        .collect();
    // `scale` is a constant: a scalar parameter is never differentiated.
    assert_eq!(fields, vec![("w", tensor(&[2, 3])), ("b", tensor(&[3]))]);
}

#[test]
fn the_derivative_takes_the_primal_parameters_and_returns_loss_and_bundle() {
    let program = lower(WEIGHTED_LOSS);
    let primal = item_function(&program, "loss");
    let reverse = item_function(&program, "__loss__rev");
    assert_eq!(reverse.params, primal.params);
    assert_eq!(
        reverse.return_type,
        HirType::Tuple(vec![
            tensor(&[]),
            HirType::Struct("GradsOf_loss".to_string())
        ])
    );
    assert!(matches!(
        reverse.body.last(),
        Some(HirStmt::Return {
            value: Some(value),
            ..
        }) if matches!(value.kind, HirExprKind::TupleLiteral { .. })
    ));
}

#[test]
fn the_primal_survives_and_only_grad_functions_gain_a_derivative() {
    let program = lower(&format!(
        "{WEIGHTED_LOSS}\nfunc plain(x: f32) -> f32 {{\n    return x * 2.0\n}}\n"
    ));
    let names = function_names(&program);
    assert!(names.contains(&"loss".to_string()));
    assert!(names.contains(&"__loss__rev".to_string()));
    assert!(!names.iter().any(|name| name == "__plain__rev"));
}

#[test]
fn a_parameter_the_loss_never_reads_gets_a_zero_gradient() {
    let program = lower(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, unused: &mut Tensor<f32, [4]>) -> Tensor<f32, []> {
    return Tensor::scalar(w.sum())
}
"#,
    );
    let reverse = item_function(&program, "__loss__rev");
    let zero_fill = reverse.body.iter().any(|stmt| {
        matches!(stmt, HirStmt::VarDecl { ty, init: Some(init), .. }
            if *ty == tensor(&[4]) && matches!(init.kind, HirExprKind::TensorFill { .. }))
    });
    assert!(
        zero_fill,
        "the unread parameter's gradient should be a zero fill"
    );
}

#[test]
fn a_call_in_the_body_is_refused_at_the_call() {
    let src = r#"
func helper(x: f32) -> f32 {
    return x * 2.0
}

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val total = w.sum()
    return Tensor::scalar(helper(total))
}
"#;
    let error = lowering_error(src);
    let LoweringError::NotDifferentiable { span, .. } = error else {
        panic!("expected NotDifferentiable, got {error:?}");
    };
    assert_eq!(
        span.start,
        src.find("helper(total)").expect("call in source")
    );
}

#[test]
fn a_mutable_binding_is_refused_at_the_statement() {
    let src = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut total = w.sum()
    return Tensor::scalar(total)
}
"#;
    let error = lowering_error(src);
    let LoweringError::NotDifferentiable { span, .. } = error else {
        panic!("expected NotDifferentiable, got {error:?}");
    };
    assert_eq!(
        span.start,
        src.find("mut total").expect("binding in source")
    );
}

#[test]
fn a_max_reduction_is_refused() {
    let error = lowering_error(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(w.max())
}
"#,
    );
    assert!(
        matches!(error, LoweringError::NotDifferentiable { .. }),
        "got {error:?}"
    );
}
