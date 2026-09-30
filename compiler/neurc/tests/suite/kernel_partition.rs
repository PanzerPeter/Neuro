// `out.partition(|base, slice| { ... })`: the write form whose disjointness the compiler
// proves, and `.flat(i)`, the row-major read it is written with.
//
// The run tests follow `kernel_attribute.rs`: only a neurc built with `--features mlir`
// compiles a kernel, and a machine without an NVIDIA GPU aborts it at startup, which each
// test accepts and asserts in full.

use crate::compile_harness::CompileTest;

/// A chunk-1 partition over a grid that overhangs its tensor, a chunk-3 partition of a
/// second output, and a `return` that leaves only the closure: every element checked
/// against the host.
#[cfg(feature = "mlir")]
const SPLIT: &str = r#"
@kernel(threads: [16, 16])
func split(a: Tensor<f32, [37, 45]>, out: KernelOut<Tensor<f32, [37, 45]>>, wide: KernelOut<Tensor<i64, [37, 45, 3]>>) {
    out.partition(|base, slice| {
        for i in 0u64..slice.len() {
            val sum = a.flat(base + i) - 700.0
            slice[i] = if sum > 0.0 { sum } else { 0.0 }
        }
    })
    wide.partition(|base, s| {
        s[0] = base as i64
        s[1] = s[0] + 1
        if base > 0u64 {
            return
        }
        s[2] = -1
    })
    wide.partition(|base, s| {
        s[2] += 10
    })
}

func main() -> i32 {
    println("main ran")
    mut a: Tensor<f32, [37, 45]> = Tensor::zeros()
    for i in 0..37 {
        for j in 0..45 {
            a[i, j] = (i * 45 + j) as f32
        }
    }
    mut r: Tensor<f32, [37, 45]> = Tensor::zeros()
    mut w: Tensor<i64, [37, 45, 3]> = Tensor::zeros()
    split(a, &mut r, &mut w)
    for i in 0..37 {
        for j in 0..45 {
            val s = a[i, j] - 700.0
            val want = if s > 0.0 { s } else { 0.0 }
            if r[i, j] != want { return 1 }
            val n = (i * 45 + j) as i64
            if w[i, j, 0] != n * 3 || w[i, j, 1] != n * 3 + 1 { return 2 }
            val third: i64 = if n == 0 { 9 } else { 10 }
            if w[i, j, 2] != third { return 3 }
        }
    }
    return 0
}
"#;

#[test]
fn a_malformed_partition_is_refused_by_the_checker() {
    let kernel = |body: &str| {
        format!(
            "@kernel(threads: [4])\nfunc k(a: Tensor<f32, [8]>, out: KernelOut<Tensor<f32, [8]>>, wide: KernelOut<Tensor<f32, [12]>>) {{\n    {body}\n}}\nfunc main() -> i32 {{ return 0 }}\n"
        )
    };
    for (body, problem) in [
        (
            "out.partition(|base| {})",
            "output 'out' is partitioned by one closure of two parameters",
        ),
        (
            "out.partition(|base: i32, slice| {})",
            "type mismatch: expected u64, found i32",
        ),
        (
            "wide.partition(|base, slice| { slice[0] = 1.0 })",
            "output 'wide' has 12 elements, which the grid's 8 threads cannot share equally",
        ),
        (
            "wide.partition(|base, slice| { out[base] = 1.0 })",
            "output 'out' is written one element at a time",
        ),
        (
            "val x = out.flat(0)",
            "output 'out' is written one element at a time",
        ),
    ] {
        let error = CompileTest::new()
            .check("partition.nr", &kernel(body))
            .expect_err("`check` should refuse the partition");
        assert!(
            error.contains(problem) && error.contains("partition.nr:3:"),
            "expected `{problem}` in:\n{error}"
        );
    }
}

/// `.flat` is not a kernel construct: on the host it reads the same element and panics
/// past the end like any index.
#[test]
fn flat_reads_a_tensor_by_its_row_major_position() {
    let source = "func main() -> i32 {\n    mut t: Tensor<i32, [2, 3, 4]> = Tensor::zeros()\n    for i in 0..2 {\n        for j in 0..3 {\n            for k in 0..4 {\n                t[i, j, k] = i * 100 + j * 10 + k\n            }\n        }\n    }\n    val r = &t\n    if t.flat(0) != 0 || t.flat(13) != 101 || r.flat(23) != 123 {\n        return 1\n    }\n    return t.flat(POSITION)\n}\n";
    let test = CompileTest::new();
    assert_eq!(
        test.compile_and_run("flat.nr", &source.replace("POSITION", "5")),
        Ok(11)
    );
    let exe = test
        .compile(&test.write_source("past.nr", &source.replace("POSITION", "24")))
        .expect("the program should compile");
    let run = std::process::Command::new(&exe)
        .output()
        .expect("the program should start");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        !run.status.success() && stderr.contains("panic: tensor index out of bounds"),
        "position 24 of 24 elements is past the end: {stderr}"
    );
    let error = test
        .check(
            "dynamic.nr",
            "func f(t: &Tensor<f32, [?, 3]>) -> f32 {\n    t.flat(1)\n}\nfunc main() -> i32 { return 0 }\n",
        )
        .expect_err("a `?` extent has no strides to read by");
    assert!(
        error.contains("`.flat`") && error.contains("dynamic.nr:2:5"),
        "{error}"
    );
}

#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_partition_writes_each_threads_run_on_the_gpu() {
    let test = CompileTest::new();
    for optimization in ["-O0", "-O2"] {
        let source = test.write_source("split.nr", SPLIT);
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

#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_slice_index_past_the_run_stops_the_kernel_and_the_program() {
    use std::os::unix::process::ExitStatusExt;
    let test = CompileTest::new();
    // Eight threads share sixteen elements two apiece, so `s[2]` is the next thread's.
    let exe = test
        .compile(&test.write_source(
            "past_run.nr",
            "@kernel(threads: [4])\nfunc k(out: KernelOut<Tensor<f32, [8]>>, wide: KernelOut<Tensor<f32, [8, 2]>>) {\n    wide.partition(|base, s| {\n        s[2] = 1.0\n    })\n}\nfunc main() -> i32 {\n    mut o: Tensor<f32, [8]> = Tensor::zeros()\n    mut w: Tensor<f32, [8, 2]> = Tensor::zeros()\n    k(&mut o, &mut w)\n    return 0\n}\n",
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
