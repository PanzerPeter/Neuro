// NEURO Programming Language - Compiler Driver
// Main entry point for the neurc compiler

use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
            println!("Checking: {:?}", input);
            println!("NEURO compiler is in Phase 1 development");
            println!("Type checking not yet implemented");
        }

        Commands::Version => {
            println!("neurc {}", env!("CARGO_PKG_VERSION"));
            println!("NEURO Programming Language Compiler");
            println!("Phase 1 - Alpha Development");
        }
    }
}
