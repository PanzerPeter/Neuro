use super::{binding_init, function_body, lower};
use neuro_hir::{HirExpr, HirExprKind, HirItem, HirType};
use shared_types::Literal;

/// The declared parameter types of the first function named `name`.
fn param_types(src: &str, name: &str) -> Vec<HirType> {
    let program = lower(src);
    for item in &program.items {
        if let HirItem::Function(f) = item {
            if f.name == name {
                return f.params.iter().map(|p| p.ty.clone()).collect();
            }
        }
    }
    panic!("no function named {name}");
}

#[test]
fn a_static_tensor_annotation_keeps_its_element_and_shape_in_hir() {
    let types = param_types(
        r#"
func forward(x: Tensor<f32, [3, 224, 224]>) { }

func main() -> i32 { return 0 }
"#,
        "forward",
    );
    assert_eq!(
        types[0],
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: vec![3, 224, 224],
        }
    );
}

#[test]
fn a_rank_zero_tensor_lowers_to_an_empty_shape() {
    let types = param_types(
        r#"
func loss(l: Tensor<f32, []>) { }

func main() -> i32 { return 0 }
"#,
        "loss",
    );
    assert_eq!(
        types[0],
        HirType::Tensor {
            element: Box::new(HirType::F32),
            shape: Vec::new(),
        }
    );
}

#[test]
fn a_tensor_type_renders_as_it_was_written() {
    let types = param_types(
        r#"
func forward(x: Tensor<u8, [2, 2]>) { }

func main() -> i32 { return 0 }
"#,
        "forward",
    );
    assert_eq!(types[0].to_string(), "Tensor<u8, [2, 2]>");
}

/// The lowered initializer of the first `val` named `name` in `main`.
fn main_binding(src: &str, name: &str) -> HirExpr {
    let program = lower(src);
    binding_init(function_body(&program, "main"), name).clone()
}

#[test]
fn a_nested_tensor_literal_flattens_to_row_major_order() {
    let init = main_binding(
        r#"
func main() -> i32 {
    val m: Tensor<f32, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [4.0, 5.0, 6.0]
    ]
    return 0
}
"#,
        "m",
    );
    let HirExprKind::TensorLiteral { elements } = &init.kind else {
        panic!("expected a tensor literal, got {:?}", init.kind);
    };
    let flat: Vec<f64> = elements
        .iter()
        .map(|e| match &e.kind {
            HirExprKind::Literal(Literal::Float(v, _)) => *v,
            other => panic!("expected a float leaf, got {other:?}"),
        })
        .collect();
    assert_eq!(flat, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    // The annotation types the leaves, so they are `f32` rather than narrowed `f64`.
    assert!(elements.iter().all(|e| e.ty == HirType::F32));
}

#[test]
fn an_unannotated_array_literal_stays_an_array() {
    let init = main_binding(
        r#"
func main() -> i32 {
    val a = [1.0, 2.0, 3.0]
    return 0
}
"#,
        "a",
    );
    assert!(matches!(init.kind, HirExprKind::ArrayLiteral { .. }));
    assert_eq!(
        init.ty,
        HirType::Array {
            element: Box::new(HirType::F64),
            size: 3,
        }
    );
}

#[test]
fn each_constructor_lowers_to_its_own_node() {
    let src = r#"
func main() -> i32 {
    val z = Tensor::<f32, [2, 2]>::zeros()
    val o = Tensor::<f32, [2, 2]>::ones()
    val e = Tensor::<f32, [2, 2]>::identity()
    val r = Tensor::<f32, [2, 2]>::random_normal(0.0f32, 1.0f32)
    val s: Tensor<f32, []> = Tensor::scalar(4.0)
    val f = Tensor::<f32, [3]>::from([1.0, 2.0, 3.0])
    return 0
}
"#;
    let program = lower(src);
    let body = function_body(&program, "main");
    assert!(matches!(
        binding_init(body, "z").kind,
        HirExprKind::TensorFill { .. }
    ));
    assert!(matches!(
        binding_init(body, "o").kind,
        HirExprKind::TensorFill { .. }
    ));
    assert!(matches!(
        binding_init(body, "e").kind,
        HirExprKind::TensorIdentity
    ));
    assert!(matches!(
        binding_init(body, "r").kind,
        HirExprKind::TensorRandomNormal { .. }
    ));
    // A rank-0 tensor is a one-element buffer, which is what the empty shape's
    // product makes it.
    let scalar = binding_init(body, "s");
    let HirExprKind::TensorLiteral { elements } = &scalar.kind else {
        panic!("expected a tensor literal for `scalar`");
    };
    assert_eq!(elements.len(), 1);
    let from = binding_init(body, "f");
    let HirExprKind::TensorLiteral { elements } = &from.kind else {
        panic!("expected a tensor literal for `from`");
    };
    assert_eq!(elements.len(), 3);
}

#[test]
fn a_turbofish_constructor_carries_its_own_tensor_type() {
    let init = main_binding(
        r#"
func main() -> i32 {
    val z = Tensor::<u8, [2, 3]>::zeros()
    return 0
}
"#,
        "z",
    );
    assert_eq!(
        init.ty,
        HirType::Tensor {
            element: Box::new(HirType::U8),
            shape: vec![2, 3],
        }
    );
}

/// The ownership surface keeps the receiver's tensor type: `.clone()` hands back a tensor
/// even when it was called through a borrow, and `.to()` hands back the tensor it consumed.
#[test]
fn the_ownership_methods_keep_the_receivers_tensor_type() {
    let program = lower(
        r#"
enum Device {
    CPU,
    GPU(i32)
}

func copy_of(t: &Tensor<f32, [2, 2]>) -> i32 {
    val copied = t.clone()
    return 0
}

func main() -> i32 {
    val a = Tensor::<f32, [2, 2]>::identity()
    val here = a.to(Device::CPU)
    return 0
}
"#,
    );
    let tensor = HirType::Tensor {
        element: Box::new(HirType::F32),
        shape: vec![2, 2],
    };
    let copied = binding_init(function_body(&program, "copy_of"), "copied");
    assert_eq!(copied.ty, tensor);
    let here = binding_init(function_body(&program, "main"), "here");
    assert_eq!(here.ty, tensor);
}

/// `.to()` carries its device through as the prelude enum, so the backend has a
/// discriminant to guard on rather than an untyped argument.
#[test]
fn a_device_transfer_lowers_its_argument_as_the_device_enum() {
    let program = lower(
        r#"
enum Device {
    CPU,
    GPU(i32)
}

func main() -> i32 {
    val a = Tensor::<f32, [2]>::zeros()
    val here = a.to(Device::CPU)
    return 0
}
"#,
    );
    let here = binding_init(function_body(&program, "main"), "here");
    let HirExprKind::Call { args, .. } = &here.kind else {
        panic!("`.to` should lower to a call, got {:?}", here.kind);
    };
    assert_eq!(args[0].ty, HirType::Enum("Device".to_string()));
}
