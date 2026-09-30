// `@kernel`: the launch shape on the function, and the grid positions its body reads.

use neuro_hir::{HirExprKind, HirGridIndex, HirItem, HirStmt, HirTarget, HirType};

use super::{binding_init, function_body, lower};

const PROGRAM: &str = r#"
@kernel(threads: [8, 4])
func fill<N>(a: Tensor<f32, [N, 3]>, out: KernelOut<Tensor<f32, [N, 3]>>) {
    val row = thread_id.x
    val block = block_id.y
    if row < 5 && block < 1 {
        out[row, 0] = a[row, 1]
    }
}

@kernel(threads: [32])
func shadowed(s: Tensor<i32, [4]>, out: KernelOut<Tensor<i32, [4]>>) {
    val thread_id = 2u32
    out[0] = s[1] + thread_id as i32
}

func main() -> i32 {
    mut t: Tensor<f32, [5, 3]> = Tensor::zeros()
    val a: Tensor<f32, [5, 3]> = Tensor::ones()
    fill(a, &mut t)
    mut u: Tensor<i32, [4]> = Tensor::zeros()
    val s: Tensor<i32, [4]> = Tensor::zeros()
    shadowed(s, &mut u)
    shadowed(s, &mut u)
    return 0
}
"#;

#[test]
fn threads_becomes_the_block_shape_padded_with_ones() {
    let program = lower(PROGRAM);
    let targets: Vec<_> = program
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Function(f) => Some((f.name.clone(), f.target)),
            _ => None,
        })
        .collect();
    assert!(
        targets.iter().any(|(name, target)| name.starts_with("fill")
            && *target == HirTarget::Kernel { threads: [8, 4, 1] }),
        "a generic kernel's instance keeps the launch shape: {targets:?}"
    );
    assert!(
        targets.iter().any(|(name, target)| name == "shadowed"
            && *target
                == HirTarget::Kernel {
                    threads: [32, 1, 1]
                }),
        "{targets:?}"
    );
    assert!(targets.contains(&("main".to_string(), HirTarget::Host)));
}

#[test]
fn a_grid_field_is_a_position_of_the_thread_or_its_block() {
    let program = lower(PROGRAM);
    let name = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name.starts_with("fill") => Some(f.name.clone()),
            _ => None,
        })
        .expect("an instance of `fill`");
    let body = function_body(&program, &name);
    for (binding, of, axis) in [
        ("row", HirGridIndex::Thread, 0),
        ("block", HirGridIndex::Block, 1),
    ] {
        let init = binding_init(body, binding);
        assert_eq!(init.kind, HirExprKind::GridPosition { of, axis });
        assert_eq!(init.ty, HirType::U32);
    }
}

#[test]
fn a_local_named_like_a_grid_position_is_an_ordinary_variable() {
    let program = lower(PROGRAM);
    let body = function_body(&program, "shadowed");
    let init = binding_init(body, "thread_id");
    assert!(
        matches!(init.kind, HirExprKind::Literal(_)),
        "{:?}",
        init.kind
    );
}

/// A kernel takes each tensor as the reference its caller lends: a bare `Tensor` input is
/// `&Tensor`, borrowed at the call, and `KernelOut<Tensor>` is `&mut Tensor`.
#[test]
fn kernel_tensors_are_references_the_call_lends() {
    let program = lower(PROGRAM);
    let shadowed = program
        .items
        .iter()
        .find_map(|item| match item {
            HirItem::Function(f) if f.name == "shadowed" => Some(f),
            _ => None,
        })
        .expect("`shadowed`");
    let mutability: Vec<_> = shadowed
        .params
        .iter()
        .map(|param| match &param.ty {
            HirType::Reference { mutable, .. } => Some(*mutable),
            _ => None,
        })
        .collect();
    assert_eq!(mutability, [Some(false), Some(true)]);

    let calls: Vec<_> = function_body(&program, "main")
        .iter()
        .filter_map(|stmt| match stmt {
            HirStmt::Expr(expr) => match &expr.kind {
                HirExprKind::Call { args, .. } => Some(args),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 3);
    for args in calls {
        for (arg, mutable) in args.iter().zip([false, true]) {
            assert!(
                matches!(&arg.kind, HirExprKind::Reference { mutable: m, .. } if *m == mutable),
                "{:?}",
                arg.kind
            );
        }
    }
}
