//! NEURO Compiler CLI Entry Point

use clap::Parser;
use neurc::{
    cli::{Cli, Commands},
    commands::*,
};
use std::process;

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Commands::Compile { files, opt_level } => {
            run_compile(files, opt_level, cli.verbose)
        },
        Commands::Check { files } => {
            run_check(files, cli.verbose)
        },
        Commands::Tokenize { file, format } => {
            run_tokenize(file, format, cli.verbose)
        },
        Commands::Parse { file, format } => {
            run_parse(file, format, cli.verbose)
        },
        Commands::Eval { input, file } => {
            run_eval(input, file, cli.verbose)
        },
        Commands::Analyze { file, format } => {
            run_analyze(file, &format, cli.verbose)
        },
        Commands::Llvm { file, opt_level, output } => {
            run_llvm(file, opt_level, output, cli.verbose)
        },
        Commands::Version => {
            run_version()
        },
    };
    
    if let Err(error) = result {
        eprintln!("Error: {}", error);
        process::exit(1);
    }
}
