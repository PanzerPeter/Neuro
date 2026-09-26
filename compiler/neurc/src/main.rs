use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use llvm_backend::OptimizationLevelSetting;
use shared_types::Span;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

/// The entry point every compiled executable must define.
const MAIN_FUNCTION: &str = "main";

mod prelude;

#[derive(Parser)]
#[command(name = "neurc")]
#[command(about = "Neuro Programming Language Compiler", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// What `compile` writes to the output path.
///
/// `Obj` stops the pipeline one step before the linker so the object can be linked by
/// something other than a C runtime startup — a shared library a foreign consumer loads,
/// which is what the DLPack differential harness needs to reach a tensor-returning
/// function. It therefore carries no entry-point requirement: a library has no `main`.
///
/// `LlvmIr` stops one step earlier still, at the textual module, for a consumer that
/// rewrites the IR before it becomes machine code. It carries no entry-point requirement
/// for the same reason `Obj` does not.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum EmitKind {
    /// A native executable, linked through the platform C compiler
    Exe,
    /// An unlinked object file
    Obj,
    /// Textual LLVM IR
    LlvmIr,
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

        /// Artifact to write
        #[arg(long, value_name = "KIND", default_value = "exe")]
        emit: EmitKind,
    },

    /// Compile a Neuro source file and run it immediately
    Run {
        /// Input source file
        #[arg(value_name = "FILE")]
        input: PathBuf,

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
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Compile {
            input,
            output,
            optimization,
            emit,
        } => match compile_file(&input, output.as_deref(), optimization, emit) {
            Ok(output_path) => {
                println!(
                    "Successfully compiled {} -> {}",
                    input.display(),
                    output_path.display()
                );
            }
            Err(e) => report_failure("Compilation failed", &e),
        },

        // `run` forwards the program's own exit code, so a Neuro program's status is
        // what the shell sees; a compiler or linker failure is the driver's own 1.
        Commands::Run {
            input,
            optimization,
        } => match run_file(&input, optimization) {
            Ok(code) => process::exit(code),
            Err(e) => report_failure("Run failed", &e),
        },

        Commands::Check { input } => {
            if let Err(e) = check_file(&input) {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
    }
}

/// Print a failed pipeline's whole error chain to stderr and exit non-zero.
///
/// The chain is printed rather than the root alone because the root names the stage
/// that failed and the causes name what it was doing: "Failed to link object file"
/// without its cause tells the user nothing they can act on.
fn report_failure(prefix: &str, error: &anyhow::Error) -> ! {
    eprintln!("{}: {}", prefix, error);
    let mut chain = error.chain();
    chain.next(); // The root is already printed above.
    for (i, cause) in chain.enumerate() {
        eprintln!("  Caused by ({}): {}", i + 1, cause);
    }
    process::exit(1);
}

/// Compile `input` into a temporary directory, run the result, and return its exit code.
///
/// The executable is never written beside the source: a `run` leaves no artifact behind,
/// which is what separates it from `compile` followed by an invocation. The temporary
/// directory is removed when this function returns, after the child has exited.
fn run_file(input: &Path, optimization: u8) -> Result<i32> {
    let dir = tempfile::tempdir().context("Failed to create temporary directory")?;

    // Keep the source's own name so the program sees a meaningful argv[0] and a crash
    // reports something other than an anonymous temporary.
    let stem = input.file_stem().unwrap_or_else(|| OsStr::new("program"));
    let mut executable = dir.path().join(stem);
    if cfg!(target_os = "windows") {
        executable.set_extension("exe");
    }

    compile_file(input, Some(&executable), optimization, EmitKind::Exe)?;

    let status = Command::new(&executable)
        .status()
        .with_context(|| format!("Failed to execute {}", executable.display()))?;

    // A child killed by a signal carries no exit code; 1 keeps that a failure rather
    // than reporting the run as a success.
    Ok(status.code().unwrap_or(1))
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

/// One program, ready for the type checker: every module's items merged, with the prelude
/// in front of them.
struct LoadedProgram {
    items: Vec<syntax_parsing::Item>,
    module_count: usize,
}

/// Expand `input` and every module it reaches into one program, and give it the prelude.
///
/// The parser and the prelude's variant names are passed in rather than imported by
/// `module-resolution`: that slice depends only on the AST it rewrites, so the driver is
/// the single place it, the parser, and the prelude source meet.
fn load_program(input: &Path) -> Result<LoadedProgram> {
    let prelude = prelude::load()?;

    // The resolver's own message names the file, the module, and what was expected, so it
    // is reported as-is rather than wrapped in a context line that would hide it behind
    // `check`'s single-line error rendering.
    let program = module_resolution::resolve_program(
        input,
        &|source| syntax_parsing::parse(source).map_err(|e| e.to_string()),
        prelude.variants(),
    )
    .map_err(|e| anyhow::anyhow!("Module error: {}", e))?;

    let module_count = program.modules.len();
    // The merged namespace is flat, so the prelude's declarations are either in the program
    // or absent from all of it; `@no_prelude` on the root file is what decides.
    let mut items = if program.no_prelude {
        program.items
    } else {
        prelude.prepend(program.items)
    };

    // Named arguments are resolved against the whole program, so this runs only once the
    // prelude and every module are in one list, and before type checking, which is what
    // lets every later pass see an ordinary positional call.
    argument_binding::bind_arguments(&mut items).map_err(|errors| {
        eprintln!("Argument errors found:");
        for (i, error) in errors.iter().enumerate() {
            eprintln!("  {}. {}", i + 1, error);
        }
        anyhow::anyhow!("{} argument error(s) found", errors.len())
    })?;

    Ok(LoadedProgram {
        items,
        module_count,
    })
}

/// Render one diagnostic with its source location: the message, the file, the line
/// and column, the offending source line, and a caret under the span.
///
/// `source` is `None` when the program spans several modules. A span then indexes
/// the text of whichever module raised the error, not the root file's, so resolving
/// it here would point confidently at the wrong line; the message is printed alone.
///
/// The column counts characters, not bytes: a caret placed at a byte column drifts
/// away from the text it is meant to underline as soon as the line holds a
/// multi-byte character.
fn render_diagnostic(path: &Path, source: Option<&str>, message: &str, span: Span) -> String {
    render_labeled("error", path, source, message, span)
}

/// Render one labeled span, the body of both an `error:` and its `note:` lines.
fn render_labeled(
    label: &str,
    path: &Path,
    source: Option<&str>,
    message: &str,
    span: Span,
) -> String {
    let bare = || format!("{}: {}", label, message);
    let Some(source) = source else { return bare() };
    if span.start > span.end
        || span.end > source.len()
        || !source.is_char_boundary(span.start)
        || !source.is_char_boundary(span.end)
    {
        return bare();
    }

    let line_start = source[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[span.start..]
        .find('\n')
        .map_or(source.len(), |i| span.start + i);
    let line_no = source[..line_start].matches('\n').count() + 1;
    let column = source[line_start..span.start].chars().count() + 1;
    // A span may run past the end of its first line (a multi-line expression); the
    // caret underlines the part that is on the line being shown, and never nothing.
    let width = source[span.start..span.end.min(line_end)]
        .chars()
        .count()
        .max(1);

    let gutter = " ".repeat(line_no.to_string().len());
    format!(
        "{label}: {message}\n\
         {gutter}--> {path}:{line_no}:{column}\n\
         {gutter} |\n\
         {line_no} | {line}\n\
         {gutter} | {pad}{carets}",
        label = label,
        message = message,
        gutter = gutter,
        path = path.display(),
        line_no = line_no,
        column = column,
        line = source[line_start..line_end].trim_end_matches('\r'),
        pad = " ".repeat(column - 1),
        carets = "^".repeat(width),
    )
}

/// Report type errors against the source they came from.
///
/// `source` is `None` for a multi-module program: see [`render_diagnostic`].
fn report_type_errors(
    path: &Path,
    source: Option<&str>,
    errors: &[semantic_analysis::TypeError],
) -> anyhow::Error {
    eprintln!("Type errors found in {:?}:", path);
    for error in errors {
        eprintln!(
            "{}",
            render_diagnostic(path, source, &error.to_string(), error.span())
        );
        // A use-after-move carries a second location: where the value went. It is the
        // half of that diagnostic a reader cannot find on their own.
        if let semantic_analysis::TypeError::UseOfMovedValue { moved_at, .. } = error {
            eprintln!(
                "{}",
                render_labeled("note", path, source, "moved here", *moved_at)
            );
        }
        eprintln!();
    }
    anyhow::anyhow!("{} type error(s) found", errors.len())
}

/// Render a lowering failure. The derivative transform's refusal is a user-facing
/// diagnostic with a location; every other variant is a checker escape and has none.
fn report_lowering_error(
    path: &Path,
    source: Option<&str>,
    error: &hir_lowering::LoweringError,
) -> anyhow::Error {
    let hir_lowering::LoweringError::NotDifferentiable { span, .. } = error else {
        return anyhow::anyhow!("HIR lowering error: {}", error);
    };
    eprintln!(
        "{}",
        render_diagnostic(path, source, &error.to_string(), *span)
    );
    anyhow::anyhow!("`@grad` function could not be differentiated")
}

/// Check a Neuro source file for syntax and type errors
fn check_file(path: &PathBuf) -> anyhow::Result<()> {
    validate_source_file(path)?;

    let LoadedProgram {
        items: ast,
        module_count,
    } = load_program(path)?;

    match semantic_analysis::type_check(&ast) {
        Ok(warnings) => {
            print_warnings(&warnings);
            // Lower the type-checked AST to typed HIR (Phase 1.8). The result is the
            // backend-agnostic contract every backend will consume; building it here
            // exercises the lowering end-to-end on every checked program.
            let hir = hir_lowering::lower_program(&ast).map_err(|error| {
                report_lowering_error(
                    path,
                    single_module_source(path, module_count).as_deref(),
                    &error,
                )
            })?;
            println!(
                "Type checking passed for {:?} ({} module(s), {} HIR items)",
                path,
                module_count,
                hir.items.len()
            );
            Ok(())
        }
        Err(errors) => Err(report_type_errors(
            path,
            single_module_source(path, module_count).as_deref(),
            &errors,
        )),
    }
}

/// The source text to resolve diagnostics against, or `None` when the program has
/// more than one module or the file cannot be re-read.
fn single_module_source(path: &Path, module_count: usize) -> Option<String> {
    (module_count == 1).then(|| fs::read_to_string(path).ok())?
}

/// Render lint warnings to stderr. Warnings never block compilation; they are
/// informational guidance for the author.
fn print_warnings(warnings: &[semantic_analysis::Warning]) {
    for warning in warnings {
        eprintln!("{}", warning);
    }
}

/// Compile a Neuro source file to a native executable, an unlinked object file, or
/// textual LLVM IR.
///
/// Pipeline: read source → parse → type-check → lower to HIR → LLVM IR → object
/// code → link. `emit` decides where it stops. For an executable `output` defaults to
/// the input name without its extension (plus `.exe` on Windows); for an object it
/// defaults to the input name with the platform object extension, and for IR to the
/// input name with `.ll`. Returns the path it wrote, which `run` needs and `compile`
/// reports.
fn compile_file(
    input: &Path,
    output: Option<&Path>,
    optimization: u8,
    emit: EmitKind,
) -> Result<PathBuf> {
    validate_source_file(input)?;

    let source = fs::read_to_string(input)
        .context(format!("Failed to read source file: {}", input.display()))?;

    log::info!("Compiling {}", input.display());
    log::info!("Using optimization level -O{}", optimization);

    log::debug!("Resolving modules and parsing...");
    let LoadedProgram {
        items: ast,
        module_count,
    } = load_program(input)?;
    log::debug!("Resolved {} module(s)", module_count);

    log::debug!("Type checking...");
    let warnings = semantic_analysis::type_check(&ast)
        .map_err(|errors| {
            let rendered = (module_count == 1).then_some(source.as_str());
            report_type_errors(input, rendered, &errors)
        })
        .context("Type checking failed")?;
    print_warnings(&warnings);

    // Lower to typed HIR (Phase 1.8). The LLVM backend consumes this HIR directly:
    // every node carries its resolved type, so the backend no longer re-derives types
    // from the AST.
    log::debug!("Lowering to typed HIR...");
    let hir = hir_lowering::lower_program(&ast)
        .map_err(|error| {
            let rendered = (module_count == 1).then_some(source.as_str());
            report_lowering_error(input, rendered, &error)
        })
        .context("Failed to lower to HIR")?;
    log::debug!("Lowered {} HIR items", hir.items.len());

    // An executable needs an entry point. Without this the pipeline runs to
    // completion and the failure surfaces as the system linker's `undefined
    // reference to 'main'`, which names the C runtime rather than the program.
    // An object file is not linked here and may well be a library, so it is exempt.
    if emit == EmitKind::Exe
        && !hir
            .items
            .iter()
            .any(|item| matches!(item, neuro_hir::HirItem::Function(f) if f.name == MAIN_FUNCTION))
    {
        anyhow::bail!(
            "no `{}` function found in {}: an executable needs an entry point",
            MAIN_FUNCTION,
            input.display()
        );
    }

    log::debug!("Generating LLVM IR and object code...");
    let optimization =
        OptimizationLevelSetting::from_u8(optimization).context("Invalid optimization level")?;

    if emit == EmitKind::LlvmIr {
        let ir =
            llvm_backend::compile_to_ir(&hir, optimization, &source, &input.display().to_string())
                .map_err(|e| anyhow::anyhow!("Code generation error: {}", e))
                .context("Failed to generate LLVM IR")?;
        let output_path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input.with_extension("ll"));
        fs::write(&output_path, ir)
            .with_context(|| format!("Failed to write LLVM IR file {}", output_path.display()))?;
        return Ok(output_path);
    }

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

    if emit == EmitKind::Obj {
        let output_path = output
            .map(Path::to_path_buf)
            .unwrap_or_else(|| input.with_extension(object_extension));
        fs::write(&output_path, &object_code)
            .with_context(|| format!("Failed to write object file {}", output_path.display()))?;
        return Ok(output_path);
    }

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

    Ok(output_path)
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

/// Record why one linker did not produce the executable, for the error the last one raises.
///
/// Every attempt's diagnosis is kept rather than logged and dropped. A driver that is simply
/// absent is a different failure from one that ran and could not resolve a symbol, and only
/// the last driver's message used to survive: an unresolved symbol in the object file was
/// reported as a missing Visual Studio, since the earlier drivers had already rejected it
/// for the real reason.
#[cfg(target_os = "windows")]
fn record_attempt(
    attempts: &mut Vec<String>,
    driver: &str,
    result: std::io::Result<std::process::Output>,
) {
    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!("{driver} linking failed");
            log::debug!("  stdout: {stdout}");
            log::debug!("  stderr: {stderr}");
            attempts.push(format!(
                "{driver}: exited with {}\n{}{}",
                output.status,
                stdout.trim_end(),
                stderr.trim_end()
            ));
        }
        Err(e) => {
            log::debug!("{driver} not available: {e}");
            attempts.push(format!("{driver}: not available ({e})"));
        }
    }
}

#[cfg(target_os = "windows")]
fn link_windows(object_path: &Path, output_path: &Path) -> Result<()> {
    let mut attempts: Vec<String> = Vec::new();

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
        other => record_attempt(&mut attempts, "clang", other),
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
        other => record_attempt(&mut attempts, "lld-link", other),
    }

    // Fall back to MSVC: cl.exe acts as a linker driver and locates the real
    // link.exe (not Git's `link` utility).
    log::debug!("Attempting to link with MSVC link.exe via vcvarsall.bat");

    let msvc_result = Command::new("cl")
        .arg("/nologo")
        .arg(object_path)
        .arg(format!("/Fe{}", output_path.display())) // /Fe takes no colon or space
        .arg("/link") // subsequent args go to the linker
        .arg("/SUBSYSTEM:CONSOLE")
        .arg("/ENTRY:main")
        .output();

    match msvc_result {
        Ok(output) if output.status.success() => {
            log::info!("Successfully linked with MSVC: {}", output_path.display());
            return Ok(());
        }
        other => record_attempt(&mut attempts, "cl.exe (MSVC)", other),
    }

    Err(anyhow::anyhow!(
        "No linker produced an executable. Each driver was tried in turn:\n\n{}\n\nNote: if every driver is reported as not available, install LLVM or Visual Studio; \
         a driver that ran and failed reports the real reason above.",
        attempts.join("\n\n")
    ))
    .context(format!(
        "Failed to link object file {} to executable {}",
        object_path.display(),
        output_path.display()
    ))
}

#[cfg(not(target_os = "windows"))]
fn link_unix(object_path: &Path, output_path: &Path) -> Result<()> {
    // cc (gcc or clang) acts as the linker driver. `-lm` is explicit because
    // `Tensor::random_normal` and the elementwise math methods emit `log`, `exp`, `tanh`,
    // `pow` and `cos`, and the C math library is a separate archive on the older glibc
    // still in wide use; it is a no-op where the platform has already folded libm into libc.
    let output = Command::new("cc")
        .arg(object_path)
        .arg("-o")
        .arg(output_path)
        .arg("-lm")
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
