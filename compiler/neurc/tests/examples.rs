// Integration test that compiles and runs *every* program under examples/.
//
// Discovery is automatic: the test walks examples/ recursively, collects each
// `.nr` file, and checks its exit code against examples/expected.txt (the
// single source of truth for expected codes). Adding a new example therefore
// requires only dropping the `.nr` file into a subdirectory and adding one
// line to examples/expected.txt — no edits to this file.
//
// The test fails loudly when the example set and the manifest drift apart:
//   - a `.nr` file with no manifest entry  -> failure (forces registration)
//   - a manifest entry with no `.nr` file  -> failure (stale entry)
//   - any exit-code mismatch               -> failure
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

/// Path to the freshly built `neurc` binary that sits beside the test binary.
fn neurc_path() -> PathBuf {
    std::env::current_exe()
        .expect("current exe")
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile> dir")
        .join("neurc")
}

/// Recursively collect every `.nr` file under `dir`, as paths relative to it.
fn collect_examples(dir: &Path) -> Vec<String> {
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
            } else if path.extension().is_some_and(|ext| ext == "nr") {
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

/// Parse `expected.txt`: `<relative-path>  <exit-code>` per line; `#` comments
/// and blank lines ignored.
fn parse_manifest(path: &Path) -> BTreeMap<String, i32> {
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
        let code: i32 = parts
            .next()
            .unwrap_or_else(|| panic!("manifest line {}: missing exit code", lineno + 1))
            .parse()
            .unwrap_or_else(|e| panic!("manifest line {}: bad exit code: {}", lineno + 1, e));
        if map.insert(rel.to_string(), code).is_some() {
            panic!("manifest line {}: duplicate entry for {}", lineno + 1, rel);
        }
    }
    map
}

/// Compile `examples/<rel>` to a temp binary, returning its path or an error.
fn compile_example(examples_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let src = examples_dir.join(rel);
    let out = std::env::temp_dir().join(format!("neuro_example_{}", rel.replace(['/', '.'], "_")));

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

fn run_exit_code(exe: &Path) -> Result<i32, String> {
    Command::new(exe)
        .output()
        .map_err(|e| format!("failed to run {}: {}", exe.display(), e))?
        .status
        .code()
        .ok_or_else(|| format!("{} terminated by signal", exe.display()))
}

#[test]
fn all_examples_compile_run_and_match_manifest() {
    let examples_dir = workspace_root().join("examples");
    let manifest = parse_manifest(&examples_dir.join("expected.txt"));
    let discovered = collect_examples(&examples_dir);

    let mut failures: Vec<String> = Vec::new();

    // Every discovered example must be registered and behave as registered.
    for rel in &discovered {
        let Some(&expected) = manifest.get(rel) else {
            failures.push(format!(
                "{rel}: present on disk but missing from examples/expected.txt \
                 (add a line: `{rel}  <exit-code>`)"
            ));
            continue;
        };
        match compile_example(&examples_dir, rel) {
            Ok(exe) => match run_exit_code(&exe) {
                Ok(code) if code == expected => {}
                Ok(code) => failures.push(format!("{rel}: exit code {code}, expected {expected}")),
                Err(e) => failures.push(format!("{rel}: {e}")),
            },
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

    assert!(
        failures.is_empty(),
        "{} example issue(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
