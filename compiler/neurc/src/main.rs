// NEURO Programming Language - Compiler Driver
// Main entry point for the neurc compiler

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

#[derive(Parser)]
#[command(name = "neurc")]
#[command(about = "NEURO Programming Language Compiler", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile NEURO source files
    Compile {
        /// Input source file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Output file path
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "0")]
        optimization: u8,
    },

    /// Check syntax and types without generating code
    Check {
        /// Input source file
        #[arg(value_name = "FILE")]
        input: PathBuf,
    },

    /// Display version information
    Version,
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            optimization,
        } => {
            // Note: optimization level support deferred to Phase 1.5
            // Currently always uses -O0 (no optimization)
            if optimization != 0 {
                log::warn!(
                    "Optimization level -O{} requested, but only -O0 is supported in Phase 1",
                    optimization
                );
                log::warn!("Compiling with -O0 (no optimization)");
            }

            if let Err(e) = compile_file(&input, output.as_deref()) {
                eprintln!("Compilation failed: {}", e);

                // Print error chain for detailed context
                let mut chain = e.chain();
                chain.next(); // Skip the root error (already printed)
                for (i, cause) in chain.enumerate() {
                    eprintln!("  Caused by ({}): {}", i + 1, cause);
                }

                process::exit(1);
            }
        }

        Commands::Check { input } => {
            if let Err(e) = check_file(&input) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }

        Commands::Version => {
            println!("neurc {}", env!("CARGO_PKG_VERSION"));
            println!("NEURO Programming Language Compiler");
            println!("Phase 1 - Alpha Development");
        }
    }
}

/// Validate that a file has the .nr extension
fn validate_source_file(path: &Path) -> Result<()> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("nr") => Ok(()),
        Some(other) => Err(anyhow::anyhow!(
            "Invalid file extension '.{}'. NEURO source files must have .nr extension",
            other
        )),
        None => Err(anyhow::anyhow!(
            "File has no extension. NEURO source files must have .nr extension"
        )),
    }
}

/// Check a NEURO source file for syntax and type errors
fn check_file(path: &PathBuf) -> anyhow::Result<()> {
    // Validate file extension
    validate_source_file(path)?;

    // Read source file
    let source = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", path, e))?;

    // Parse the source code
    let ast = syntax_parsing::parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

    // Type check the program
    match semantic_analysis::type_check(&ast) {
        Ok(()) => {
            println!("Type checking passed for {:?}", path);
            Ok(())
        }
        Err(errors) => {
            eprintln!("Type errors found in {:?}:", path);
            for (i, error) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, error);
            }
            Err(anyhow::anyhow!("{} type error(s) found", errors.len()))
        }
    }
}

/// Compile a NEURO source file to a native executable.
///
/// This function orchestrates the complete compilation pipeline:
/// 1. Read source file
/// 2. Lexical analysis and parsing
/// 3. Semantic analysis (type checking)
/// 4. LLVM code generation (object code)
/// 5. Linking to create executable
///
/// # Arguments
///
/// * `input` - Path to the input .nr source file
/// * `output` - Optional path for the output executable (defaults to input name without extension)
///
/// # Returns
///
/// * `Ok(())` - Compilation succeeded, executable created
/// * `Err(anyhow::Error)` - Compilation failed with detailed error context
///
/// # Examples
///
/// ```ignore
/// compile_file(Path::new("program.nr"), None)?;
/// // Creates "program" (or "program.exe" on Windows)
/// ```
fn compile_file(input: &Path, output: Option<&Path>) -> Result<()> {
    // Validate file extension
    validate_source_file(input)?;

    // Read source file
    let source = fs::read_to_string(input)
        .context(format!("Failed to read source file: {}", input.display()))?;

    log::info!("Compiling {}", input.display());

    // Parse the source code
    log::debug!("Parsing source...");
    let ast = syntax_parsing::parse(&source)
        .map_err(|e| anyhow::anyhow!("Parse error: {}", e))
        .context("Failed to parse source file")?;

    // Type check the program
    log::debug!("Type checking...");
    semantic_analysis::type_check(&ast)
        .map_err(|errors| {
            eprintln!("Type errors found:");
            for (i, error) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, error);
            }
            anyhow::anyhow!("{} type error(s) found", errors.len())
        })
        .context("Type checking failed")?;

    // Generate LLVM object code
    log::debug!("Generating LLVM IR and object code...");
    let object_code = llvm_backend::compile(&ast)
        .map_err(|e| anyhow::anyhow!("Code generation error: {}", e))
        .context("Failed to generate object code")?;

    // Write object code to temporary file
    // On Windows, MSVC expects .obj extension; on Unix, .o is conventional
    log::debug!("Writing object file...");
    let object_extension = if cfg!(target_os = "windows") {
        "obj"
    } else {
        "o"
    };

    let mut object_file = tempfile::Builder::new()
        .suffix(&format!(".{}", object_extension))
        .tempfile()
        .context("Failed to create temporary object file")?;

    object_file
        .write_all(&object_code)
        .context("Failed to write object code to temporary file")?;

    // Ensure data is flushed to disk
    object_file.flush().context("Failed to flush object file")?;

    // Persist the tempfile to prevent early deletion
    // The linker needs the file to exist for the duration of the linking process
    let (_, object_path) = object_file
        .keep()
        .context("Failed to persist temporary object file")?;

    // Determine output executable path
    let output_path = if let Some(out) = output {
        out.to_path_buf()
    } else {
        // Default: same name as input file, without extension
        let mut default_output = input.with_extension("");
        // On Windows, add .exe extension
        if cfg!(target_os = "windows") {
            default_output.set_extension("exe");
        }
        default_output
    };

    // Link object file to create executable
    log::debug!("Linking to create executable: {}", output_path.display());
    link_object_to_executable(&object_path, &output_path)
        .context("Failed to link object file to executable")?;

    // Clean up the temporary object file
    let _ = fs::remove_file(&object_path);

    println!(
        "Successfully compiled {} -> {}",
        input.display(),
        output_path.display()
    );

    Ok(())
}

/// Link an object file to a native executable using the system linker.
///
/// This function invokes the platform's C compiler as a linker driver,
/// which handles platform-specific linking requirements (C runtime, startup code, etc.).
///
/// # Arguments
///
/// * `object_path` - Path to the input object file (.o or .obj)
/// * `output_path` - Path for the output executable
///
/// # Returns
///
/// * `Ok(())` - Linking succeeded
/// * `Err(anyhow::Error)` - Linking failed
///
/// # Implementation Notes
///
/// - On Windows: Tries clang first (part of LLVM installation), then falls back to MSVC cl.exe
/// - On Unix: Uses cc (gcc/clang) as linker driver
/// - Automatically links against C runtime for startup code
fn link_object_to_executable(object_path: &Path, output_path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        link_windows(object_path, output_path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        link_unix(object_path, output_path)
    }
}

#[cfg(target_os = "windows")]
fn link_windows(object_path: &Path, output_path: &Path) -> Result<()> {
    // Try linking with clang first (it's available from LLVM installation)
    // Clang acts as a linker driver and handles all the details
    log::debug!("Attempting to link with clang");
    let clang_result = Command::new("clang")
        .arg(object_path)
        .arg("-o")
        .arg(output_path)
        .arg("-Wl,/subsystem:console") // Pass subsystem flag to linker
        .output();

    match clang_result {
        Ok(output) if output.status.success() => {
            log::info!("Successfully linked with clang: {}", output_path.display());
            return Ok(());
        }
        Ok(output) => {
            log::debug!("Clang linking failed");
            log::debug!("  stdout: {}", String::from_utf8_lossy(&output.stdout));
            log::debug!("  stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => {
            log::debug!("Clang not available: {}", e);
        }
    }

    // Try lld-link (LLVM's linker for Windows)
    log::debug!("Attempting to link with lld-link");
    let lld_result = Command::new("lld-link")
        .arg(format!("/OUT:{}", output_path.display()))
        .arg("/SUBSYSTEM:CONSOLE")
        .arg("/ENTRY:main")
        .arg(object_path)
        .output();

    match lld_result {
        Ok(output) if output.status.success() => {
            log::info!(
                "Successfully linked with lld-link: {}",
                output_path.display()
            );
            return Ok(());
        }
        Ok(output) => {
            log::debug!("lld-link linking failed");
            log::debug!("  stdout: {}", String::from_utf8_lossy(&output.stdout));
            log::debug!("  stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
        Err(e) => {
            log::debug!("lld-link not available: {}", e);
        }
    }

    // Fall back to MSVC link.exe
    // Note: We need to find the real MSVC link.exe, not Git's link utility
    log::debug!("Attempting to link with MSVC link.exe via vcvarsall.bat");

    // Try using cl.exe as a linker driver (it will find the right link.exe)
    let output = Command::new("cl")
        .arg("/nologo") // Suppress startup banner
        .arg(object_path) // Input object file
        .arg(format!("/Fe{}", output_path.display())) // Output executable (no colon, no space)
        .arg("/link") // Following args are for the linker
        .arg("/SUBSYSTEM:CONSOLE")
        .arg("/ENTRY:main")
        .output()
        .context("Failed to execute MSVC cl.exe - ensure Visual Studio is installed and vcvarsall.bat has been run")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!(
            "MSVC linking failed:\nstdout: {}\nstderr: {}\n\nNote: Ensure you have Visual Studio installed and are running from a Developer Command Prompt, or run vcvarsall.bat",
            stdout,
            stderr
        ))
        .context(format!(
            "Failed to link object file {} to executable {}",
            object_path.display(),
            output_path.display()
        ));
    }

    log::info!("Successfully linked with MSVC: {}", output_path.display());
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn link_unix(object_path: &Path, output_path: &Path) -> Result<()> {
    // On Unix, use cc (which is usually gcc or clang)
    // The cc command acts as a linker driver
    let output = Command::new("cc")
        .arg(object_path)
        .arg("-o")
        .arg(output_path)
        .output()
        .context("Failed to execute cc - ensure a C compiler (gcc/clang) is installed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(anyhow::anyhow!(
            "Linking failed:\nstdout: {}\nstderr: {}",
            stdout,
            stderr
        ))
        .context(format!(
            "Failed to link object file {} to executable {}",
            object_path.display(),
            output_path.display()
        ));
    }

    log::info!("Successfully linked with cc: {}", output_path.display());
    Ok(())
}
