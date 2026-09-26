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

fn calls_in(expr: &neuro_hir::HirExpr) -> bool {
    match &expr.kind {
        HirExprKind::Call { .. } => true,
        HirExprKind::Binary { left, right, .. } => calls_in(left) || calls_in(right),
        HirExprKind::Unary { operand, .. } | HirExprKind::Reference { operand, .. } => {
            calls_in(operand)
        }
        _ => false,
    }
}

#[test]
fn a_call_is_differentiated_through_the_callee_body() {
    // The callee is declared after the caller: derivatives are built once every function
    // is lowered.
    let program = lower(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val total = w.sum()
    return Tensor::scalar(scaled(total) + scaled(w[0]))
}

func scaled(x: f32) -> f32 {
    val y = x * 2.0
    return y
}
"#,
    );
    let reverse = item_function(&program, "__loss__rev");
    let calls = all_stmts(&reverse.body).into_iter().any(|stmt| match stmt {
        HirStmt::VarDecl {
            init: Some(init), ..
        } => calls_in(init),
        HirStmt::Assign { value, .. } => calls_in(value),
        _ => false,
    });
    assert!(
        !calls,
        "the derivative should inline the callee, not call it"
    );
    // Each call site gets its own copy of the callee's `* 2.0`, and each copy its adjoint.
    let doublings = all_stmts(&reverse.body)
        .into_iter()
        .filter(|stmt| {
            matches!(stmt, HirStmt::VarDecl { init: Some(init), .. }
                if matches!(&init.kind, HirExprKind::Binary { op: ast_types::BinaryOp::Multiply, right, .. }
                    if matches!(right.kind, HirExprKind::Literal(shared_types::Literal::Float(v, _)) if v == 2.0)))
        })
        .count();
    assert_eq!(doublings, 4, "two forward doublings and two adjoint ones");
}

#[test]
fn a_recursive_call_is_refused_at_the_call() {
    refusal_at(
        r#"
func halve(x: f32, n: i32) -> f32 {
    if n > 0 { return halve(x * 0.5, n - 1) }
    return x
}

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(halve(w.sum(), 3))
}
"#,
        "halve(x * 0.5, n - 1)",
    );
}

#[test]
fn a_callee_writing_through_its_mut_parameter_is_refused_there() {
    refusal_at(
        r#"
func doubled_sum(x: &mut Tensor<f32, [2]>) -> f32 {
    x *= 2.0
    return x.sum()
}

@grad
func loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    return Tensor::scalar(doubled_sum(w))
}
"#,
        "x *= 2.0",
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
fn a_for_over_a_range_is_the_counted_while_it_desugars_to() {
    let program = lower(
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
    );
    let reverse = item_function(&program, "__loss__rev");
    let stmts = all_stmts(&reverse.body);
    assert!(
        !stmts
            .iter()
            .any(|stmt| matches!(stmt, HirStmt::ForRange { .. })),
        "the derivative walks a range as a while, got {stmts:?}"
    );
    assert_eq!(
        stmts
            .iter()
            .filter(|stmt| matches!(stmt, HirStmt::While { .. }))
            .count(),
        2
    );
}

#[test]
fn a_for_over_a_collection_is_refused_at_the_statement() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2]>, scales: [f32; 2]) -> Tensor<f32, []> {
    mut acc = w * 1.0
    for scale in scales {
        acc = &acc * scale
    }
    return Tensor::scalar(acc.sum())
}
"#,
        "for scale",
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

#[test]
fn a_slice_at_a_computed_position_is_refused_at_the_slice() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2, 2]>, row: i64) -> Tensor<f32, []> {
    val picked = w[row, ..]
    return Tensor::scalar(picked.sum())
}
"#,
        "w[row, ..]",
    );
}

#[test]
fn an_einsum_along_a_diagonal_is_refused_at_the_contraction() {
    refusal_at(
        r#"
@grad
func loss(w: &mut Tensor<f32, [2, 2]>) -> Tensor<f32, []> {
    val trace = einsum("ii->", w)
    return Tensor::scalar(trace)
}
"#,
        "einsum(",
    );
}

const TRAINING_STEP: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [2, 3]>, b: &mut Tensor<f32, [3]>, scale: f32) -> Tensor<f32, []> {
    val shifted = w + b
    val total = shifted.sum()
    return Tensor::scalar(total * scale)
}

func main() -> i32 {
    mut w = Tensor::<f32, [2, 3]>::ones()
    mut b = Tensor::<f32, [3]>::ones()
    val l = loss(&mut w, &mut b, 2.0f32)
    l.backward()
    w -= 0.1f32 * w.grad()
    val evaluated = loss(&mut w, &mut b, 1.0f32)
    return 0
}
"#;

/// The name a callee lowers to, when `expr` is a call by name.
fn callee_name(expr: &neuro_hir::HirExpr) -> Option<&str> {
    match &expr.kind {
        HirExprKind::Call { callee, .. } => match &callee.kind {
            HirExprKind::Variable(name) => Some(name),
            _ => None,
        },
        _ => None,
    }
}

#[test]
fn a_backward_runs_the_derivative_where_the_call_ran() {
    let program = lower(TRAINING_STEP);
    let body = &item_function(&program, "main").body;
    let decls: Vec<(&str, Option<&str>)> = body
        .iter()
        .filter_map(|stmt| match stmt {
            HirStmt::VarDecl { name, init, .. } => {
                Some((name.as_str(), init.as_ref().and_then(callee_name)))
            }
            _ => None,
        })
        .collect();
    // The call a `.backward()` pairs with becomes the derivative, whose loss is unpacked
    // into the original binding; a call nothing backpropagates stays the primal.
    assert_eq!(
        decls[2..],
        [
            ("__backward_1", Some("__loss__rev")),
            ("l", None),
            ("evaluated", Some("loss")),
        ]
    );
}

#[test]
fn a_backward_moves_one_gradient_into_each_differentiated_argument() {
    let program = lower(TRAINING_STEP);
    let body = &item_function(&program, "main").body;
    let writes: Vec<String> = body
        .iter()
        .filter_map(|stmt| {
            let HirStmt::Expr(expr) = stmt else {
                return None;
            };
            let HirExprKind::Call { callee, args } = &expr.kind else {
                return None;
            };
            let HirExprKind::FieldAccess { field, .. } = &callee.kind else {
                return None;
            };
            let [gradient] = args.as_slice() else {
                return None;
            };
            let HirExprKind::FieldAccess { field: from, .. } = &gradient.kind else {
                return None;
            };
            Some(format!("{field}({from})"))
        })
        .collect();
    // `scale` is a plain number: nothing is written for it.
    assert_eq!(writes, ["__set_grad(w)", "__set_grad(b)"]);
}

const METHOD_LOSS: &str = r#"
struct Damping { factor: f32 }

struct Fit {
    y: Tensor<f32, [2]>,
    damping: Damping
}

impl Fit {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>, scale: f32) -> Tensor<f32, []> {
        val damped = w * self.damping.factor
        val squares = &damped * &damped
        return Tensor::scalar(squares.sum() * scale + self.y.sum())
    }
}

func main() -> i32 {
    val fit = Fit { y: [1.0, 2.0], damping: Damping { factor: 0.5 } }
    mut w = Tensor::<f32, [2]>::ones()
    val l = fit.loss(&mut w, 2.0f32)
    l.backward()
    return 0
}
"#;

fn inherent_method<'a>(
    program: &'a HirProgram,
    type_name: &str,
    name: &str,
) -> &'a neuro_hir::HirMethod {
    program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Impl(block) if block.type_name == type_name => {
                block.methods.iter().find(|method| method.name == name)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("method '{type_name}.{name}' not found"))
}

/// A method's derivative is a method of the same type, so it takes the receiver the way
/// the primal does; its bundle is named for the `Type__method` key.
#[test]
fn a_grad_method_gains_a_derivative_method_beside_it() {
    let program = lower(METHOD_LOSS);
    let primal = inherent_method(&program, "Fit", "loss");
    let reverse = inherent_method(&program, "Fit", "__loss__rev");
    assert_eq!(reverse.self_param, primal.self_param);
    assert_eq!(reverse.params, primal.params);
    assert_eq!(
        reverse.return_type,
        HirType::Tuple(vec![
            tensor(&[]),
            HirType::Struct("GradsOf_Fit__loss".to_string())
        ])
    );
    let bundle = item_struct(&program, "GradsOf_Fit__loss");
    let fields: Vec<_> = bundle
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(fields, ["w"]);
}

/// The receiver is a constant: a number field is read by value, and a tensor field, which
/// the primal can only read in place, is copied into a binding of the derivative's own.
#[test]
fn a_constant_receivers_fields_are_read_into_the_derivative() {
    let program = lower(METHOD_LOSS);
    let reverse = inherent_method(&program, "Fit", "__loss__rev");
    let reads: Vec<String> = reverse
        .body
        .iter()
        .filter_map(|stmt| match stmt {
            HirStmt::VarDecl {
                init: Some(init), ..
            } => match &init.kind {
                HirExprKind::FieldAccess { field, .. } => Some(field.clone()),
                HirExprKind::Call { callee, .. } => match &callee.kind {
                    HirExprKind::FieldAccess { object, field } => match &object.kind {
                        HirExprKind::FieldAccess { field: of, .. } => Some(format!("{of}.{field}")),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(reads, ["factor", "y.clone"]);
}

#[test]
fn a_backward_on_a_method_call_runs_the_derivative_method() {
    let program = lower(METHOD_LOSS);
    let body = &item_function(&program, "main").body;
    let reverse = body
        .iter()
        .find_map(|stmt| match stmt {
            HirStmt::VarDecl {
                name,
                init: Some(init),
                ..
            } if name == "__backward_1" => match &init.kind {
                HirExprKind::Call { callee, .. } => match &callee.kind {
                    HirExprKind::FieldAccess { object, field } => {
                        Some((object.ty.clone(), field.clone()))
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .expect("the paired call becomes a call of the derivative method");
    assert_eq!(
        reverse,
        (
            HirType::Struct("Fit".to_string()),
            "__loss__rev".to_string()
        )
    );
}

/// A field whose value is neither a number nor a tensor has no copy the replay can take.
#[test]
fn a_field_holding_a_string_is_refused() {
    let error = lowering_error(
        r#"
struct Named { label: string }

impl Named {
    @grad
    func loss(&self, w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
        val label = self.label
        return Tensor::scalar(w.sum())
    }
}
"#,
    );
    assert!(
        matches!(error, LoweringError::NotDifferentiable { ref construct, .. } if construct.contains("field")),
        "got {error:?}"
    );
}
