// NEURO Programming Language - Compiler Driver
// Main entry point for the neurc compiler

use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::process;

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
            println!("Compiling: {:?}", input);
            println!(
                "Output: {:?}",
                output.unwrap_or_else(|| PathBuf::from("a.out"))
            );
            println!("Optimization level: {}", optimization);

            // Phase 1: Stub implementation
            println!("NEURO compiler is in Phase 1 development");
            println!("Full compilation not yet implemented");
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
