// `--gpu-arch gfxNNN`: `@gpu` bodies lowered through `rocdl` to an AMD code object, and the
// GPU runtime over HIP in place of the CUDA driver.
//
// No machine this suite runs on has an AMD GPU, so what is pinned is everything short of a
// kernel running: the build, the runtime's refusal when HIP is absent or every device is
// hidden by `HIP_VISIBLE_DEVICES`, and a fallback program's host answers. Linking a code
// object runs `$ROCM_PATH/llvm/bin/ld.lld`, and ROCm's is stock LLVM lld, so the tests point
// `ROCM_PATH` at a directory holding a link to the `ld.lld` on `PATH`.

use std::process::{Command, Output};

use crate::compile_harness::CompileTest;

const TRANSFER: &str = r#"
func main() -> i32 {
    val a = Tensor::<f32, [4]>::ones()
    val g = a.to(Device::GPU(0))
    val h = g.to(Device::CPU)
    println("{h[0]}")
    return 0
}
"#;

fn neurc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_neurc"))
}

/// Run `exe` with every AMD device hidden, so the result is the same with or without one.
#[cfg(unix)]
fn run_without_devices(exe: &std::path::Path) -> Output {
    Command::new(exe)
        .env("HIP_VISIBLE_DEVICES", "")
        .output()
        .expect("the program should start")
}

#[cfg(unix)]
fn assert_aborted(output: &Output, prefix: &str) {
    use std::os::unix::process::ExitStatusExt;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with(prefix), "{stderr}");
    assert_eq!(
        output.status.signal(),
        Some(6),
        "expected SIGABRT: {stderr}"
    );
}

#[test]
fn a_chip_that_names_no_vendor_is_refused() {
    let test = CompileTest::new();
    let source = test.write_source("chip.nr", TRANSFER);
    let output = neurc()
        .args(["compile", "--gpu-arch", "rdna3"])
        .arg(&source)
        .output()
        .expect("Failed to execute neurc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains("expected an NVIDIA `sm_NN` or an AMD `gfxNNN` chip"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn a_transfer_built_for_amd_goes_through_hip() {
    let test = CompileTest::new();
    let source = test.write_source("transfer.nr", TRANSFER);
    let exe = source.with_extension("");
    let output = neurc()
        .args(["compile", "--gpu-arch", "gfx90a", "-o"])
        .arg(&exe)
        .arg(&source)
        .output()
        .expect("Failed to execute neurc");
    assert!(output.status.success(), "{output:?}");

    let run = run_without_devices(&exe);
    assert_aborted(
        &run,
        "panic: `Device::GPU` needs an AMD GPU, and none is usable: ",
    );
    assert!(run.stdout.is_empty(), "the transfer must not complete");
}

#[cfg(all(feature = "mlir", unix))]
mod kernels {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A stand-in ROCm install under `dir`: only its linker, which is what serializing a
    /// code object runs.
    fn rocm_with_lld(dir: &Path) -> PathBuf {
        let lld = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|path| path.join("ld.lld"))
            .find(|path| path.is_file())
            .expect("building for AMD links a code object with `ld.lld`; install lld");
        let rocm = dir.join("rocm");
        let bin = rocm.join("llvm").join("bin");
        std::fs::create_dir_all(&bin).expect("the stand-in ROCm directory is created");
        std::os::unix::fs::symlink(lld, bin.join("ld.lld")).expect("ld.lld is linked in");
        rocm
    }

    /// Compile `source` for gfx90a with `rocm` as `ROCM_PATH`, adding `args`.
    fn compile_for_amd(source: &Path, rocm: &Path, args: &[&str]) -> Output {
        neurc()
            .args(["compile", "--gpu-arch", "gfx90a"])
            .args(args)
            .arg(source)
            .env("ROCM_PATH", rocm)
            .output()
            .expect("Failed to execute neurc")
    }

    fn examples_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
    }

    #[test]
    fn the_kernels_are_a_code_object_loaded_through_hip() {
        let test = CompileTest::new();
        let source = test.write_source(
            "fallback.nr",
            &std::fs::read_to_string(examples_dir().join("showcase/gpu_fallback.nr"))
                .expect("the showcase is readable"),
        );
        let rocm = rocm_with_lld(source.parent().expect("the source has a directory"));
        let ir_path = source.with_extension("ll");
        let output = compile_for_amd(
            &source,
            &rocm,
            &[
                "--emit",
                "llvm-ir",
                "-o",
                ir_path.to_str().expect("utf-8 path"),
            ],
        );
        assert!(output.status.success(), "{output:?}");

        let ir = std::fs::read_to_string(&ir_path).expect("the IR is written");
        assert!(
            ir.contains("call ptr @mgpuModuleLoad(")
                && !ir.contains("call ptr @mgpuModuleLoadJIT("),
            "a code object is loaded as-is, never JIT compiled:\n{ir}"
        );
        assert!(ir.contains("\\7FELF") && ir.contains("amdgcn-amd-amdhsa--gfx90a"));
        assert!(ir.contains("libamdhip64.so") && !ir.contains("libcuda"));
    }

    #[test]
    fn a_fallback_showcase_built_for_amd_gives_its_pinned_answers() {
        let examples = examples_dir();
        let expected_code: i32 = std::fs::read_to_string(examples.join("expected.txt"))
            .expect("the exit-code pins are readable")
            .lines()
            .find_map(|line| line.strip_prefix("showcase/gpu_fallback.nr"))
            .and_then(|code| code.trim().parse().ok())
            .expect("the showcase has an exit-code pin");
        let expected_stdout = std::fs::read(examples.join("showcase/gpu_fallback.out"))
            .expect("the showcase has a stdout pin");

        let test = CompileTest::new();
        let source = test.write_source(
            "fallback.nr",
            &std::fs::read_to_string(examples.join("showcase/gpu_fallback.nr"))
                .expect("the showcase is readable"),
        );
        let rocm = rocm_with_lld(source.parent().expect("the source has a directory"));
        let exe = source.with_extension("");
        let output = compile_for_amd(&source, &rocm, &["-o", exe.to_str().expect("utf-8 path")]);
        assert!(output.status.success(), "{output:?}");

        let run = run_without_devices(&exe);
        assert_eq!(
            run.status.code(),
            Some(expected_code),
            "stderr: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(run.stdout, expected_stdout);
    }

    #[test]
    fn a_bare_gpu_program_built_for_amd_aborts_before_main() {
        let test = CompileTest::new();
        let source = test.write_source(
            "bare.nr",
            "@gpu\nfunc double(a: &Tensor<f32, [8]>) -> Tensor<f32, [8]> {\n    a + a\n}\n\
             func main() -> i32 {\n    println(\"main ran\")\n    return 0\n}\n",
        );
        let rocm = rocm_with_lld(source.parent().expect("the source has a directory"));
        let exe = source.with_extension("");
        let output = compile_for_amd(&source, &rocm, &["-o", exe.to_str().expect("utf-8 path")]);
        assert!(output.status.success(), "{output:?}");

        let run = run_without_devices(&exe);
        assert_aborted(&run, "panic: `@gpu` needs an AMD GPU, and none is usable: ");
        assert!(run.stdout.is_empty(), "main must not run");
    }

    #[test]
    fn without_rocm_an_amd_build_says_what_is_missing() {
        let test = CompileTest::new();
        let source = test.write_source(
            "no_rocm.nr",
            "@gpu\nfunc double(a: &Tensor<f32, [8]>) -> Tensor<f32, [8]> {\n    a + a\n}\n\
             func main() -> i32 { return 0 }\n",
        );
        let empty = source
            .parent()
            .expect("the source has a directory")
            .join("empty");
        let output = compile_for_amd(&source, &empty, &[]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{stderr}");
        assert!(
            stderr.contains("an AMD target needs ROCm installed"),
            "{stderr}"
        );
    }
}
