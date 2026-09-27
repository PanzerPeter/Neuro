//! `.item()` end to end: the one element of a rank-0 tensor, read as its element type
//! through an owned receiver, a borrow and a temporary, and inside a `@grad` body, where
//! the read carries the gradient back to the tensor it came from.
use crate::compile_harness::CompileTest;

fn run(test: &CompileTest, name: &str, source: &str) -> i32 {
    test.compile_and_run(name, source)
        .expect("compile/run failed")
}

fn refused(test: &CompileTest, name: &str, source: &str) -> String {
    test.check(name, source)
        .expect_err("the program checked, but it must be refused")
}

#[test]
fn item_reads_the_element_of_owned_borrowed_and_temporary_receivers() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "item_values.nr",
        r#"
func peek(t: &Tensor<f32, []>) -> f32 {
    t.item()
}

func main() -> i32 {
    val v: Tensor<i32, [4]> = [1, 2, 3, 4]
    val total: Tensor<i32, []> = v.sum(axis: 0)
    val owned: i32 = total.item()
    val again: i32 = total.item()
    val temporary: i32 = v.max(axis: 0).item()
    val f: Tensor<f32, []> = Tensor::scalar(2.5)
    val borrowed: f32 = peek(&f)
    return owned + again + temporary + (borrowed * 2.0) as i32
}
"#,
    );
    assert_eq!(exit, 10 + 10 + 4 + 5);
}

#[test]
fn item_in_a_grad_body_carries_the_gradient() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "item_grad.nr",
        r#"
@grad
func loss(w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
    val squares = (w * w).sum(axis: 0)
    Tensor::scalar(squares.item() * 3.0)
}

func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = loss(&mut w)
    l.backward()
    // d/dw of 3 |w|^2 is 6 w.
    val g = w.grad()
    return (g[0] + g[1] + g[2]) as i32
}
"#,
    );
    assert_eq!(exit, 36);
}

#[test]
fn item_on_a_tensor_with_an_axis_is_refused() {
    let test = CompileTest::new();
    let err = refused(
        &test,
        "item_rank.nr",
        r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = Tensor::<i32, [2, 2]>::ones()
    return m.item()
}
"#,
    );
    assert!(err.contains("rank 2"), "unexpected diagnostic: {err}");
}

#[test]
fn item_with_an_argument_is_refused() {
    let test = CompileTest::new();
    let err = refused(
        &test,
        "item_arity.nr",
        r#"
func main() -> i32 {
    val s: Tensor<i32, []> = Tensor::scalar(3)
    return s.item(0)
}
"#,
    );
    assert!(!err.is_empty());
}
