//! Build command implementation

use std::path::PathBuf;
use std::fs;
use anyhow::{Context, Result};
use lexical_analysis::Lexer;
use syntax_parsing::Parser;
use semantic_analysis::analyze_program;
use llvm_backend::compile_to_executable;

/// Build a NEURO program to executable
pub fn run_build(
    file: PathBuf,
    output: Option<PathBuf>,
    opt_level: u8,
    debug: bool,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("🔨 Building executable from: {}", file.display());
        println!("   Optimization level: O{}", opt_level);
        println!("   Debug info: {}", debug);
    }

    // Read source file
    if verbose {
        println!("📖 Reading source file...");
    }
    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read source file: {}", file.display()))?;

    if verbose {
        println!("   Source size: {} bytes", source.len());
    }

    // Lexical analysis
    if verbose {
        println!("🔍 Phase 1: Lexical analysis...");
    }
    let mut lexer = Lexer::new(&source);
    let tokens = lexer.tokenize()
        .with_context(|| "Lexical analysis failed - check for syntax errors in your source code")?;

    if verbose {
        println!("   Generated {} tokens", tokens.len());
    }

    // Syntax parsing
    if verbose {
        println!("🌳 Phase 2: Syntax parsing...");
    }
    let mut parser = Parser::new(tokens);
    let ast = parser.parse()
        .with_context(|| "Syntax parsing failed - check for grammatical errors in your source code")?;

    if verbose {
        println!("   Parsed {} items", ast.items.len());
    }

    // Semantic analysis
    if verbose {
        println!("🔬 Phase 3: Semantic analysis...");
    }
    let _semantic_info = analyze_program(&ast)
        .with_context(|| "Semantic analysis failed - check for type errors and undefined variables")?;

    if verbose {
        println!("   ✓ Semantic analysis passed");
    }

    // Determine output path
    let output_path = output.unwrap_or_else(|| {
        let mut default_name = file.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output")
            .to_string();

        // Add .exe extension on Windows
        if cfg!(windows) {
            default_name.push_str(".exe");
        }
        PathBuf::from(default_name)
    });

    // Extract module name from file path
    let module_name = file.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("main");

    // Build executable
    if verbose {
        println!("⚙️  Phase 4: Generating executable...");
        println!("   Module name: {}", module_name);
        println!("   Output path: {}", output_path.display());
    }

    let executable_path = compile_to_executable(&ast, module_name, &output_path)
        .with_context(|| {
            format!(
                "Executable generation failed. Common causes:\n\
                   • Missing LLVM tools (ensure llc.exe is in PATH)\n\
                   • Missing linker (ensure clang/gcc is available)\n\
                   • Undefined function references (e.g., missing function declarations)\n\
                   • Target path: {}",
                output_path.display()
            )
        })?;

    if verbose {
        println!("   ✓ Executable generated successfully");
    }

    // Print build summary
    println!("🎉 Successfully built executable: {}", executable_path.display());
    if verbose {
        println!("📊 Build Summary:");
        println!("   📁 Input: {}", file.display());
        println!("   🎯 Output: {}", executable_path.display());
        println!("   ⚡ Optimization: O{}", opt_level);
        println!("   🐛 Debug info: {}", debug);

        // Check if the executable exists and get size
        if let Ok(metadata) = fs::metadata(&executable_path) {
            println!("   📏 Executable size: {} bytes", metadata.len());
        }

        println!("\n💡 To run your program: {}", executable_path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_build_simple_program() {
        // Create a temporary source file
        let temp_dir = tempdir().unwrap();
        let source_file = temp_dir.path().join("test.nr");

        let source_code = r#"
fn main() -> int {
    return 42;
}
"#;

        fs::write(&source_file, source_code).unwrap();

        // Test building
        let result = run_build(source_file, None, 0, false, false);

        // Should succeed for basic programs
        assert!(result.is_ok(), "Build should succeed for valid programs: {:?}", result.err());
    }
}