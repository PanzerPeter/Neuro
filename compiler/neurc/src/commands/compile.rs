//! Compile command implementation

use crate::{driver::CompilerDriver, output};
use anyhow::Result;
use std::path::PathBuf;

pub fn run_compile(files: Vec<PathBuf>, _opt_level: u8, verbose: bool) -> Result<()> {
    let mut driver = CompilerDriver::new(verbose);
    
    let file_count = files.len();
    for file in &files {
        println!("Compiling: {}", file.display());
        
        match driver.compile_file(&file) {
            Ok(result) => {
                if verbose {
                    println!(" Successfully compiled {}", file.display());
                    println!("  - {} tokens", result.tokens.len());
                    println!("  - {} AST items", result.ast.items.len());
                } else {
                    println!("{}", output::format_success(
                        &format!("Successfully compiled {}", file.display())
                    ));
                }
            },
            Err(error) => {
                eprintln!("{}", output::format_error(&error));
                return Err(error);
            }
        }
    }
    
    if file_count > 1 {
        println!("{}", output::format_success(
            &format!("Successfully compiled {} files", file_count)
        ));
    }
    
    Ok(())
}