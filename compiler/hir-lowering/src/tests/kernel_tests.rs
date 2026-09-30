// `@kernel`: the launch shape on the function, and the grid positions its body reads.

use neuro_hir::{HirExprKind, HirGridIndex, HirItem, HirTarget, HirType};

use super::{binding_init, function_body, lower};

const PROGRAM: &str = r#"
@kernel(threads: [8, 4])
func fill<N>(out: &mut Tensor<f32, [N, 3]>) {
    val row = thread_id.x
    val block = block_id.y
    if row < 5 && block < 1 {
        out[row, 0] = 1.0
    }
}

@kernel(threads: [32])
func shadowed(out: &mut Tensor<i32, [4]>) {
    val thread_id = 2u32
    out[0] = thread_id as i32
}

func main() -> i32 {
    mut t: Tensor<f32, [5, 3]> = Tensor::zeros()
    fill(&mut t)
    mut u: Tensor<i32, [4]> = Tensor::zeros()
    shadowed(&mut u)
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
