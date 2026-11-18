//! `.backward()`, `.grad()` and `.zero_grad()` end to end: the gradient slot filled by the
//! derivative a `@grad` call pairs with, read back, cleared, and released with its tensor.
//!
//! The numbers here are analytic (`d/dw sum(w * w) * s = 2 * w * s`), so each program can
//! pin its result in its exit code; `grad_differential` checks the general case against
//! finite differences.
use crate::compile_harness::CompileTest;
use std::process::Command;

const LOSS: &str = r#"
@grad
func loss(w: &mut Tensor<f32, [3]>, scale: f32) -> Tensor<f32, []> {
    val squares = w * w
    return Tensor::scalar(squares.sum() * scale)
}
"#;

fn run(test: &CompileTest, name: &str, main: &str) -> i32 {
    test.compile_and_run(name, &format!("{LOSS}\n{main}"))
        .expect("compile/run failed")
}

/// The stderr of a program that must abort, or a panic of the test when it ran to the end.
fn panic_message(test: &CompileTest, name: &str, main: &str) -> String {
    let path = test.write_source(name, &format!("{LOSS}\n{main}"));
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(exe).output().expect("run executable");
    assert_ne!(output.status.code(), Some(0), "the program ran to the end");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn backward_parks_each_gradient_in_its_parameters_slot() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "grad_read.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = loss(&mut w, 2.0f32)
    l.backward()
    val g = w.grad()
    // 2 * w * 2 = [4, 8, 12], and the loss is (1 + 4 + 9) * 2 = 28.
    return (g[0] + g[1] + g[2]) as i32 + l.sum() as i32
}
"#,
    );
    assert_eq!(exit, 24 + 28);
}

#[test]
fn a_training_loop_in_a_pool_converges() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "grad_train.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    mut step = 0
    while step < 50 {
        pool {
            val l = loss(&mut w, 1.0f32)
            l.backward()
            w -= 0.25f32 * w.grad()
            w.zero_grad()
        }
        step += 1
    }
    // Each step halves w, so fifty of them leave it at zero to f32 precision.
    val total = w.sum()
    return if total < 0.0001f32 { 1 } else { 0 }
}
"#,
    );
    assert_eq!(exit, 1);
}

#[test]
fn a_gradient_computed_inside_a_pool_is_not_arena_memory() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "grad_pool.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    pool {
        val l = loss(&mut w, 1.0f32)
        l.backward()
    }
    // The second block reuses every byte the first one released. A gradient taken from
    // the arena would now read back as sevens.
    pool {
        val junk = Tensor::<f32, [1024]>::zeros()
        val sevens = junk + 7.0f32
        assert(sevens[0] == 7.0f32)
    }
    val g = w.grad()
    return (g[0] + g[1] + g[2]) as i32
}
"#,
    );
    assert_eq!(exit, 12);
}

#[test]
fn a_second_backward_replaces_the_gradient() {
    let test = CompileTest::new();
    let exit = run(
        &test,
        "grad_replace.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val first = loss(&mut w, 1.0f32)
    first.backward()
    val second = loss(&mut w, 10.0f32)
    second.backward()
    val g = w.grad()
    return g[2] as i32
}
"#,
    );
    // 2 * 3 * 10, the second call's gradient alone.
    assert_eq!(exit, 60);
}

#[test]
fn reading_an_empty_slot_panics_at_the_read() {
    let test = CompileTest::new();
    let stderr = panic_message(
        &test,
        "grad_empty.nr",
        r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = loss(&mut w, 1.0f32)
    l.backward()
    w.zero_grad()
    val g = w.grad()
    return 0
}
"#,
    );
    assert!(stderr.contains("empty gradient slot"), "{stderr}");
    assert!(stderr.contains(":14:13"), "{stderr}");
}

#[test]
fn a_parameter_is_borrowed_from_its_call_to_its_backward() {
    let test = CompileTest::new();
    let diagnostics = test
        .check(
            "grad_borrow.nr",
            &format!(
                "{LOSS}\n{}",
                r#"
func main() -> i32 {
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = loss(&mut w, 1.0f32)
    val peek = w[0]
    l.backward()
    return 0
}
"#
            ),
        )
        .expect_err("w is mutably borrowed until the backward");
    assert!(diagnostics.contains("mutably borrowed"), "{diagnostics}");
}

/// A `@grad` method with no `wrt:` differentiates its tensor parameters and reads its
/// receiver as a constant, a nested number, a loop bound and a tensor field included.
#[test]
fn a_grad_method_backpropagates_with_its_receiver_as_a_constant() {
    let test = CompileTest::new();
    let exit = test
        .compile_and_run(
            "grad_method.nr",
            r#"
struct Gain { value: f32 }

struct Objective {
    gain: Gain,
    rounds: i32,
    offsets: Tensor<f32, [3]>
}

impl Objective {
    @grad
    func loss(&self, w: &mut Tensor<f32, [3]>) -> Tensor<f32, []> {
        mut total = 0.0f32
        mut round = 0
        while round < self.rounds {
            val squares = w * w
            total = total + squares.sum() * self.gain.value
            round += 1
        }
        return Tensor::scalar(total + w[0] * self.offsets.sum())
    }
}

func main() -> i32 {
    val objective = Objective { gain: Gain { value: 2.0 }, rounds: 3, offsets: [1.0, 2.0, 4.0] }
    mut w: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val l = objective.loss(&mut w)
    l.backward()
    val g = w.grad()
    // Loss 3 * 2 * 14 + 1 * 7 = 91; gradient 12 * w + [7, 0, 0] = [19, 24, 36].
    return l.sum() as i32 + (g[0] + g[1] + g[2]) as i32
}
"#,
        )
        .expect("compile/run failed");
    assert_eq!(exit, 91 + 79);
}
