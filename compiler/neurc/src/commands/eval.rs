//! Eval command implementation

use crate::{driver::CompilerDriver, output};
use anyhow::Result;

pub fn run_eval(input: String, is_file: bool, verbose: bool) -> Result<()> {
    let driver = CompilerDriver::new(verbose);
    
    match driver.evaluate(&input, is_file) {
        Ok(result) => {
            println!("⚙️ Evaluation result:");
            println!("{}", result);
            Ok(())
        },
        Err(error) => {
            eprintln!("{}", output::format_error(&error));
            Err(error)
        }
    }
}