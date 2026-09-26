use super::semantic_errors;
use crate::errors::TypeError;

/// A scalar keeps its own type and a tensor its element type and shape, half precision
/// included. Each result is asserted through an annotation that accepts only that type.
#[test]
fn math_methods_produce_their_specified_types() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: f32 = 2.0
    val b: f64 = 3.0
    val ea: f32 = a.exp()
    val lb: f64 = b.log()
    val r: f64 = (b * b + b * b).sqrt()
    val p: f32 = a.pow(3.0)
    val t: Tensor<f32, [2, 3]> = Tensor::<f32, [2, 3]>::ones()
    val tt: Tensor<f32, [2, 3]> = t.tanh()
    val tp: Tensor<f32, [2, 3]> = t.pow(2.0f32)
    val h: Tensor<bf16, [4]> = Tensor::<bf16, [4]>::ones()
    val ha: Tensor<bf16, [4]> = h.abs()
    val hp: Tensor<bf16, [4]> = h.pow(2.0bf16)
    val d: Tensor<f64, []> = Tensor::scalar(b)
    val ds: Tensor<f64, []> = d.sqrt()
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "math methods should check; got {errors:?}"
    );
}

/// The receiver is read, so a tensor stays usable afterwards and a borrow is accepted.
#[test]
fn a_tensor_receiver_is_read_not_moved() {
    let errors = semantic_errors(
        r#"
func total(t: &Tensor<f32, [3]>) -> f32 {
    return t.exp().sum()
}

func main() -> i32 {
    val t: Tensor<f32, [3]> = Tensor::<f32, [3]>::ones()
    val e = t.exp()
    val l = t.log()
    val s = total(&t) + e.sum() + l.sum() + t.sum()
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the receiver should survive; got {errors:?}"
    );
}

/// Integers and half-precision scalars have none of the methods.
#[test]
fn integer_and_half_scalar_receivers_have_no_math() {
    for (receiver, declaration) in [
        ("i", "val i: i32 = 4"),
        ("h", "val h: f16 = 1.5f16"),
        ("n", "val n: Tensor<i32, [2]> = Tensor::<i32, [2]>::ones()"),
    ] {
        let errors = semantic_errors(&format!(
            "func main() -> i32 {{\n    {declaration}\n    val x = {receiver}.sqrt()\n    return 0\n}}\n"
        ));
        assert!(
            matches!(errors.as_slice(), [TypeError::MethodNotFound { method_name, .. }] if method_name == "sqrt"),
            "`{receiver}.sqrt()` should have no such method; got {errors:?}"
        );
    }
}

/// `.pow` takes exactly one exponent of the element type and the rest take nothing.
#[test]
fn math_arguments_are_checked() {
    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val a: f32 = 2.0
    val t: Tensor<f32, [2]> = Tensor::<f32, [2]>::ones()
    val x = a.exp(1.0)
    val y = a.pow()
    return 0
}
"#,
    );
    let arity: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, TypeError::ArgumentCountMismatch { .. }))
        .collect();
    assert_eq!(
        arity.len(),
        2,
        "both arities should be refused; got {errors:?}"
    );

    let errors = semantic_errors(
        r#"
func main() -> i32 {
    val t: Tensor<f32, [2]> = Tensor::<f32, [2]>::ones()
    val y = t.pow(2.0f64)
    return 0
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::Mismatch { .. }]),
        "an `f64` exponent for an `f32` tensor should be refused; got {errors:?}"
    );
}
