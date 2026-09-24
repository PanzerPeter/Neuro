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

/// Every statement of `body`, nested blocks included, in source order.
fn all_stmts(body: &[HirStmt]) -> Vec<&HirStmt> {
    let mut found = Vec::new();
    for stmt in body {
        found.push(stmt);
        match stmt {
            HirStmt::If {
                then_block,
                else_block,
                ..
            } => {
                found.extend(all_stmts(then_block));
                found.extend(all_stmts(else_block.as_deref().unwrap_or_default()));
            }
            HirStmt::While { body, .. } => found.extend(all_stmts(body)),
            HirStmt::Expr(expr) => {
                if let HirExprKind::Loop { body, .. } = &expr.kind {
                    found.extend(all_stmts(body));
                }
            }
            _ => {}
        }
    }
    found
}

fn refusal_at(src: &str, needle: &str) {
    let error = lowering_error(src);
    let LoweringError::NotDifferentiable { span, .. } = error else {
        panic!("expected NotDifferentiable, got {error:?}");
    };
    assert_eq!(span.start, src.find(needle).expect("construct in source"));
}

const LOOPED_LOSS: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    mut step = 0
    while step < 3 {
        acc = &acc * w
        step += 1
    }
    return Tensor::scalar(acc.sum())
}
"#;

#[test]
fn a_while_loop_is_counted_forward_and_undone_iteration_by_iteration() {
    let program = lower(LOOPED_LOSS);
    let reverse = item_function(&program, "__loss__rev");
    let stmts = all_stmts(&reverse.body);
    let forward_loops = stmts
        .iter()
        .filter(
            |stmt| matches!(stmt, HirStmt::Expr(e) if matches!(e.kind, HirExprKind::Loop { .. })),
        )
        .count();
    let counted_loops = stmts
        .iter()
        .filter(|stmt| matches!(stmt, HirStmt::While { .. }))
        .count();
    // One forward loop, then the backward walk and, inside it, the replay that rebuilds
    // each iteration's carried values: no value is kept per iteration.
    assert_eq!(forward_loops, 1);
    assert_eq!(counted_loops, 2);
}

#[test]
fn a_reassigned_binding_is_a_mutable_slot_of_the_derivative() {
    let program = lower(LOOPED_LOSS);
    let reverse = item_function(&program, "__loss__rev");
    let slot = reverse.body.iter().any(
        |stmt| matches!(stmt, HirStmt::VarDecl { mutable: true, ty, .. } if *ty == tensor(&[2])),
    );
    assert!(slot, "the carried tensor should live in a `mut` binding");
}

#[test]
fn a_branch_is_taken_again_in_the_reverse_pass() {
    let program = lower(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val x = w[0]
    mut s = x * x
    if x > 0.0 {
        s = s * 3.0
    }
    return Tensor::scalar(s)
}
"#,
    );
    let reverse = item_function(&program, "__loss__rev");
    let branches = all_stmts(&reverse.body)
        .iter()
        .filter(|stmt| {
            matches!(
                stmt,
                HirStmt::If {
                    else_block: Some(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(branches, 2, "one forward branch and its reverse");
}

#[test]
fn an_inactive_branch_needs_no_reverse() {
    let program = lower(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, flag: bool) -> Tensor<f32, []> {
    mut k: f32 = 1.0
    if flag {
        k = 2.0
    }
    return Tensor::scalar(w.sum() * k)
}
"#,
    );
    let reverse = item_function(&program, "__loss__rev");
    let branches = all_stmts(&reverse.body)
        .iter()
        .filter(|stmt| matches!(stmt, HirStmt::If { .. }))
        .count();
    assert_eq!(
        branches, 1,
        "only the forward branch: nothing active leaves it"
    );
}

#[test]
fn a_break_is_refused_at_the_statement() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    while true {
        acc = &acc * w
        break
    }
    return Tensor::scalar(acc.sum())
}
"#,
        "break",
    );
}

#[test]
fn a_for_loop_is_refused_at_the_statement() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    for i in 0..2 {
        acc = &acc * w
    }
    return Tensor::scalar(acc.sum())
}
"#,
        "for i",
    );
}

#[test]
fn a_return_inside_a_loop_is_refused_at_the_return() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    mut acc = w * 1.0
    while acc.sum() < 10.0 {
        return Tensor::scalar(acc.sum())
    }
    return Tensor::scalar(acc.sum())
}
"#,
        "return Tensor::scalar(acc.sum())
    }",
    );
}

#[test]
fn an_assignment_to_a_parameter_is_refused() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    w *= 2.0
    return Tensor::scalar(w.sum())
}
"#,
        "w *= 2.0",
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
