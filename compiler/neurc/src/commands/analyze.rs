//! Semantic analysis command implementation

use crate::driver::CompilerDriver;
use crate::output::{format_semantic_analysis, format_error, OutputFormat};
use anyhow::Result;
use std::path::Path;

/// Run semantic analysis on a source file and show detailed results
pub fn run_analyze<P: AsRef<Path>>(
    file_path: P, 
    format: &str, 
    _verbose: bool
) -> Result<()> {
    let format = OutputFormat::from_string(format)
        .map_err(|e| anyhow::anyhow!(e))?;
    
    let mut driver = CompilerDriver::new(false);
    
    match driver.compile_file(&file_path) {
        Ok(result) => {
            // Format and display semantic analysis results
            match format_semantic_analysis(&result.semantic_info, &format) {
                Ok(output) => println!("{}", output),
                Err(err) => eprintln!("{}", format_error(&err.into())),
            }
        }
        Err(error) => {
            eprintln!("{}", format_error(&error));
            std::process::exit(1);
        }
    }
    
    Ok(())
}