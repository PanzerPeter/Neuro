//! Command-line interface for the NEURO compiler
//! 
//! This module defines the CLI structure and argument parsing for neurc.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// NEURO Programming Language Compiler
#[derive(Parser)]
#[command(name = "neurc")]
#[command(about = "A compiler for the NEURO programming language")]
#[command(version = "0.1.0")]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,

    /// Output file path
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// The subcommand to run
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Compile NEURO source files
    Compile {
        /// Input source file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "0")]
        opt_level: u8,
    },
    /// Check syntax and semantics without compilation
    Check {
        /// Input source file(s)
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Tokenize source files and output tokens
    Tokenize {
        /// Input source file
        file: PathBuf,
        /// Output format (json, pretty)
        #[arg(long, default_value = "pretty")]
        format: String,
    },
    /// Parse source files and output AST
    Parse {
        /// Input source file
        file: PathBuf,
        /// Output format (json, pretty)
        #[arg(long, default_value = "pretty")]
        format: String,
    },
    /// Evaluate simple expressions
    Eval {
        /// Input source file or expression
        input: String,
        /// Treat input as file path instead of expression
        #[arg(short, long)]
        file: bool,
    },
    /// Run semantic analysis and show detailed results
    Analyze {
        /// Input source file
        file: PathBuf,
        /// Output format (json, pretty)
        #[arg(long, default_value = "pretty")]
        format: String,
    },
    /// Generate LLVM IR from source files
    Llvm {
        /// Input source file
        file: PathBuf,
        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "0")]
        opt_level: u8,
        /// Output file for LLVM IR (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Build NEURO source to executable
    Build {
        /// Input source file
        file: PathBuf,
        /// Output executable name
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "2")]
        opt_level: u8,
        /// Include debug information
        #[arg(long)]
        debug: bool,
    },
    /// Run NEURO program directly
    Run {
        /// Input source file
        file: PathBuf,
        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "0")]
        opt_level: u8,
    },
    /// Show version information
    Version,
}