// `@gpu(fallback: true)`: a GPU kernel where the program finds a usable GPU at startup, and
// the same body on the host where it does not.
//
// Unlike bare `@gpu`, a fallback program runs on every build and every machine, so its
// answers are pinned everywhere: on a GPU, on CI's GPU-less MLIR job, with every device
// hidden by `CUDA_VISIBLE_DEVICES`, and on a neurc without MLIR, which builds only the host
// body and says so.

use std::process::{Command, Output};

use crate::compile_harness::CompileTest;

/// Two fallback functions, one taking an owned operand, each checked against the same
/// arithmetic done inline on the host.
const FALLBACK: &str = r#"
@gpu(fallback: true)
func blend(a: &Tensor<f32, [37, 45]>, b: &Tensor<f32, [37, 45]>) -> Tensor<f32, [37, 45]> {
    val s = a + b
    s * b
}

@gpu(fallback: true)
func project(w: Tensor<f64, [3, 2]>, x: &Tensor<f64, [2, 2]>) -> Tensor<f64, [3, 2]> {
    w @ x
}

func main() -> i32 {
    val a = Tensor::<f32, [37, 45]>::ones()
    val b = &a * 2.0f32
    val c = blend(&a, &b)
    val inline = (&a + &b) * &b
    if c[0, 0] != inline[0, 0] || c[36, 44] != 6.0f32 { return 1 }
    val w: Tensor<f64, [3, 2]> = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
    val x: Tensor<f64, [2, 2]> = [[1.0, 0.0], [1.0, 1.0]]
    val p = project(w, &x)
    if p[0, 0] != 3.0 || p[2, 1] != 6.0 { return 2 }
    pool {
        mut i = 0
        while i < 200 {
            val t = blend(&a, &b)
            i += 1
        }
    }
    println("{c[0, 0]} {p[2, 1]}")
    return 0
}
"#;

/// `FALLBACK` with a bare `@gpu` beside the fallback ones.
fn with_a_bare_gpu_function() -> String {
    FALLBACK.replacen(
        "@gpu(fallback: true)\nfunc project",
        "@gpu\nfunc project",
        1,
    )
}

fn run(exe: &std::path::Path, hide_devices: bool) -> Output {
    let mut command = Command::new(exe);
    if hide_devices {
        command.env("CUDA_VISIBLE_DEVICES", "");
    }
    command.output().expect("the program should start")
}

fn assert_ran(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        "6.0 6.0\n"
    );
}

#[test]
fn a_fallback_program_runs_with_or_without_a_gpu() {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("fallback.nr", FALLBACK))
        .expect("a fallback program compiles on every build");
    assert_ran(&run(&exe, false));
    assert_ran(&run(&exe, true));
}

#[test]
fn fallback_takes_only_a_bool_literal() {
    let error = CompileTest::new()
        .check(
            "fallback_value.nr",
            &FALLBACK.replacen("fallback: true", "fallback: 1", 1),
        )
        .expect_err("the value is decided at compile time");
    assert!(
        error.contains("`fallback:` takes the literal `true` or `false`"),
        "{error}"
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn off_linux_the_host_body_is_built_with_a_warning() {
    let test = CompileTest::new();
    let source = test.write_source("host_only.nr", FALLBACK);
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "-o"])
        .arg(source.with_extension("bin"))
        .arg(&source)
        .output()
        .expect("Failed to execute neurc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    for (name, line) in [("blend", 3), ("project", 9)] {
        assert!(
            stderr.contains(&format!(
                "warning: `@gpu(fallback: true)` function '{name}' always runs on the host"
            )) && stderr.contains(&format!("host_only.nr:{line}:1")),
            "expected `{name}` warned about at line {line}:\n{stderr}"
        );
    }

    // A bare `@gpu` beside them is still refused: it has no host body to build.
    let mixed = test.write_source("mixed.nr", &with_a_bare_gpu_function());
    let error = test.compile(&mixed).expect_err("bare `@gpu` needs a GPU");
    assert!(
        error.contains("`@gpu` function 'project' needs a GPU"),
        "{error}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_fallback_body_must_still_become_a_kernel() {
    let test = CompileTest::new();
    let text = "@gpu(fallback: true)\nfunc total(a: &Tensor<f32, [4]>) -> f32 {\n    a.sum()\n}\nfunc main() -> i32 { return 0 }\n";
    let source = test.write_source("not_a_kernel.nr", text);
    let compiled = test
        .compile(&source)
        .expect_err("a fallback is a host copy of a kernel, not a substitute for one");
    let checked = test
        .check("not_a_kernel.nr", text)
        .expect_err("`check` should refuse the body `compile` refuses");
    for error in [compiled, checked] {
        assert!(
            error.contains("`@gpu` function 'total' cannot become a GPU kernel")
                && error.contains("not_a_kernel.nr:2:1"),
            "{error}"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_fallback_function_gets_a_device_and_a_host_body() {
    let test = CompileTest::new();
    let source = test.write_source("both.nr", FALLBACK);
    let ir_path = source.with_extension("ll");
    let output = Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "--emit", "llvm-ir", "-o"])
        .arg(&ir_path)
        .arg(&source)
        .output()
        .expect("Failed to execute neurc");
    assert!(output.status.success(), "{output:?}");
    let ir = std::fs::read_to_string(&ir_path).expect("the IR is written");
    for symbol in ["@blend.gpu(", "@blend.host(", "@__neuro_gpu_usable("] {
        assert!(ir.contains(symbol), "expected `{symbol}`:\n{ir}");
    }
    assert!(ir.contains("mgpuLaunchKernel"), "{ir}");
}

#[cfg(target_os = "linux")]
#[test]
fn one_bare_gpu_function_still_aborts_before_main_without_a_gpu() {
    use std::os::unix::process::ExitStatusExt;
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("mixed.nr", &with_a_bare_gpu_function()))
        .expect("the program should compile");
    let output = run(&exe, true);
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
