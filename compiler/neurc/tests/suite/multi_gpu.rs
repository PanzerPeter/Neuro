// Multiple GPUs: `Device::GPU(n)` for every GPU the machine has, each with its own
// context, stream, device arena and copy of the kernels.
//
// No machine the project runs on has two GPUs, so these programs run against
// `fake_libcuda.c`, a driver with two devices backed by host memory, which the runtime
// loads in place of the real one through `LD_LIBRARY_PATH`. Transfers really copy, so
// values survive a trip through both devices; kernels do not run, so a `@gpu` call is
// checked for where it ran, not for what it computed. The fake aborts on any use of a
// stream, module, function or buffer from a context other than its own, which is the
// bookkeeping a second device needs.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::compile_harness::CompileTest;

const VIOLATION: &str = "fake-cuda: violation";

/// Build the fake driver as `libcuda.so.1` in `dir`, beside the program under test.
fn build_fake_driver(dir: &Path) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite/fake_libcuda.c");
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(dir.join("libcuda.so.1"))
        .arg(&source)
        .status()
        .expect("cc should start: the suite links its programs with it");
    assert!(status.success(), "the fake driver should build");
}

fn compile(test: &CompileTest, source: &str) -> PathBuf {
    let path = test.write_source("multi_gpu.nr", source);
    let dir = path
        .parent()
        .expect("a source file has a directory")
        .to_path_buf();
    build_fake_driver(&dir);
    test.compile(&path).expect("a multi-GPU program compiles")
}

fn run_on_fake(exe: &Path, devices: u32) -> Output {
    Command::new(exe)
        .env(
            "LD_LIBRARY_PATH",
            exe.parent().expect("the binary has a directory"),
        )
        .env("FAKE_CUDA_DEVICES", devices.to_string())
        .output()
        .expect("the program should start")
}

fn assert_no_violation(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!stderr.contains(VIOLATION), "{stderr}");
    stderr
}

#[test]
fn a_tensor_moves_between_gpus_and_back_unchanged() {
    const PROGRAM: &str = r#"
func main() -> i32 {
    val a: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val on1 = a.to(Device::GPU(1))
    val on0 = on1.to(Device::GPU(0))
    val again = on0.to(Device::GPU(1))
    val same = again.to(Device::GPU(1))
    val back = same.to(Device::CPU)
    mut i = 0
    while i < 200 {
        val t = Tensor::<f32, [256]>::ones() * (i as f32)
        val round = t.to(Device::GPU(i % 2)).to(Device::GPU((i + 1) % 2)).to(Device::CPU)
        if round[255] != i as f32 { return 1 }
        i += 1
    }
    // Released on GPU 1 when `main` returns, from GPU 1's context.
    val kept = Tensor::<f32, [4]>::ones().to(Device::GPU(1))
    println("{back[0, 0]} {back[1, 2]}")
    return back.sum() as i32
}
"#;
    let test = CompileTest::new();
    let output = run_on_fake(&compile(&test, PROGRAM), 2);
    let stderr = assert_no_violation(&output);
    assert_eq!(output.status.code(), Some(21), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "1.0 6.0\n");
}

#[test]
fn a_gpu_index_the_machine_lacks_is_refused_with_the_count() {
    for (index, devices) in [("2", 2), ("-1", 2), ("1", 1)] {
        let source = format!(
            "func main() -> i32 {{\n    val g = Tensor::<f32, [4]>::ones().to(Device::GPU({index}))\n    return 0\n}}\n"
        );
        let test = CompileTest::new();
        let output = run_on_fake(&compile(&test, &source), devices);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.starts_with(&format!(
                "panic: `Device::GPU({index})` names no GPU: this machine has {devices}"
            )),
            "GPU({index}) with {devices} device(s): {stderr}"
        );
        assert!(!output.status.success());
    }
}

#[cfg(target_os = "linux")]
const BLEND: &str = r#"
@gpu
func blend(a: &Tensor<f32, [37, 45]>, b: &Tensor<f32, [37, 45]>) -> Tensor<f32, [37, 45]> {
    val s = a + b
    s * b
}
"#;

/// A call runs where its device operands live: GPU 1 loads its own copy of the kernels on
/// the first launch there, and its stream, arena and result are GPU 1's. An all-host call
/// still runs on GPU 0.
#[cfg(target_os = "linux")]
#[test]
fn a_gpu_call_runs_on_the_gpu_its_operands_live_on() {
    let program = format!(
        r#"{BLEND}
func main() -> i32 {{
    val a = Tensor::<f32, [37, 45]>::ones()
    val on_host = blend(&a, &a)
    val d1 = a.clone().to(Device::GPU(1))
    val resident = blend(&d1, &d1)
    val chained = blend(&resident, &d1)
    val mixed = blend(&a, &chained)
    pool {{
        mut i = 0
        while i < 50 {{
            val t = blend(&d1, &d1)
            i += 1
        }}
    }}
    val back = mixed.to(Device::CPU)
    println("done")
    return 0
}}
"#
    );
    let test = CompileTest::new();
    let output = run_on_fake(&compile(&test, &program), 2);
    let stderr = assert_no_violation(&output);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "done\n");

    let count = |line: &str| stderr.lines().filter(|l| *l == line).count();
    let modules = count("fake-cuda: module on device 0");
    assert!(modules > 0, "{stderr}");
    assert_eq!(
        count("fake-cuda: module on device 1"),
        modules,
        "GPU 1 loads each module once, on its first launch: {stderr}"
    );
    let on_zero = count("fake-cuda: launch on device 0");
    let on_one = count("fake-cuda: launch on device 1");
    // One all-host call against 53 on GPU 1, whatever the kernels per call.
    assert!(on_zero > 0 && on_one == 53 * on_zero, "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn operands_on_two_gpus_are_refused_at_the_call() {
    use std::os::unix::process::ExitStatusExt;
    let program = format!(
        r#"{BLEND}
func main() -> i32 {{
    val a = Tensor::<f32, [37, 45]>::ones()
    val x = a.clone().to(Device::GPU(0))
    val y = a.clone().to(Device::GPU(1))
    println("before")
    val z = blend(&x, &y)
    return 0
}}
"#
    );
    let test = CompileTest::new();
    let output = run_on_fake(&compile(&test, &program), 2);
    let stderr = assert_no_violation(&output);
    assert!(
        stderr.contains(
            "panic: a `@gpu` call's operands live on GPU 0 and GPU 1: move them to one device with `.to(Device::GPU(n))` first"
        ),
        "{stderr}"
    );
    assert_eq!(
        output.status.signal(),
        Some(6),
        "expected SIGABRT: {stderr}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "before\n");
}
