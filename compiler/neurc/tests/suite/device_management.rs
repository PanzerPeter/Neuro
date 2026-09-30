// Device management: `.to(Device::GPU(n))` moves a tensor's buffer into GPU memory and
// `.to(Device::CPU)` brings it back, and a `@gpu` call reads a tensor already there in
// place.
//
// A transfer needs the GPU runtime but not the MLIR backend, so these programs compile on
// every build. Only a machine with an NVIDIA GPU can run a transfer to the end; CI has none,
// so each run test accepts the one other outcome a GPU-less machine can give, and asserts
// that fully too: the transfer's own diagnostic, raised at the transfer rather than at
// startup. `CUDA_VISIBLE_DEVICES` hides every device to pin that diagnostic anywhere.

use std::process::{Command, Output};

use crate::compile_harness::CompileTest;

const NO_GPU: &str = "panic: `Device::GPU` needs an NVIDIA GPU, and none is usable: ";

const ROUND_TRIP: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    println("before")
    val g = a.to(Device::GPU(0))
    val same = g.to(Device::GPU(0))
    val back = same.to(Device::CPU)
    val kept = back.to(Device::CPU)
    mut i = 0
    while i < 500 {
        val t = Tensor::<f32, [1024]>::ones() * (i as f32)
        val round = t.to(Device::GPU(0)).to(Device::CPU)
        if round[1023] != i as f32 { return 1 }
        i += 1
    }
    println("{kept[0, 0]} {kept[1, 2]}")
    return kept.sum() as i32
}
"#;

fn run(exe: &std::path::Path, hide_devices: bool) -> Output {
    let mut command = Command::new(exe);
    if hide_devices {
        command.env("CUDA_VISIBLE_DEVICES", "");
    }
    command.output().expect("the program should start")
}

/// The transfer's diagnostic: raised at the transfer, so whatever `main` printed before it
/// is still there.
#[cfg(unix)]
fn assert_no_gpu_at_the_transfer(output: &Output) {
    use std::os::unix::process::ExitStatusExt;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with(NO_GPU), "{stderr}");
    assert_eq!(
        output.status.signal(),
        Some(6),
        "expected SIGABRT: {stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
}

fn compile(test: &CompileTest, name: &str, source: &str) -> std::path::PathBuf {
    test.compile(&test.write_source(name, source))
        .expect("a device transfer compiles on every build")
}

#[cfg(unix)]
#[test]
fn a_tensor_round_trips_through_the_gpu_unchanged() {
    let test = CompileTest::new();
    let output = run(&compile(&test, "round_trip.nr", ROUND_TRIP), false);
    if String::from_utf8_lossy(&output.stderr).starts_with(NO_GPU) {
        assert_no_gpu_at_the_transfer(&output);
        return;
    }
    assert_eq!(
        output.status.code(),
        Some(21),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n1.0 6.0\n");
}

#[cfg(unix)]
#[test]
fn without_a_visible_gpu_the_transfer_aborts_with_the_reason() {
    let test = CompileTest::new();
    assert_no_gpu_at_the_transfer(&run(&compile(&test, "hidden.nr", ROUND_TRIP), true));
}

/// No machine has a GPU 7 or a GPU -1. Without a usable GPU the refusal is the missing
/// GPU; with one, it names the index and the count. `multi_gpu.rs` pins the count.
#[cfg(unix)]
#[test]
fn a_gpu_index_the_machine_lacks_is_refused() {
    for index in ["7", "-1"] {
        let source = format!(
            "func main() -> i32 {{\n    val g = Tensor::<f32, [4]>::ones().to(Device::GPU({index}))\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run(&compile(&test, "index.nr", &source), false);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "GPU({index}) must not succeed");
        assert!(
            stderr.starts_with(NO_GPU)
                || stderr.starts_with(&format!("panic: `Device::GPU({index})` names no GPU")),
            "{stderr}"
        );
    }
}

/// Host code cannot dereference device memory, so every host tensor operation checks where
/// its operand lives and names the way out, at the operation.
#[cfg(unix)]
#[test]
fn host_code_refuses_a_device_tensor_at_the_operation() {
    // Each operation's tensor operand starts in column 13, after `    val r = `.
    for operation in [
        "val r = g.sum()",
        "val r = g[1]",
        "val r = &g + &g",
        "val r = g.clone()",
        "val r = g.map(|v: f32| v * 2.0f32)",
    ] {
        let source = format!(
            "func main() -> i32 {{\n    val g = Tensor::<f32, [4]>::ones().to(Device::GPU(0))\n    println(\"before\")\n    {operation}\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run(&compile(&test, "host_read.nr", &source), false);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "`{operation}` must not read device memory"
        );
        if stderr.starts_with(NO_GPU) {
            continue;
        }
        assert!(
            stderr.starts_with(
                "panic: this tensor lives on a GPU, where host code cannot read it: move it back with `.to(Device::CPU)` first at "
            ) && stderr.contains("host_read.nr:4:13"),
            "`{operation}`: {stderr}"
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
    }
}

/// Device operands are read where they are and the result stays with them; a host operand
/// beside one is staged as before. Chained calls never touch the host until `.to(CPU)`.
#[cfg(all(feature = "mlir", unix))]
#[test]
fn a_gpu_call_reads_device_operands_in_place_and_keeps_its_result_there() {
    const KERNELS: &str = r#"
@gpu
func blend(a: &Tensor<f32, [37, 45]>, b: &Tensor<f32, [37, 45]>) -> Tensor<f32, [37, 45]> {
    val s = a + b
    s * b
}

@gpu
func project(w: Tensor<f32, [3, 2]>, x: &Tensor<f32, [2, 2]>) -> Tensor<f32, [3, 2]> {
    w @ x
}

func main() -> i32 {
    val a = Tensor::<f32, [37, 45]>::ones()
    val b = &a * 2.0f32
    val da = a.clone().to(Device::GPU(0))
    val db = b.clone().to(Device::GPU(0))
    val resident = blend(&da, &db)
    val chained = blend(&resident, &db)
    val mixed = blend(&a, &db)
    val host = blend(&a, &b)
    val c = chained.to(Device::CPU)
    val m = mixed.to(Device::CPU)
    println("{host[0, 0]} {c[36, 44]} {m[5, 7]}")
    val w: Tensor<f32, [3, 2]> = [[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]]
    val x: Tensor<f32, [2, 2]> = [[1.0, 0.0], [1.0, 1.0]]
    val p = project(w.to(Device::GPU(0)), &x).to(Device::CPU)
    println("{p[0, 0]} {p[2, 1]}")
    pool {
        mut i = 0
        while i < 200 {
            val t = blend(&da, &db)
            i += 1
        }
    }
    return 0
}
"#;
    let test = CompileTest::new();
    let output = run(&compile(&test, "resident.nr", KERNELS), false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("none is usable") {
        // A bare `@gpu` function makes a missing GPU fatal before `main`.
        assert!(output.stdout.is_empty(), "{stderr}");
        return;
    }
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "6.0 16.0 6.0\n3.0 6.0\n"
    );
}
