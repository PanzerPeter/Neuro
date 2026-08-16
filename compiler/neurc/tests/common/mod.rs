// Shared test utilities for neurc integration tests
// Provides CompileTest helper for end-to-end compilation testing

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Path to the `neurc` binary Cargo built for this test run.
///
/// Cargo sets `CARGO_BIN_EXE_neurc` for integration tests in the `neurc`
/// package; it is absolute and already carries the platform executable
/// suffix. Do not derive it from `current_exe()` — that assumes the legacy
/// `target/<profile>/deps/` layout and breaks under Cargo's build-dir layout.
fn neurc_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Helper struct for running end-to-end compilation tests
pub struct CompileTest {
    temp_dir: TempDir,
}

impl CompileTest {
    pub fn new() -> Self {
        Self {
            temp_dir: TempDir::new().expect("Failed to create temp directory"),
        }
    }

    /// Write source code to a temporary .nr file and return its path.
    ///
    /// `filename` may carry directories (`utils/mod.nr`) so a multi-file program can be
    /// laid out the way module resolution expects to find it.
    pub fn write_source(&self, filename: &str, source: &str) -> PathBuf {
        let source_path = self.temp_dir.path().join(filename);
        if let Some(parent) = source_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create source directory");
        }
        fs::write(&source_path, source).expect("Failed to write source file");
        source_path
    }

    /// Compile a source file and return the path to the executable
    pub fn compile(&self, source_path: &PathBuf) -> Result<PathBuf, String> {
        let output_path = source_path.with_extension(if cfg!(target_os = "windows") {
            "exe"
        } else {
            ""
        });

        // Run the compiler
        let output = Command::new(neurc_path())
            .arg("compile")
            .arg(source_path)
            .arg("-o")
            .arg(&output_path)
            .output()
            .expect("Failed to execute neurc");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "Compilation failed:\nstdout: {}\nstderr: {}",
                stdout, stderr
            ));
        }

        Ok(output_path)
    }

    /// Run an executable and return its exit code
    pub fn run_executable(&self, exe_path: &PathBuf) -> Result<i32, String> {
        let output = Command::new(exe_path)
            .output()
            .map_err(|e| format!("Failed to execute {}: {}", exe_path.display(), e))?;

        Ok(output.status.code().unwrap_or(-1))
    }

    /// Compile and run a program, returning its exit code
    pub fn compile_and_run(&self, filename: &str, source: &str) -> Result<i32, String> {
        let source_path = self.write_source(filename, source);
        let exe_path = self.compile(&source_path)?;
        self.run_executable(&exe_path)
    }
}
