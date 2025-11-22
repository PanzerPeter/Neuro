// NEURO Programming Language - Compiler Driver
// Main entry point for the neurc compiler

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use tempfile::NamedTempFile;

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

/// Check a NEURO source file for syntax and type errors
fn check_file(path: &PathBuf) -> anyhow::Result<()> {
    // Read source file
    let source = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", path, e))?;

    // Parse the source code
    let ast = syntax_parsing::parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

    // Type check the program
    match semantic_analysis::type_check(&ast) {
        Ok(()) => {
            println!("✓ Type checking passed for {:?}", path);
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
    log::debug!("Writing object file...");
    let mut object_file = NamedTempFile::new().context("Failed to create temporary object file")?;

    object_file
        .write_all(&object_code)
        .context("Failed to write object code to temporary file")?;

    // Ensure data is flushed to disk
    object_file.flush().context("Failed to flush object file")?;

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
    link_object_to_executable(object_file.path(), &output_path)
        .context("Failed to link object file to executable")?;

    println!(
        "Successfully compiled {} -> {}",
        input.display(),
        output_path.display()
    );

    Ok(())
}

/// Link an object file to a native executable using the system linker.
///
/// This function uses the `cc` crate to invoke the platform's C compiler/linker,
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
/// - On Windows: Uses MSVC link.exe or MinGW ld (via cc crate detection)
/// - On Unix: Uses ld or clang (via cc crate detection)
/// - Automatically links against C runtime for startup code
fn link_object_to_executable(object_path: &Path, output_path: &Path) -> Result<()> {
    // Use cc crate to invoke the system linker
    // This automatically handles platform-specific details:
    // - Locating the linker (link.exe on Windows, ld on Unix)
    // - Linking against C runtime
    // - Handling startup code (_start, mainCRTStartup, etc.)
    cc::Build::new()
        .object(object_path)
        .try_compile(&output_path.to_string_lossy())
        .context(format!(
            "Failed to link object file {} to executable {}",
            object_path.display(),
            output_path.display()
        ))?;

    Ok(())
}
