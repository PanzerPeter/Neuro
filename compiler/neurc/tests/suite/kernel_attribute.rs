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
        (
            // 70000 blocks along `y`: the launch once failed at run time instead.
            "@kernel(threads: [1, 1])\nfunc k(out: KernelOut<Tensor<u8, [2, 70000]>>) {}\nfunc main() -> i32 { return 0 }\n",
            "runs 70000 blocks along `y` (an extent of 70000 in blocks of 1); no GPU launches more than 65535 there",
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

/// Integer arithmetic in a kernel follows the build's tier: an overflow stops the kernel and
/// the program on the debug tier, as an index past an extent does, and wraps from `-O1` up.
#[cfg(target_os = "linux")]
#[test]
fn an_integer_overflow_stops_the_kernel_on_the_debug_tier_only() {
    use std::os::unix::process::ExitStatusExt;
    let test = CompileTest::new();
    let source = test.write_source(
        "kernel_overflow.nr",
        "@kernel(threads: [4])\nfunc bump(a: Tensor<i32, [4]>, out: KernelOut<Tensor<i32, [4]>>) {\n    val i = thread_id.x\n    if i < 4 {\n        unsafe { out[i] = a[i] + 1 }\n    }\n}\nfunc main() -> i32 {\n    val a: Tensor<i32, [4]> = [1, 2, 3, 2147483647]\n    mut out: Tensor<i32, [4]> = Tensor::zeros()\n    bump(a, &mut out)\n    println(\"{out[3]}\")\n    return 0\n}\n",
    );
    for (level, stopped) in [("0", true), ("2", false)] {
        let exe = source.with_extension(format!("o{level}"));
        let built = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
            .args(["compile", "-O", level, "-o"])
            .arg(&exe)
            .arg(&source)
            .output()
            .expect("neurc runs");
        assert!(built.status.success(), "{built:?}");
        let output = std::process::Command::new(&exe)
            .output()
            .expect("the program should start");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("none is usable") {
            return;
        }
        if stopped {
            assert_eq!(output.status.signal(), Some(6), "-O0: {stderr}");
            assert!(stderr.contains("CUDA_ERROR_ASSERT"), "-O0: {stderr}");
        } else {
            assert_eq!(String::from_utf8_lossy(&output.stdout), "-2147483648\n");
        }
    }
}

/// A program whose only GPU code is `@kernel` names `@kernel` when no GPU is usable.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_only_program_names_kernel_when_no_gpu_is_usable() {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("kernel_no_gpu.nr", ADD_RELU))
        .expect("the kernel program compiles");
    let output = std::process::Command::new(&exe)
        .env("CUDA_VISIBLE_DEVICES", "")
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("panic: `@kernel` needs an NVIDIA GPU, and none is usable: "),
        "{stderr}"
    );
    assert!(output.stdout.is_empty(), "main must not run");
}

/// A kernel input may be a temporary: `k(a + b, ...)` binds it for the call, evaluating
/// the arguments left to right, and gives what the bound form gives.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_input_may_be_a_temporary() {
    let source = r#"
@kernel(threads: [8])
func axpy(a: Tensor<f32, [16]>, b: Tensor<f32, [16]>, k: u32, out: KernelOut<Tensor<f32, [16]>>) {
    val i = thread_id.x
    if i < 16 {
        unsafe { out[i] = a[i] * (k as f32) + b[i] }
    }
}
func noisy(tag: i32, t: Tensor<f32, [16]>) -> Tensor<f32, [16]> {
    println("eval {tag}")
    t
}
func main() -> i32 {
    mut x: Tensor<f32, [16]> = Tensor::zeros()
    for i in 0..16 { x[i] = i as f32 }
    val y = Tensor::<f32, [16]>::ones()
    mut r1: Tensor<f32, [16]> = Tensor::zeros()
    mut r2: Tensor<f32, [16]> = Tensor::zeros()
    axpy(noisy(1, &x + &y), noisy(2, &y * 2.0f32), 3, &mut r1)
    val t1 = &x + &y
    val t2 = &y * 2.0f32
    axpy(t1, t2, 3, &mut r2)
    if r1[15] == r2[15] { r1[15] as i32 } else { 0 }
}
"#;
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("kernel_temporary.nr", source))
        .expect("a temporary kernel input compiles");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("none is usable") {
        return;
    }
    assert_eq!(String::from_utf8_lossy(&output.stdout), "eval 1\neval 2\n");
    assert_eq!(output.status.code(), Some(16 * 3 + 2));
}

/// A kernel body may shift, and read a local or module `const`, each as the host computes
/// it; a device `.map` closure shifts on the GPU too. A shift by the operand's width or
/// more panics on the debug tier with the host's message.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_body_shifts_and_reads_constants() {
    let source = r#"
const SCALE: i32 = 3
@kernel(threads: [8])
func f(a: Tensor<i32, [16]>, k: i32, out: KernelOut<Tensor<i32, [16]>>) {
    const BIAS: i32 = 2
    val i = thread_id.x
    if i < 16 {
        unsafe { out[i] = (a[i] << k) * SCALE + BIAS }
    }
}
func main() -> i32 {
    mut a: Tensor<i32, [16]> = Tensor::zeros()
    for i in 0..16 { a[i] = i as i32 }
    mut r: Tensor<i32, [16]> = Tensor::zeros()
    f(a, 1, &mut r)
    val shifted = a.clone().to(Device::GPU(0)).map(|x: i32| -> i32 { x << 2 }).to(Device::CPU)
    r[15] + shifted[15]
}
"#;
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("kernel_shift.nr", source))
        .expect("the kernel compiles");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    if String::from_utf8_lossy(&output.stderr).contains("none is usable") {
        return;
    }
    assert_eq!(output.status.code(), Some((15 << 1) * 3 + 2 + (15 << 2)));

    let past_width = source.replace("x << 2", "x << 40");
    let exe = test
        .compile(&test.write_source("kernel_shift_past.nr", &past_width))
        .expect("the kernel compiles");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("panic: shift overflow at "), "{stderr}");
}

/// `body`, which computes `acc: i64` from `n: i32`, run once per element in a kernel and once
/// per element on the host, the program failing at the first element where the two differ.
#[cfg(target_os = "linux")]
fn kernel_against_host(body: &str) -> String {
    let indent = |by: &str| {
        body.lines()
            .map(|line| format!("{by}{line}\n"))
            .collect::<String>()
    };
    format!(
        "@kernel(threads: [8])\nfunc k(a: Tensor<i32, [24]>, out: KernelOut<Tensor<i64, [24]>>) {{\n    val i = thread_id.x\n    if i < 24 {{\n        val n = a[i]\n{}        unsafe {{ out[i] = acc }}\n    }}\n}}\nfunc host(n: i32) -> i64 {{\n{}    acc\n}}\nfunc main() -> i32 {{\n    mut a: Tensor<i32, [24]> = Tensor::zeros()\n    for j in 0..24 {{ a[j] = j as i32 - 4 }}\n    mut r: Tensor<i64, [24]> = Tensor::zeros()\n    k(a, &mut r)\n    for j in 0..24 {{\n        if r[j] != host(j as i32 - 4) {{ return 1 }}\n    }}\n    println(\"ok\")\n    0\n}}\n",
        indent("        "),
        indent("    "),
    )
}

/// Compiles `source` at `-O level` and runs it, or `None` where no GPU is usable.
#[cfg(target_os = "linux")]
fn run_at(
    test: &CompileTest,
    name: &str,
    source: &str,
    level: &str,
) -> Option<std::process::Output> {
    let path = test.write_source(name, source);
    let exe = path.with_extension(format!("o{level}"));
    let built = std::process::Command::new(env!("CARGO_BIN_EXE_neurc"))
        .args(["compile", "-O", level, "-o"])
        .arg(&exe)
        .arg(&path)
        .output()
        .expect("neurc runs");
    assert!(built.status.success(), "{built:?}");
    let output = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    (!String::from_utf8_lossy(&output.stderr).contains("none is usable")).then_some(output)
}

/// Each body gives the host's answer bit for bit in a kernel, on both tiers.
#[cfg(target_os = "linux")]
fn assert_kernel_matches_host(name: &str, body: &str) {
    let test = CompileTest::new();
    let source = kernel_against_host(body);
    for level in ["0", "2"] {
        let Some(output) = run_at(&test, name, &source, level) else {
            return;
        };
        assert_eq!(
            (
                output.status.code(),
                String::from_utf8_lossy(&output.stdout).as_ref()
            ),
            (Some(0), "ok\n"),
            "-O{level}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A kernel body walks a range reversed, stepped, both, and inclusive as the host walks it,
/// near each type's ends included, and reads `-128i8` as the constant it is.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_body_walks_reversed_and_stepped_ranges() {
    assert_kernel_matches_host(
        "kernel_ranges.nr",
        "mut acc: i64 = 0
for x in (0..n % 9).rev() { acc = acc * 3 + x as i64 }
for x in (n..20).step(3) { acc = acc * 5 + x as i64 }
for x in (-2..=n).rev().step(2) { acc = acc * 7 + x as i64 }
for x in (250u8..=255u8).step(2) { acc += x as i64 }
for x in (0u8..=255u8).rev().step(100) { acc += x as i64 }
for x in (0u32..((n + 4) as u32 % 5)).rev() { acc += x as i64 }
for x in (n..=n + 2).rev() {
    if x == n + 1 { continue }
    acc = acc * 2 + x as i64
}
for x in (120i8..=127i8).step(3) { acc += x as i64 }
for x in (-128i8..=-120i8).rev().step(4) { acc += x as i64 }
val s = (n + 5) as u16
for x in (0u16..=65535u16).rev().step(s * 1000) { acc += x as i64 }
",
    );
}

/// A stride that is not positive stops a kernel on every tier, and fails a device `.map`'s
/// closure with the host's panic at the stride.
#[cfg(target_os = "linux")]
#[test]
fn a_range_step_that_is_not_positive_fails_on_the_gpu() {
    use std::os::unix::process::ExitStatusExt;
    let test = CompileTest::new();
    let kernel = "@kernel(threads: [4])\nfunc k(a: Tensor<i32, [4]>, out: KernelOut<Tensor<i32, [4]>>) {\n    val i = thread_id.x\n    if i < 4 {\n        mut s = 0\n        for x in (0..10).step(a[i]) { s += x }\n        unsafe { out[i] = s }\n    }\n}\nfunc main() -> i32 {\n    val a: Tensor<i32, [4]> = [1, 2, 0, 3]\n    mut out: Tensor<i32, [4]> = Tensor::zeros()\n    k(a, &mut out)\n    out[1]\n}\n";
    let traversal = "func main() -> i32 {\n    val a: Tensor<i32, [4]> = [1, 2, -1, 3]\n    val g = a.to(Device::GPU(0))\n    val r = g.map(|x: i32| -> i32 {\n        mut s = 0\n        for k in (0..10).rev().step(x) { s += k }\n        s\n    }).to(Device::CPU)\n    r[0]\n}\n";
    for level in ["0", "2"] {
        let Some(output) = run_at(&test, "kernel_step.nr", kernel, level) else {
            return;
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.signal(), Some(6), "-O{level}: {stderr}");
        assert!(stderr.contains("CUDA_ERROR_ASSERT"), "-O{level}: {stderr}");

        let Some(output) = run_at(&test, "map_step.nr", traversal, level) else {
            return;
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with("panic: range step must be positive at ")
                && stderr.contains("map_step.nr:6:37"),
            "-O{level}: {stderr}"
        );
    }
    let fixed = traversal.replace("-1, 3", "4, 3");
    let Some(output) = run_at(&test, "map_step_ok.nr", &fixed, "0") else {
        return;
    };
    assert_eq!(output.status.code(), Some(45), "{output:?}");
}

/// A `loop` in a kernel body yields its `break` value, a labeled one included, of any
/// scalar type.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_body_loop_yields_its_break_value() {
    assert_kernel_matches_host(
        "kernel_loop_value.nr",
        "mut j: i32 = 0
val first = loop {
    j += 1
    if j * j > n + 10 {
        break j
    }
}
mut total: i64 = 0
val found = outer: loop {
    for x in 0..10 {
        total += x as i64
        if x == n % 7 {
            break outer total * 2
        }
        if x > 7 { break }
    }
    if total > 100 { break total }
}
val flag = loop {
    if n > 3 { break true }
    break false
}
val c = loop { break if flag { 'y' } else { 'n' } }
mut acc: i64 = first as i64 * 1000 + found
if flag { acc += 7 }
if c == 'y' { acc += 1 }
loop {
    acc += 1
    if acc % 3 == 0 { break }
}
",
    );
}

/// A kernel body matches an integer, `bool` or `char` with literal, range, or-, binding and
/// `_` patterns and guards, as a value and as a statement.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_body_matches_on_a_scalar() {
    assert_kernel_matches_host(
        "kernel_match.nr",
        "mut acc: i64 = match n {
    -4 | -3 => 100,
    0 => 7,
    1..=3 => 20 + n as i64,
    4..8 => {
        val d = n * 2
        d as i64
    }
    m if m % 2 == 0 => m as i64 * 3,
    _ => -1,
}
val u = (n + 4) as u8 * 11
acc = acc * 10 + match u {
    0..=50 => 1,
    200..=255 => 2,
    _ => 3,
}
val big = (n as u32).wrapping_mul(1000000000u32)
acc = acc * 10 + match big {
    0 => 0,
    3000000000u32..=4294967295u32 => 9,
    _ => 4,
}
val c = ((n + 100) as u8) as char
acc = acc * 10 + match c {
    'a'..='f' => 5,
    'Z' | '`' => 6,
    _ => 8,
}
acc = acc * 10 + match n > 5 {
    true => 1,
    false => 2,
}
match n % 3 {
    0 => { acc += 1000 }
    1 => {}
    _ => { acc -= 1000 }
}
acc = acc * 10 + match (n - 10) as i8 {
    -128..=-10 => 1,
    -9..0 => 2,
    _ => 3,
}
if c as u32 > 100u32 { acc += 1 }
",
    );
}

/// The integer methods the host computes as arithmetic, and `.is_nan()`, compute the same
/// in a kernel body; a `.shr` past the width panics on the debug tier with the host's
/// message in a device `.map`, and masks its amount from `-O1` up.
#[cfg(target_os = "linux")]
#[test]
fn a_kernel_body_calls_the_scalar_arithmetic_methods() {
    assert_kernel_matches_host(
        "kernel_methods.nr",
        "val w = n * 100000000
mut acc: i64 = w.wrapping_mul(7) as i64
acc = acc.wrapping_add(w.wrapping_add(2147483647) as i64)
acc = acc.wrapping_sub(w.wrapping_sub(-2147483647) as i64)
acc += w.saturating_add(1500000000) as i64
acc += w.saturating_sub(1500000000) as i64
acc += w.saturating_mul(n) as i64
acc += w.saturating_mul(-n) as i64
val u = ((n + 4) % 14) as u32 * 300000000u32
acc += u.saturating_add(3000000000u32) as i64
acc += u.saturating_sub(3000000000u32) as i64
acc += u.saturating_mul(5u32) as i64
acc += (u as u8).saturating_mul(3u8) as i64
acc += (n as i8).saturating_mul(100i8) as i64
acc += (n as i64 * 1000000000000).saturating_mul(n as i64 * 100000000) % 1000003
acc += w.shr(3) as i64
acc += u.shr(29u32) as i64
acc += (n as i16 - 100i16).shr(2i16) as i64
acc += (u as u64).wrapping_mul(u as u64).shr(40u64) as i64
val f = n as f32 / 0.0
if f.is_nan() { acc += 1000 }
if (n as f64).is_nan() { acc += 1 }
",
    );
    let test = CompileTest::new();
    let source = "func main() -> i32 {\n    val a: Tensor<i32, [4]> = [256, 512, 768, 1024]\n    val r = a.to(Device::GPU(0)).map(|x: i32| -> i32 { x.shr(40) }).to(Device::CPU)\n    r[3]\n}\n";
    for (level, wanted) in [("0", None), ("2", Some(1024 >> (40 % 32)))] {
        let Some(output) = run_at(&test, "map_shr.nr", source, level) else {
            return;
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        match wanted {
            None => assert!(
                stderr.starts_with("panic: shift overflow at ")
                    && stderr.contains("map_shr.nr:3:56"),
                "{stderr}"
            ),
            Some(code) => assert_eq!(output.status.code(), Some(code), "{stderr}"),
        }
    }
}

/// `thread_id` and `block_id` bound whole by a `val` are read through their fields, in the
/// body and in a `partition` closure, and an inner local of the same name hides the alias.
#[cfg(target_os = "linux")]
#[test]
fn a_grid_position_bound_whole_is_read_by_its_fields() {
    let source = r#"
@kernel(threads: [4, 2])
func k(out: KernelOut<Tensor<u32, [6, 5]>>, blocks: KernelOut<Tensor<u32, [6, 5]>>) {
    val t = thread_id
    val b = block_id
    val u = t
    if t.x < 6 && u.y < 5 {
        val v = {
            val t = 100u32
            t + 1
        }
        unsafe {
            out[t.x, t.y] = t.x * 10 + u.y + v + t.z
            blocks[u.x, t.y] = b.x * 100 + b.y * 10 + b.z
        }
    }
}

@kernel(threads: [4])
func p(out: KernelOut<Tensor<u32, [8]>>) {
    val t = thread_id
    out.partition(|base, slice| {
        val inner = t
        slice[0] = inner.x * 2 + base as u32
    })
}

func main() -> i32 {
    mut out: Tensor<u32, [6, 5]> = Tensor::zeros()
    mut blocks: Tensor<u32, [6, 5]> = Tensor::zeros()
    k(&mut out, &mut blocks)
    for i in 0..6 {
        for j in 0..5 {
            if out[i, j] != (i * 10 + j + 101) as u32 { return 1 }
            if blocks[i, j] != ((i / 4) * 100 + (j / 2) * 10) as u32 { return 2 }
        }
    }
    mut q: Tensor<u32, [8]> = Tensor::zeros()
    p(&mut q)
    for i in 0..8 {
        if q[i] != (i * 3) as u32 { return 3 }
    }
    println("ok")
    0
}
"#;
    let test = CompileTest::new();
    for level in ["0", "2"] {
        let Some(output) = run_at(&test, "grid_whole.nr", source, level) else {
            return;
        };
        assert_eq!(
            (
                output.status.code(),
                String::from_utf8_lossy(&output.stdout).as_ref()
            ),
            (Some(0), "ok\n"),
            "-O{level}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
