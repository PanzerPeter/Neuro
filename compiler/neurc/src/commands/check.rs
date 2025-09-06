//! Check command implementation

use crate::{driver::CompilerDriver, output};
use anyhow::Result;
use std::path::PathBuf;

pub fn run_check(files: Vec<PathBuf>, verbose: bool) -> Result<()> {
    let mut driver = CompilerDriver::new(verbose);
    let mut all_passed = true;
    
    let file_count = files.len();
    for file in &files {
        if verbose {
            println!("Checking: {}", file.display());
        }
        
        match driver.compile_file(&file) {
            Ok(_result) => {
                println!("{}  OK", file.display());
            },
            Err(error) => {
                eprintln!("{} L ERROR", file.display());
                eprintln!("{}", output::format_error(&error));
                all_passed = false;
            }
        }
    }
    
    if all_passed {
        println!("{}", output::format_success(
            &format!("All {} files passed checks", file_count)
        ));
        Ok(())
    } else {
        Err(anyhow::anyhow!("Some files failed checks"))
    }
}