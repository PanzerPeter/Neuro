//! Run command implementation

use std::path::PathBuf;
use std::fs;
use std::process::Command;
use anyhow::{Context, Result, anyhow};
use lexical_analysis::Lexer;
use syntax_parsing::Parser;
use semantic_analysis::analyze_program;
use llvm_backend::{compile_to_executable, execute_program_jit};
use tempfile::tempdir;

/// Run a NEURO program directly (compile and execute)
pub fn run_run(
    file: PathBuf,
    opt_level: u8,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("Running program: {}", file.display());
    }

    // Read source file
    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read source file: {}", file.display()))?;

    // Lexical analysis
    if verbose {
        println!("Phase 1: Lexical analysis...");
    }
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()
        .with_context(|| "Lexical analysis failed")?;

    // Syntax parsing
    if verbose {
        println!("Phase 2: Syntax parsing...");
    }
    let mut parser = Parser::new(tokens);
    let ast = parser.parse()
        .with_context(|| "Syntax parsing failed")?;

    // Semantic analysis
    if verbose {
        println!("Phase 3: Semantic analysis...");
    }
    let _semantic_info = analyze_program(&ast)
        .with_context(|| "Semantic analysis failed")?;

    // Create temporary executable
    let temp_dir = tempdir()
        .with_context(|| "Failed to create temporary directory")?;

    let exe_name = if cfg!(windows) {
        "temp_program.exe"
    } else {
        "temp_program"
    };
    let temp_exe = temp_dir.path().join(exe_name);

    // Extract module name from file path
    let module_name = file.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");

    // Try to build executable first, fallback to JIT execution
    if verbose {
        println!("Phase 4: Attempting to generate executable...");
    }

    match compile_to_executable(&ast, module_name, &temp_exe) {
        Ok(executable_path) => {
            // Successfully compiled to executable - run it
            if verbose {
                println!("✓ Executable generated successfully");
                println!("Phase 5: Executing compiled program...");
                println!("Program Output:");
                println!("==================");
            }

            let mut cmd = Command::new(&executable_path);
            let output = cmd.output()
                .with_context(|| format!("Failed to execute program: {}", executable_path.display()))?;

            // Display program output
            if !output.stdout.is_empty() {
                let stdout_str = String::from_utf8_lossy(&output.stdout);
                print!("{}", stdout_str);
            }

            if !output.stderr.is_empty() {
                let stderr_str = String::from_utf8_lossy(&output.stderr);
                eprintln!("{}", stderr_str);
            }

            // Check exit code and report it
            let exit_code = output.status.code().unwrap_or(-1);
            if verbose {
                println!("==================");
                println!("Program exited with code: {}", exit_code);
            } else {
                // Always show exit code for non-verbose mode too
                println!("Program exited with code: {}", exit_code);
            }

            // Only treat negative exit codes as errors (indicating system-level failures)
            if exit_code < 0 {
                return Err(anyhow!("Program failed with system error code: {}", exit_code));
            }
        }
        Err(llvm_error) => {
            // LLVM compilation failed - use JIT execution
            if verbose {
                println!("⚠ LLVM tools not available, using JIT execution instead");
                println!("Phase 5: Executing via JIT...");
                println!("Program Output:");
                println!("==================");
            }

            let jit_result = execute_program_jit(&ast)
                .with_context(|| format!("JIT execution failed: {}", llvm_error))?;

            // Display JIT output
            for line in &jit_result.output {
                println!("{}", line);
            }

            if verbose {
                println!("==================");
                println!("Program completed via JIT (exit code: {})", jit_result.exit_code);
            } else {
                println!("Program exited with code: {}", jit_result.exit_code);
            }

            // Only treat negative exit codes as errors for JIT as well
            if jit_result.exit_code < 0 {
                return Err(anyhow!("Program failed with system error code: {}", jit_result.exit_code));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_run_simple_program() {
        // Create a temporary source file
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("test.nr");

        let source_code = r#"
fn main() -> int {
    return 0;
}
"#;

        fs::write(&source_file, source_code).unwrap();

        // Test running
        let result = run_run(source_file, 0, false);

        // Should succeed for basic programs
        assert!(result.is_ok(), "Run should succeed for valid programs: {:?}", result.err());
    }
}