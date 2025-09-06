//! Compiler driver that orchestrates the compilation pipeline
//!
//! This module integrates lexical analysis, parsing, and module resolution
//! to provide a complete compilation experience.

use anyhow::{Context, Result};
use lexical_analysis::Tokenizer;
use syntax_parsing::{Parser, Evaluator};
use module_system::ModuleSystem;
use semantic_analysis::{SemanticInfo, analyze_program};
use shared_types::{Program, Token};
use std::fs;
use std::path::Path;

/// Main compiler driver that orchestrates the compilation process
pub struct CompilerDriver {
    module_system: ModuleSystem,
    verbose: bool,
}

/// Compilation result containing intermediate representations
#[derive(Debug)]
pub struct CompilationResult {
    pub tokens: Vec<Token>,
    pub ast: Program,
    pub semantic_info: SemanticInfo,
    pub source: String,
    pub file_path: String,
}

impl CompilerDriver {
    /// Create a new compiler driver
    pub fn new(verbose: bool) -> Self {
        Self {
            module_system: ModuleSystem::new(),
            verbose,
        }
    }

    /// Read and process a source file through the complete pipeline
    pub fn compile_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<CompilationResult> {
        let file_path = file_path.as_ref();
        let file_path_str = file_path.to_string_lossy().to_string();
        
        if self.verbose {
            println!("Reading file: {}", file_path_str);
        }

        // Read source file
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path_str))?;

        self.compile_source(source, file_path_str)
    }

    /// Compile source code through the complete pipeline
    pub fn compile_source(&mut self, source: String, file_path: String) -> Result<CompilationResult> {
        if self.verbose {
            println!("Tokenizing source...");
        }

        // Tokenization
        let tokenizer = Tokenizer::with_file(source.clone(), file_path.clone());
        let tokens = tokenizer.tokenize()
            .with_context(|| "Tokenization failed")?;

        if self.verbose {
            println!("Found {} tokens", tokens.len());
            println!("Parsing tokens into AST...");
        }

        // Parsing
        let filtered_tokens = tokenizer.tokenize_filtered()
            .with_context(|| "Token filtering failed")?;
        
        let mut parser = Parser::new(filtered_tokens);
        let ast = parser.parse()
            .with_context(|| "Parsing failed")?;

        if self.verbose {
            println!("AST generated with {} items", ast.items.len());
            println!("Running semantic analysis...");
        }

        // Semantic analysis
        let semantic_info = analyze_program(&ast)
            .with_context(|| "Semantic analysis failed")?;

        if self.verbose {
            if !semantic_info.errors.is_empty() {
                println!("Found {} semantic errors", semantic_info.errors.len());
                for error in &semantic_info.errors {
                    println!("  {}", error);
                }
            } else {
                println!("Semantic analysis completed successfully");
                println!("Found {} symbols", semantic_info.symbols.len());
            }
        }

        // Module registration (basic for now)
        let _module_id = self.module_system.register_module(
            file_path.clone().into(),
            ast.clone()
        );

        Ok(CompilationResult {
            tokens,
            ast,
            semantic_info,
            source,
            file_path,
        })
    }

    /// Just tokenize without parsing
    pub fn tokenize_file<P: AsRef<Path>>(&self, file_path: P) -> Result<(Vec<Token>, String)> {
        let file_path = file_path.as_ref();
        let file_path_str = file_path.to_string_lossy().to_string();
        
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path_str))?;

        let tokenizer = Tokenizer::with_file(source.clone(), file_path_str);
        let tokens = tokenizer.tokenize()
            .with_context(|| "Tokenization failed")?;

        Ok((tokens, source))
    }

    /// Tokenize and parse but don't process modules
    pub fn parse_file<P: AsRef<Path>>(&self, file_path: P) -> Result<(Program, Vec<Token>, String)> {
        let file_path = file_path.as_ref();
        let file_path_str = file_path.to_string_lossy().to_string();
        
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path_str))?;

        let tokenizer = Tokenizer::with_file(source.clone(), file_path_str);
        let tokens = tokenizer.tokenize()
            .with_context(|| "Tokenization failed")?;

        let filtered_tokens = tokenizer.tokenize_filtered()
            .with_context(|| "Token filtering failed")?;
        
        let mut parser = Parser::new(filtered_tokens);
        let ast = parser.parse()
            .with_context(|| "Parsing failed")?;

        Ok((ast, tokens, source))
    }

    /// Evaluate expressions in a source string or file
    pub fn evaluate(&self, input: &str, is_file: bool) -> Result<String> {
        let source = if is_file {
            fs::read_to_string(input)
                .with_context(|| format!("Failed to read file: {}", input))?
        } else {
            input.to_string()
        };

        // If not a file, try to parse as a single expression first
        if !is_file {
            let tokenizer = Tokenizer::new(source.clone());
            let filtered_tokens = tokenizer.tokenize_filtered()
                .with_context(|| "Tokenization failed")?;
            let mut parser = Parser::new(filtered_tokens);
            
            if let Ok(expr) = parser.parse_expression_only() {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&expr) {
                    Ok(value) => return Ok(format!("{}", value)),
                    Err(err) => return Ok(format!("Evaluation error: {}", err)),
                }
            }
            // If expression parsing failed, fall back to full program parsing
        }
        
        let tokenizer = Tokenizer::new(source);
        let filtered_tokens = tokenizer.tokenize_filtered()
            .with_context(|| "Tokenization failed")?;
        let mut parser = Parser::new(filtered_tokens);
        
        let ast = parser.parse()
            .with_context(|| "Parsing failed")?;

        let _evaluator = Evaluator::new();
        // For now, just evaluate expressions in the AST items
        let mut results = Vec::new();
        for item in &ast.items {
            match item {
                shared_types::Item::Function(func) => {
                    // For now, just indicate that we found a function
                    results.push(format!("Function: {}", func.name));
                }
                shared_types::Item::Import(import) => {
                    results.push(format!("Import: {}", import.path));
                }
                shared_types::Item::Struct(struct_def) => {
                    results.push(format!("Struct: {}", struct_def.name));
                }
            }
        }
        let results = format!("{:?}", results);

        Ok(format!("{:?}", results))
    }
}