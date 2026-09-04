use super::lower;
use neuro_hir::{HirItem, HirType};

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
