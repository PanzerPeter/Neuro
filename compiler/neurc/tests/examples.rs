// Integration test that compiles and runs *every* program under examples/.
//
// Discovery is automatic: the test walks examples/ recursively, collects each
// `.nr` file, and checks two things against two sources of truth:
//
//   exit code -> examples/expected.txt, one `<path>  <code>` line per example
//   stdout    -> the example's sibling `.out` file, byte for byte (line endings
//                normalized, so neither git's checkout policy nor the Windows CRT's
//                text-mode `\n` -> `\r\n` shows up as a difference)
//
// An example that writes nothing to standard output has no `.out` file, and the
// absence *is* the expectation: its stdout must be empty. So adding a silent
// example is one new line in expected.txt and nothing else, and adding a
// printing one is that line plus `<name>.out` holding exactly what it prints.
//
// The test fails loudly when the example set and its expectations drift apart:
//   - a `.nr` file with no manifest entry  -> failure (forces registration)
//   - a manifest entry with no `.nr` file  -> failure (stale entry)
//   - a `.out` file with no `.nr` beside it -> failure (stale golden file)
//   - any exit-code mismatch               -> failure
//   - any stdout mismatch                  -> failure, with both texts shown
// All discrepancies across all examples are collected and reported together,
// so one run shows every problem rather than stopping at the first.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Workspace root (two levels up from this crate's manifest dir).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()`. That assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Recursively collect every file under `dir` with extension `ext`, as paths
/// relative to it. Used for both the `.nr` programs and their `.out` golden files.
fn collect_by_extension(dir: &Path, ext: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = match std::fs::read_dir(&current) {
            Ok(entries) => entries,
            Err(e) => panic!("read_dir {}: {}", current.display(), e),
        };
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|found_ext| found_ext == ext) {
                let rel = path
                    .strip_prefix(dir)
                    .expect("strip examples prefix")
                    .to_string_lossy()
                    .replace('\\', "/"); // normalize for Windows
                found.push(rel);
            }
        }
    }
    found.sort();
    found
}

/// The manifest marker after an exit code naming an example that needs a GPU.
const GPU_MARKER: &str = "gpu";

/// How the GPU runtime begins its diagnostic when the machine has no usable GPU.
const NO_GPU_PANIC: &str = "panic: `@gpu` needs an NVIDIA GPU, and none is usable: ";

/// What the manifest says about one `.nr` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expectation {
    /// A standalone program: compile it and assert this exit code.
    Exit(i32),
    /// A program with `@gpu` functions, which only a Linux neurc compiles and only a
    /// machine with an NVIDIA GPU runs. On such a machine it is held to its exit code and
    /// output like any other; with no GPU it must abort at startup with the runtime's
    /// diagnostic; and off Linux it must type-check.
    Gpu(i32),
    /// A non-root module of a multi-file program. It has no `main` of its own and is
    /// compiled only as part of the root that reaches into it, so the harness records
    /// that it is accounted for and moves on.
    Module,
}

/// Parse `expected.txt`: `<relative-path>  <exit-code|module>` per line, the exit code
/// optionally followed by `gpu`; `#` comments and blank lines ignored.
fn parse_manifest(path: &Path) -> BTreeMap<String, Expectation> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read manifest {}: {}", path.display(), e));
    let mut map = BTreeMap::new();
    for (lineno, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let rel = parts
            .next()
            .unwrap_or_else(|| panic!("manifest line {}: missing path", lineno + 1));
        let marker = parts
            .next()
            .unwrap_or_else(|| panic!("manifest line {}: missing exit code", lineno + 1));
        let code = || -> i32 {
            marker
                .parse()
                .unwrap_or_else(|e| panic!("manifest line {}: bad exit code: {}", lineno + 1, e))
        };
        let expectation = match (marker, parts.next()) {
            ("module", None) => Expectation::Module,
            (_, None) => Expectation::Exit(code()),
            (_, Some(GPU_MARKER)) => Expectation::Gpu(code()),
            (_, Some(other)) => panic!("manifest line {}: unknown marker `{other}`", lineno + 1),
        };
        if map.insert(rel.to_string(), expectation).is_some() {
            panic!("manifest line {}: duplicate entry for {}", lineno + 1, rel);
        }
    }
    map
}

/// Compile `examples/<rel>` to a temp binary, returning its path or an error.
fn compile_example(examples_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let src = examples_dir.join(rel);
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("neuro_example_{}", rel.replace(['/', '.'], "_")));

    let output = Command::new(neurc_path())
        .arg("compile")
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .map_err(|e| format!("failed to run neurc: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "compile failed\n  stdout: {}\n  stderr: {}",
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(out)
}

/// Run `exe` and return what it wrote to standard output together with its exit code.
fn run_example(exe: &Path) -> Result<(i32, String), String> {
    let output = Command::new(exe)
        .output()
        .map_err(|e| format!("failed to run {}: {}", exe.display(), e))?;
    let code = output
        .status
        .code()
        .ok_or_else(|| format!("{} terminated by signal", exe.display()))?;
    // Line endings are normalized: on Windows the runtime's `\n` leaves the CRT's
    // text-mode fd 1 as `\r\n`, and the golden files are checked out with whichever
    // ending git was configured for. Neither is what an example is asserting.
    let text = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    Ok((code, text))
}

/// The stdout `examples/<rel>` must produce.
///
/// It lives in the sibling `.out` file. No such file means the example is a silent
/// one and must print nothing, so the absence is itself the expectation: that is
/// what keeps a program which quietly starts printing from passing unnoticed.
fn expected_stdout(examples_dir: &Path, rel: &str) -> Result<String, String> {
    let golden = examples_dir.join(rel).with_extension("out");
    match std::fs::read_to_string(&golden) {
        Ok(text) => Ok(text.replace("\r\n", "\n")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("read {}: {}", golden.display(), e)),
    }
}

/// Render a stdout mismatch: the first line that differs, then both texts in full.
///
/// Trailing whitespace is invisible in a terminal and is exactly the kind of
/// difference a golden file exists to catch, so every line is quoted.
fn describe_stdout_mismatch(rel: &str, expected: &str, actual: &str) -> String {
    let golden = rel.trim_end_matches(".nr").to_string() + ".out";
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    let mut first_diff = String::from("(differs in trailing newline only)");
    let mut lineno = 1;
    loop {
        match (expected_lines.next(), actual_lines.next()) {
            (None, None) => break,
            (want, got) if want == got => lineno += 1,
            (want, got) => {
                first_diff = format!(
                    "line {lineno}: expected {}, got {}",
                    want.map_or("<end of output>".to_string(), |l| format!("{l:?}")),
                    got.map_or("<end of output>".to_string(), |l| format!("{l:?}")),
                );
                break;
            }
        }
    }
    format!(
        "stdout does not match {golden}\n    {first_diff}\n\
         --- expected ({} bytes) ---\n{}\n\
         --- actual ({} bytes) ---\n{}\n\
         --- end ---",
        expected.len(),
        quote_lines(expected),
        actual.len(),
        quote_lines(actual),
    )
}

/// Quote every line of `text` so trailing spaces and empty lines stay visible.
fn quote_lines(text: &str) -> String {
    if text.is_empty() {
        return "    <nothing>".to_string();
    }
    text.lines()
        .map(|line| format!("    {line:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a `gpu` example's first look decided.
enum GpuOutcome {
    /// Nothing more to check (it type-checked, or aborted for want of a GPU), or why not.
    Settled(Result<(), String>),
    /// A GPU ran it: hold it to its pins like any other example.
    RanOnGpu,
}

/// Off Linux a `gpu` example cannot compile, so it must at least type-check. On Linux it is
/// compiled and run once: a machine with no GPU must produce the runtime's startup
/// abort, and a machine with one goes on to the ordinary pin checks, which run it again.
fn gpu_example_outcome(examples_dir: &Path, rel: &str) -> GpuOutcome {
    if !cfg!(target_os = "linux") {
        let output = Command::new(neurc_path())
            .arg("check")
            .arg(examples_dir.join(rel))
            .output();
        return GpuOutcome::Settled(match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => Err(format!(
                "does not type-check:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(e) => Err(format!("failed to run neurc: {e}")),
        });
    }
    let exe = match compile_example(examples_dir, rel) {
        Ok(exe) => exe,
        Err(e) => return GpuOutcome::Settled(Err(e)),
    };
    let output = match Command::new(&exe).output() {
        Ok(output) => output,
        Err(e) => return GpuOutcome::Settled(Err(format!("failed to run: {e}"))),
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.starts_with(NO_GPU_PANIC) {
        return GpuOutcome::RanOnGpu;
    }
    GpuOutcome::Settled(
        match output.stdout.is_empty() && output.status.code() != Some(0) {
            true => Ok(()),
            false => Err(format!(
                "reported no GPU but did not abort at startup: {stderr}"
            )),
        },
    )
}

#[test]
fn all_examples_compile_run_and_match_manifest() {
    let examples_dir = workspace_root().join("examples");
    let manifest = parse_manifest(&examples_dir.join("expected.txt"));
    let discovered = collect_by_extension(&examples_dir, "nr");

    let mut failures: Vec<String> = Vec::new();

    // Every discovered example must be registered and behave as registered.
    for rel in &discovered {
        let Some(&expectation) = manifest.get(rel) else {
            failures.push(format!(
                "{rel}: present on disk but missing from examples/expected.txt \
                 (add a line: `{rel}  <exit-code>`)"
            ));
            continue;
        };
        let expected_code = match expectation {
            Expectation::Module => continue,
            Expectation::Exit(code) => code,
            Expectation::Gpu(code) => match gpu_example_outcome(&examples_dir, rel) {
                GpuOutcome::Settled(Ok(())) => continue,
                GpuOutcome::Settled(Err(e)) => {
                    failures.push(format!("{rel}: {e}"));
                    continue;
                }
                GpuOutcome::RanOnGpu => code,
            },
        };
        let expected_text = match expected_stdout(&examples_dir, rel) {
            Ok(text) => text,
            Err(e) => {
                failures.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let exe = match compile_example(&examples_dir, rel) {
            Ok(exe) => exe,
            Err(e) => {
                failures.push(format!("{rel}: {e}"));
                continue;
            }
        };
        match run_example(&exe) {
            Ok((code, stdout)) => {
                if code != expected_code {
                    failures.push(format!("{rel}: exit code {code}, expected {expected_code}"));
                }
                if stdout != expected_text {
                    failures.push(format!(
                        "{rel}: {}",
                        describe_stdout_mismatch(rel, &expected_text, &stdout)
                    ));
                }
            }
            Err(e) => failures.push(format!("{rel}: {e}")),
        }
    }

    // Every manifest entry must correspond to a real file.
    for rel in manifest.keys() {
        if !discovered.contains(rel) {
            failures.push(format!(
                "{rel}: listed in examples/expected.txt but no such file on disk"
            ));
        }
    }

    // Every golden file must belong to an example, or it is describing a program
    // that no longer exists and is silently asserting nothing.
    for golden in collect_by_extension(&examples_dir, "out") {
        let owner = golden.trim_end_matches(".out").to_string() + ".nr";
        if !discovered.contains(&owner) {
            failures.push(format!(
                "{golden}: golden output file with no {owner} beside it"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} example issue(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
