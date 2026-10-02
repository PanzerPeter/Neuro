// `@kernel(threads: [...])`: a function whose body runs once per thread of a launch grid.
//
// As with `@gpu`, only a Linux neurc compiles one and only a machine with an NVIDIA GPU
// runs the result, so each run test accepts either outcome a machine can
// produce and asserts it fully: the right answer on a GPU, or the startup abort without one.

use crate::compile_harness::CompileTest;

/// An element-wise kernel over a grid that does not divide by its blocks, checked against
/// the same arithmetic on the host, then again with the output already on the device.
#[cfg(target_os = "linux")]
const ADD_RELU: &str = r#"
@kernel(threads: [16, 16])
func add_relu(a: Tensor<f32, [37, 45]>, b: Tensor<f32, [37, 45]>, out: KernelOut<Tensor<f32, [37, 45]>>) {
    val row = thread_id.x
    val col = thread_id.y
    if row < 37 && col < 45 {
        val sum = a[row, col] + b[row, col]
        // SAFETY: thread (row, col) is the only one to write out[row, col].
        unsafe { out[row, col] = if sum > 0.0 { sum } else { 0.0 } }
    }
}

@kernel(threads: [4])
func row_sums(m: Tensor<i64, [9, 7]>, out: KernelOut<Tensor<i64, [9]>>, blocks: KernelOut<Tensor<u32, [9]>>) {
    val row = thread_id.x
    if row >= 9 {
        return
    }
    mut total: i64 = 0
    for k in 0..7 {
        if k == 3 {
            continue
        }
        total += m[row, k]
    }
    unsafe {
        out[row] = total
        blocks[row] = block_id.x
    }
}

func main() -> i32 {
    println("main ran")
    mut a: Tensor<f32, [37, 45]> = Tensor::zeros()
    mut b: Tensor<f32, [37, 45]> = Tensor::zeros()
    for i in 0..37 {
        for j in 0..45 {
            a[i, j] = (i * 45 + j) as f32
            b[i, j] = -700.0
        }
    }
    mut r: Tensor<f32, [37, 45]> = Tensor::zeros()
    add_relu(a, b, &mut r)
    for i in 0..37 {
        for j in 0..45 {
            val s = a[i, j] + b[i, j]
            val want = if s > 0.0 { s } else { 0.0 }
            if r[i, j] != want { return 1 }
        }
    }

    mut m: Tensor<i64, [9, 7]> = Tensor::zeros()
    for i in 0..9 {
        for j in 0..7 {
            m[i, j] = (i * 10 + j) as i64
        }
    }
    mut sums: Tensor<i64, [9]> = Tensor::zeros()
    mut blocks: Tensor<u32, [9]> = Tensor::zeros()
    row_sums(m, &mut sums, &mut blocks)
    if sums[8] != 80 * 6 + 1 + 2 + 4 + 5 + 6 { return 2 }
    if blocks[8] != 2u32 || blocks[3] != 0u32 { return 3 }

    val on_device = m.to(Device::GPU(0))
    mut sums_on_device = Tensor::<i64, [9]>::zeros().to(Device::GPU(0))
    mut blocks_on_device = Tensor::<u32, [9]>::zeros().to(Device::GPU(0))
    row_sums(on_device, &mut sums_on_device, &mut blocks_on_device)
    val back = sums_on_device.to(Device::CPU)
    if back[8] != sums[8] { return 4 }
    return 0
}
"#;

#[test]
fn a_malformed_kernel_is_refused_by_the_checker() {
    for (source, problem) in [
        (
            "@kernel(threads: [2000])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) {}\nfunc main() -> i32 { return 0 }\n",
            "no GPU runs more than 1024",
        ),
        (
            "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) -> f32 {\n    1.0\n}\nfunc main() -> i32 { return 0 }\n",
            "returns nothing",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [4]>, out: &mut Tensor<f32, [4]>) {}\nfunc main() -> i32 { return 0 }\n",
            "parameter 'out' is a reference; a kernel reads a `Tensor<T, S>` and writes a `KernelOut<Tensor<T, S>>`",
        ),
        (
            "@kernel(threads: [4, 4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) {}\nfunc main() -> i32 { return 0 }\n",
            "one per axis",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [4]>) {}\nfunc main() -> i32 { return 0 }\n",
            "needs a `KernelOut<Tensor<T, S>>` parameter",
        ),
        (
            "func host(out: KernelOut<Tensor<f32, [4]>>) {}\nfunc main() -> i32 { return 0 }\n",
            "`KernelOut<T>` is only a kernel parameter's type",
        ),
        (
            "func sink(t: &mut Tensor<f32, [4]>) {}\n@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) {\n    sink(out)\n}\nfunc main() -> i32 { return 0 }\n",
            "output 'out' is written one element at a time",
        ),
        (
            "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<i32, [4]>>) {\n    out[thread_id.x] += 1\n}\nfunc main() -> i32 { return 0 }\n",
            "output 'out' is indexed only inside `unsafe { }`",
        ),
        (
            "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<i32, [4]>>) {\n    val x = out[(thread_id.x + 1) % 4]\n}\nfunc main() -> i32 { return 0 }\n",
            "output 'out' is indexed only inside `unsafe { }`",
        ),
        (
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [4]>, out: KernelOut<Tensor<f32, [4]>>) {}\nfunc main() -> i32 {\n    val a: Tensor<f32, [4]> = Tensor::ones()\n    mut r: Tensor<f32, [4]> = Tensor::zeros()\n    k(&a, &mut r)\n    return 0\n}\n",
            "pass the tensor, not `&`",
        ),
    ] {
        let error = CompileTest::new()
            .check("malformed.nr", source)
            .expect_err("`check` should refuse the kernel");
        assert!(error.contains(problem), "expected `{problem}` in:\n{error}");
    }
}

/// A kernel borrows its `Tensor` inputs, so the caller still owns them after the call,
/// and writes the tensor its `KernelOut` is built from.
#[test]
fn a_kernel_call_borrows_its_inputs_and_lends_its_output() {
    let source = "@kernel(threads: [4])\nfunc scale(a: Tensor<f32, [4]>, s: f32, out: KernelOut<Tensor<f32, [4]>>) {\n    val i = thread_id.x\n    if i < 4 {\n        unsafe { out[i] = a[i] * s }\n    }\n}\nfunc main() -> i32 {\n    val a: Tensor<f32, [4]> = Tensor::ones()\n    mut r: Tensor<f32, [4]> = Tensor::zeros()\n    scale(a, 2.0, &mut r)\n    scale(a, 3.0, &mut r)\n    return (a[0] + r[0]) as i32\n}\n";
    CompileTest::new()
        .check("borrows.nr", source)
        .expect("a kernel input stays usable after the call");
    let error = CompileTest::new()
        .check(
            "aliased.nr",
            &source.replace("scale(a, 3.0, &mut r)", "scale(r, 3.0, &mut r)"),
        )
        .expect_err("one call may not read and write the same tensor");
    assert!(error.contains("aliased.nr:12:"), "{error}");
}

#[test]
fn grid_positions_exist_only_inside_a_kernel_body() {
    let error = CompileTest::new()
        .check(
            "outside.nr",
            "func main() -> i32 {\n    return thread_id.x as i32\n}\n",
        )
        .expect_err("`thread_id` is not a name outside a kernel");
    assert!(
        error.contains("undefined variable 'thread_id'") && error.contains("outside.nr:2:12"),
        "{error}"
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn off_linux_a_kernel_is_a_compile_error() {
    let test = CompileTest::new();
    let source = test.write_source(
        "off_linux.nr",
        "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [4]>>) {\n    unsafe { out[thread_id.x] = 1.0 }\n}\nfunc main() -> i32 { return 0 }\n",
    );
    let error = test
        .compile(&source)
        .expect_err("a kernel has no host body");
    assert!(
        error.contains("`@kernel` function 'k' needs a GPU")
            && error.contains("only Linux builds reach")
            && error.contains("off_linux.nr:2:1"),
        "{error}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_body_the_gpu_path_cannot_lower_is_refused_at_the_construct() {
    let test = CompileTest::new();
    let text = "func twice(x: f32) -> f32 {\n    x * 2.0\n}\n@kernel(threads: [4])\nfunc k(a: Tensor<f32, [4]>, out: KernelOut<Tensor<f32, [4]>>) {\n    val i = thread_id.x\n    unsafe { out[i] = twice(a[i]) }\n}\nfunc main() -> i32 { return 0 }\n";
    let source = test.write_source("call_in_kernel.nr", text);
    let compiled = test
        .compile(&source)
        .expect_err("a kernel body cannot call a host function");
    let checked = test
        .check("call_in_kernel.nr", text)
        .expect_err("`check` should refuse what `compile` refuses");
    for error in [compiled, checked] {
        assert!(
            error.contains("`@kernel` function 'k' cannot lower a function call to the GPU")
                && error.contains("call_in_kernel.nr:7:23"),
            "{error}"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_kernel_runs_on_the_gpu_or_the_program_aborts_at_startup() {
    let test = CompileTest::new();
    for optimization in ["-O0", "-O2"] {
        let source = test.write_source("add_relu.nr", ADD_RELU);
        let exe = source.with_extension("");
        let compiled = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
            .args(["compile", optimization, "-o"])
            .arg(&exe)
            .arg(&source)
            .output()
            .expect("Failed to execute neurc");
        assert!(compiled.status.success(), "{compiled:?}");
        let output = std::process::Command::new(&exe)
            .output()
            .expect("the program should start");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("none is usable") {
            assert!(output.stdout.is_empty(), "main must not run: {stderr}");
            return;
        }
        assert_eq!(output.status.code(), Some(0), "{optimization}: {stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "main ran\n");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn an_index_past_an_extent_stops_the_kernel_and_the_program() {
    use std::os::unix::process::ExitStatusExt;
    let test = CompileTest::new();
    // Eight threads a block over ten elements: two blocks, and the last six threads have
    // no element to read.
    let exe = test
        .compile(&test.write_source(
            "overhang.nr",
            "@kernel(threads: [8])\nfunc double(a: Tensor<i32, [10]>, out: KernelOut<Tensor<i32, [10]>>) {\n    val i = thread_id.x\n    unsafe { out[i] = a[i] * 2 }\n}\nfunc main() -> i32 {\n    val a: Tensor<i32, [10]> = Tensor::ones()\n    mut out: Tensor<i32, [10]> = Tensor::zeros()\n    double(a, &mut out)\n    return out[0]\n}\n",
        ))
        .expect("the kernel should compile");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.signal(),
        Some(6),
        "expected SIGABRT: {stderr}"
    );
    if stderr.contains("none is usable") {
        return;
    }
    assert!(
        stderr.contains("panic: GPU error: cuStreamSynchronize failed with CUDA_ERROR_ASSERT"),
        "{stderr}"
    );
}
