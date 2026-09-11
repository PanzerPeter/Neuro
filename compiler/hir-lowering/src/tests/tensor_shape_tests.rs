// `.flatten` is written positionally here rather than as `.flatten(dims: [...])`:
// a label is resolved by the `argument-binding` pass, which runs before lowering in the
// pipeline but not in this slice's test harness. The labelled spelling is covered end to
// end by `neurc`'s integration tests.

use super::{binding_init, function_body, lower};

use neuro_hir::{HirExprKind, HirType};

/// The shape a shape-manipulation call lowers to, and where each result axis came from.
fn cast_of(src: &str, binding: &str) -> (Vec<usize>, Option<Vec<usize>>) {
    let program = lower(src);
    let body = function_body(&program, "main");
    let init = binding_init(body, binding);
    let HirExprKind::TensorShapeCast { permutation, .. } = &init.kind else {
        panic!(
            "'{binding}' should lower to a shape cast, got {:?}",
            init.kind
        );
    };
    let HirType::Tensor { shape, .. } = &init.ty else {
        panic!("a shape cast should produce a tensor type, got {}", init.ty);
    };
    (shape.clone(), permutation.clone())
}

#[test]
fn transpose_lowers_to_a_swapped_permutation() {
    let (shape, permutation) = cast_of(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = Tensor::<i32, [2, 3]>::ones()
    val t = m.t()
    return 0
}
"#,
        "t",
    );
    assert_eq!(shape, vec![3, 2]);
    assert_eq!(permutation, Some(vec![1, 0]));
}

/// A reshape keeps the element order, so it carries no permutation: the backend reads
/// that as "re-describe the receiver's buffer" rather than "copy it".
#[test]
fn reshape_lowers_without_a_permutation() {
    let (shape, permutation) = cast_of(
        r#"
func main() -> i32 {
    val m: Tensor<i32, [3, 4]> = Tensor::<i32, [3, 4]>::ones()
    val r = m.reshape([2, -1])
    return 0
}
"#,
        "r",
    );
    assert_eq!(shape, vec![2, 6]);
    assert_eq!(permutation, None);
}

/// Lowering resolves a dimension name against the receiver's own shape, which is what
/// the axis names on `HirType::Tensor` are carried for.
#[test]
fn permute_resolves_dimension_names_during_lowering() {
    let (shape, permutation) = cast_of(
        r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 3, width: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val p = image.permute([height, width, channels])
    return 0
}
"#,
        "p",
    );
    assert_eq!(shape, vec![3, 4, 2]);
    assert_eq!(permutation, Some(vec![1, 2, 0]));
}

#[test]
fn flatten_merges_the_named_run_and_keeps_the_rest() {
    let (shape, permutation) = cast_of(
        r#"
func main() -> i32 {
    val batch: Tensor<i32, [batch: 2, seq_len: 3, embed: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val f = batch.flatten([seq_len, embed])
    return 0
}
"#,
        "f",
    );
    assert_eq!(shape, vec![2, 12]);
    assert_eq!(permutation, None);
}

#[test]
fn a_bare_flatten_merges_every_axis() {
    let (shape, permutation) = cast_of(
        r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 3, 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val f = t.flatten()
    return 0
}
"#,
        "f",
    );
    assert_eq!(shape, vec![24]);
    assert_eq!(permutation, None);
}

/// A permuted shape carries its names along, so a later call can still name an axis by
/// the name it had before the reorder.
#[test]
fn a_permuted_shape_keeps_its_names() {
    let program = lower(
        r#"
func main() -> i32 {
    val image: Tensor<i32, [channels: 2, height: 3, width: 4]> = Tensor::<i32, [2, 3, 4]>::ones()
    val p = image.permute([height, width, channels])
    val f = p.flatten([width, channels])
    return 0
}
"#,
    );
    let body = function_body(&program, "main");
    let HirType::Tensor { shape, .. } = &binding_init(body, "f").ty else {
        panic!("a flatten should produce a tensor type");
    };
    assert_eq!(shape, &vec![3, 8]);
}
