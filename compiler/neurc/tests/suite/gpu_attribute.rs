// `@gpu`: a function whose body must run as a GPU kernel, and nowhere else.
//
// Only a neurc built with `--features mlir` can compile one, and only a machine with an
// NVIDIA GPU can run the result. CI's MLIR job has no GPU, so the run tests accept either
// outcome a GPU-less machine can produce, but each asserts it fully: the right answer on a
// GPU, or the startup diagnostic without one. The diagnostic itself is pinned by hiding
// every device with `CUDA_VISIBLE_DEVICES`, which works whether or not a GPU is present.

use crate::compile_harness::CompileTest;

/// Two kernels' worth of work, each checked against the same arithmetic done on the host.
#[cfg(feature = "mlir")]
const KERNELS: &str = r#"
@gpu
func blend(a: &Tensor<f32, [37, 45]>, b: &Tensor<f32, [37, 45]>) -> Tensor<f32, [37, 45]> {
    val s = a + b
    s * b
}

@gpu
func project(w: Tensor<f64, [3, 2]>, x: &Tensor<f64, [2, 2]>) -> Tensor<f64, [3, 2]> {
    w @ x
}

func main() -> i32 {
    println("main ran")
    val a = Tensor::<f32, [37, 45]>::ones()
    val b = &a * 2.0f32
    val c = blend(&a, &b)
    if c[0, 0] != 6.0f32 { return 1 }
    if c[36, 44] != 6.0f32 { return 2 }
    val w: Tensor<f64, [3, 2]> = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
    val x: Tensor<f64, [2, 2]> = [[1.0, 0.0], [1.0, 1.0]]
    val p = project(w, &x)
    if p[0, 0] != 3.0 { return 3 }
    if p[2, 1] != 6.0 { return 4 }
    // A pool restores the device arena as well as the host one; a leak would spill.
    pool {
        mut i = 0
        while i < 200 {
            val t = blend(&a, &b)
            i += 1
        }
    }
    return 0
}
"#;

const GRAD_THROUGH_GPU: &str = r#"
@gpu
func double(x: &Tensor<f32, [2]>) -> Tensor<f32, [2]> {
    x + x
}

@grad
func loss(t: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {
    val d = double(t)
    return Tensor::scalar(d.sum())
}

func main() -> i32 {
    return 0
}
"#;

#[test]
fn a_misplaced_gpu_is_refused_by_the_checker() {
    for (source, problem) in [
        (
            "@gpu(fallback: true)\nfunc id(x: f32) -> f32 {\n    x\n}\nfunc main() -> i32 { return 0 }\n",
            "`fallback:` is not supported yet",
        ),
        (
            "struct S { x: f32 }\nimpl S {\n    @gpu\n    func get(&self) -> f32 {\n        self.x\n    }\n}\nfunc main() -> i32 { return 0 }\n",
            "on a method is not supported yet",
        ),
    ] {
        let error = CompileTest::new()
            .check("misplaced.nr", source)
            .expect_err("`check` should refuse the attribute");
        assert!(error.contains(problem), "expected `{problem}` in:\n{error}");
    }
}

#[test]
fn a_grad_body_cannot_differentiate_through_a_gpu_call() {
    let error = CompileTest::new()
        .check("grad_through_gpu.nr", GRAD_THROUGH_GPU)
        .expect_err("the derivative would run the kernel's body on the host");
    assert!(error.contains("a call to a `@gpu` function"), "{error}");
}

#[cfg(not(feature = "mlir"))]
#[test]
fn without_mlir_a_gpu_function_is_a_compile_error() {
    let test = CompileTest::new();
    let source = test.write_source(
        "no_mlir.nr",
        "@gpu\nfunc add(a: &Tensor<f32, [2]>, b: &Tensor<f32, [2]>) -> Tensor<f32, [2]> {\n    a + b\n}\nfunc main() -> i32 { return 0 }\n",
    );
    let error = test.compile(&source).expect_err("no host fallback exists");
    assert!(
        error.contains("`@gpu` function 'add' needs a GPU")
            && error.contains("built without the MLIR backend")
            && error.contains("no_mlir.nr:2:1"),
        "{error}"
    );
}

#[cfg(feature = "mlir")]
#[test]
fn a_body_that_cannot_become_a_kernel_is_a_compile_error() {
    let test = CompileTest::new();
    let text = "@gpu\nfunc add(a: Tensor<i32, [4]>, b: Tensor<i32, [4]>) -> Tensor<i32, [4]> {\n    a + b\n}\n@gpu\nfunc total(a: &Tensor<f32, [4]>) -> f32 {\n    a.sum()\n}\nfunc main() -> i32 { return 0 }\n";
    let source = test.write_source("not_a_kernel.nr", text);
    let compiled = test
        .compile(&source)
        .expect_err("neither body can run on a GPU");
    // `check` refuses what `compile` refuses, rather than passing the program.
    let checked = test
        .check("not_a_kernel.nr", text)
        .expect_err("`check` should refuse the bodies `compile` refuses");
    for error in [compiled, checked] {
        for (name, line) in [("add", 2), ("total", 6)] {
            assert!(
                error.contains(&format!(
                    "`@gpu` function '{name}' cannot become a GPU kernel"
                )) && error.contains(&format!("not_a_kernel.nr:{line}:1")),
                "expected `{name}` refused at line {line}:\n{error}"
            );
        }
    }
}

#[cfg(feature = "mlir")]
#[test]
fn gpu_bodies_launch_kernels_and_host_bodies_stay_on_the_host() {
    let test = CompileTest::new();
    let source = test.write_source(
        "mixed.nr",
        "@gpu\nfunc on_gpu(a: &Tensor<f32, [8]>) -> Tensor<f32, [8]> {\n    a * a\n}\nfunc on_host(a: &Tensor<f32, [8]>) -> Tensor<f32, [8]> {\n    a + a\n}\nfunc main() -> i32 { return 0 }\n",
    );
    let ir_path = source.with_extension("ll");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source)
        .output()
        .expect("Failed to execute neurc");
    assert!(output.status.success(), "{output:?}");
    let ir = std::fs::read_to_string(&ir_path).expect("the IR is written");

    let body = |name: &str| {
        let start = ir
            .find(&format!("define ptr @{name}("))
            .unwrap_or_else(|| panic!("no `{name}`:\n{ir}"));
        let rest = &ir[start..];
        rest[..rest.find("\n}").unwrap_or(rest.len())].to_string()
    };
    assert!(body("on_gpu").contains("mgpuMemcpy"), "{}", body("on_gpu"));
    assert!(!body("on_host").contains("mgpu"), "{}", body("on_host"));
    assert!(ir.contains("mgpuLaunchKernel"), "{ir}");
}

#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_gpu_program_runs_on_the_gpu_or_aborts_at_startup() {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("kernels.nr", KERNELS))
        .expect("the kernels should compile");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("none is usable") {
        assert_aborted_at_startup(&output);
        return;
    }
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "main ran\n");
}

#[cfg(all(feature = "mlir", unix))]
#[test]
fn without_a_visible_gpu_the_program_aborts_before_main() {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("hidden.nr", KERNELS))
        .expect("the kernels should compile");
    let output = std::process::Command::new(&exe)
        .env("CUDA_VISIBLE_DEVICES", "")
        .output()
        .expect("the program should start");
    assert_aborted_at_startup(&output);
}

/// The module loads in a global constructor, so the check runs before `main` prints.
#[cfg(all(feature = "mlir", unix))]
fn assert_aborted_at_startup(output: &std::process::Output) {
    use std::os::unix::process::ExitStatusExt;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("panic: `@gpu` needs an NVIDIA GPU, and none is usable: "),
        "{stderr}"
    );
    assert_eq!(
        output.status.signal(),
        Some(6),
        "expected SIGABRT: {stderr}"
    );
    assert!(output.stdout.is_empty(), "main must not run");
}
