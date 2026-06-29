use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use llvm_backend::OptimizationLevelSetting;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

#[derive(Parser)]
#[command(name = "neurc")]
#[command(about = "Neuro Programming Language Compiler", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile Neuro source files
    Compile {
        /// Input source file
        #[arg(value_name = "FILE")]
        input: PathBuf,

        /// Output file path
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,

        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=3))]
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
            if let Err(e) = compile_file(&input, output.as_deref(), optimization) {
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
            println!("Neuro Programming Language Compiler");
            println!("Phase 1 - Alpha Development");
        }
    }
}

/// Validate that a file has the .nr extension
fn validate_source_file(path: &Path) -> Result<()> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("nr") => Ok(()),
        Some(other) => Err(anyhow::anyhow!(
            "Invalid file extension '.{}'. Neuro source files must have .nr extension",
            other
        )),
        None => Err(anyhow::anyhow!(
            "File has no extension. Neuro source files must have .nr extension"
        )),
    }
}

/// Check a Neuro source file for syntax and type errors
fn check_file(path: &PathBuf) -> anyhow::Result<()> {
    validate_source_file(path)?;

    let source = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", path, e))?;

    let ast = syntax_parsing::parse(&source).map_err(|e| anyhow::anyhow!("Parse error: {}", e))?;

    match semantic_analysis::type_check(&ast) {
        Ok(warnings) => {
            print_warnings(&warnings);
            // Lower the type-checked AST to typed HIR (Phase 1.8). The result is the
            // backend-agnostic contract every backend will consume; building it here
            // exercises the lowering end-to-end on every checked program.
            let hir = hir_lowering::lower_program(&ast)
                .map_err(|e| anyhow::anyhow!("HIR lowering error: {}", e))?;
            println!(
                "Type checking passed for {:?} ({} HIR items)",
                path,
                hir.items.len()
            );
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

/// Render lint warnings to stderr. Warnings never block compilation; they are
/// informational guidance for the author.
fn print_warnings(warnings: &[semantic_analysis::Warning]) {
    for warning in warnings {
        eprintln!("{}", warning);
    }
}

/// Compile a Neuro source file to a native executable.
///
/// Pipeline: read source → parse → type-check → lower to HIR → LLVM object
/// code → link. `output` defaults to the input name without its extension
/// (plus `.exe` on Windows).
fn compile_file(input: &Path, output: Option<&Path>, optimization: u8) -> Result<()> {
    validate_source_file(input)?;

    let source = fs::read_to_string(input)
        .context(format!("Failed to read source file: {}", input.display()))?;

    log::info!("Compiling {}", input.display());
    log::info!("Using optimization level -O{}", optimization);

    log::debug!("Parsing source...");
    let ast = syntax_parsing::parse(&source)
        .map_err(|e| anyhow::anyhow!("Parse error: {}", e))
        .context("Failed to parse source file")?;

    log::debug!("Type checking...");
    let warnings = semantic_analysis::type_check(&ast)
        .map_err(|errors| {
            eprintln!("Type errors found:");
            for (i, error) in errors.iter().enumerate() {
                eprintln!("  {}. {}", i + 1, error);
            }
            anyhow::anyhow!("{} type error(s) found", errors.len())
        })
        .context("Type checking failed")?;
    print_warnings(&warnings);

    // Lower to typed HIR (Phase 1.8). The LLVM backend consumes this HIR directly —
    // every node carries its resolved type, so the backend no longer re-derives types
    // from the AST.
    log::debug!("Lowering to typed HIR...");
    let hir = hir_lowering::lower_program(&ast)
        .map_err(|e| anyhow::anyhow!("HIR lowering error: {}", e))
        .context("Failed to lower to HIR")?;
    log::debug!("Lowered {} HIR items", hir.items.len());

    log::debug!("Generating LLVM IR and object code...");
    let optimization =
        OptimizationLevelSetting::from_u8(optimization).context("Invalid optimization level")?;

    let object_code =
        llvm_backend::compile(&hir, optimization, &source, &input.display().to_string())
            .map_err(|e| anyhow::anyhow!("Code generation error: {}", e))
            .context("Failed to generate object code")?;

    // MSVC expects .obj on Windows; .o is conventional on Unix.
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

    object_file.flush().context("Failed to flush object file")?;

    // Persist past the TempFile guard so the file survives until the linker reads it.
    let (_, object_path) = object_file
        .keep()
        .context("Failed to persist temporary object file")?;

    let output_path = if let Some(out) = output {
        out.to_path_buf()
    } else {
        // Default: input name with the extension stripped (`.exe` on Windows).
        let mut default_output = input.with_extension("");
        if cfg!(target_os = "windows") {
            default_output.set_extension("exe");
        }
        default_output
    };

    log::debug!("Linking to create executable: {}", output_path.display());
    link_object_to_executable(&object_path, &output_path)
        .context("Failed to link object file to executable")?;

    let _ = fs::remove_file(&object_path);

    println!(
        "Successfully compiled {} -> {}",
        input.display(),
        output_path.display()
    );

    Ok(())
}

/// Link an object file to a native executable via the platform's C compiler,
/// which acts as a linker driver (C runtime, startup code, etc.).
///
/// Windows tries clang, then lld-link, then MSVC cl.exe; Unix uses cc.
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
    log::debug!("Attempting to link with clang");
    let clang_result = Command::new("clang")
        .arg(object_path)
        .arg("-o")
        .arg(output_path)
        .arg("-Wl,/subsystem:console")
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

    // Fall back to MSVC: cl.exe acts as a linker driver and locates the real
    // link.exe (not Git's `link` utility).
    log::debug!("Attempting to link with MSVC link.exe via vcvarsall.bat");

    let output = Command::new("cl")
        .arg("/nologo")
        .arg(object_path)
        .arg(format!("/Fe{}", output_path.display())) // /Fe takes no colon or space
        .arg("/link") // subsequent args go to the linker
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
    // cc (gcc or clang) acts as the linker driver.
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
