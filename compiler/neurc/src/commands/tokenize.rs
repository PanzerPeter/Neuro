//! Tokenize command implementation

use crate::{driver::CompilerDriver, output::{self, OutputFormat}};
use anyhow::Result;
use std::path::PathBuf;

pub fn run_tokenize(file: PathBuf, format: String, verbose: bool) -> Result<()> {
    let driver = CompilerDriver::new(verbose);
    let output_format = OutputFormat::from_string(&format)
        .map_err(|e| anyhow::anyhow!(e))?;
    
    match driver.tokenize_file(&file) {
        Ok((tokens, _source)) => {
            let formatted_output = output::format_tokens(&tokens, &output_format)?;
            println!("{}", formatted_output);
            Ok(())
        },
        Err(error) => {
            eprintln!("{}", output::format_error(&error));
            Err(error)
        }
    }
}