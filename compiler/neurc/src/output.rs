//! Output formatting for compiler results
//!
//! This module provides different output formats for tokens, AST, and error reports.

use anyhow::Result;
use shared_types::{Token, Program};
use semantic_analysis::{SemanticInfo, SemanticError};
use serde_json;

/// Available output formats
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Pretty,
    Json,
}

impl OutputFormat {
    pub fn from_string(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(OutputFormat::Pretty),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!("Unknown output format: {}", s)),
        }
    }
}

/// Format tokens for output
pub fn format_tokens(tokens: &[Token], format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Pretty => {
            let mut output = String::new();
            output.push_str(&format!("Tokens ({}):\n", tokens.len()));
            output.push_str(&"-".repeat(50));
            output.push_str("\n");
            
            for (i, token) in tokens.iter().enumerate() {
                let lexeme = get_token_lexeme(&token.token_type);
                output.push_str(&format!(
                    "{:3}: {:15} | {} | {}\n",
                    i,
                    format!("{:?}", token.token_type),
                    lexeme,
                    format_span(&token.span)
                ));
            }
            Ok(output)
        },
        OutputFormat::Json => {
            let json_tokens: Vec<_> = tokens.iter().map(|token| {
                serde_json::json!({
                    "type": format!("{:?}", token.token_type),
                    "lexeme": get_token_lexeme(&token.token_type),
                    "span": {
                        "start": token.span.start,
                        "end": token.span.end
                    }
                })
            }).collect();
            
            Ok(serde_json::to_string_pretty(&json_tokens)?)
        }
    }
}

/// Format AST for output
pub fn format_ast(ast: &Program, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Pretty => {
            let mut output = String::new();
            output.push_str(&format!("Program AST ({} items):\n", ast.items.len()));
            output.push_str(&"-".repeat(50));
            output.push_str("\n");
            
            for (i, item) in ast.items.iter().enumerate() {
                output.push_str(&format!("{}: {}\n", i, format_item_pretty(item, 0)));
            }
            Ok(output)
        },
        OutputFormat::Json => {
            Ok(serde_json::to_string_pretty(ast)?)
        }
    }
}

/// Format a span for display
fn format_span(span: &shared_types::Span) -> String {
    format!("{}..{}", span.start, span.end)
}

/// Format an AST item in a human-readable way
fn format_item_pretty(item: &shared_types::Item, indent: usize) -> String {
    let prefix = "  ".repeat(indent);
    match item {
        shared_types::Item::Function(func) => {
            format!(
                "{}Function '{}' ({} params) at {}",
                prefix,
                func.name,
                func.parameters.len(),
                format_span(&func.span)
            )
        },
        shared_types::Item::Import(import) => {
            format!(
                "{}Import '{}' at {}",
                prefix,
                import.path,
                format_span(&import.span)
            )
        },
        shared_types::Item::Struct(struct_def) => {
            format!(
                "{}Struct '{}' ({} fields) at {}",
                prefix,
                struct_def.name,
                struct_def.fields.len(),
                format_span(&struct_def.span)
            )
        },
    }
}

/// Format compilation errors for display
pub fn format_error(error: &anyhow::Error) -> String {
    let mut output = String::new();
    output.push_str("[ERROR] Compilation Error:\n");
    output.push_str(&"-".repeat(50));
    output.push_str("\n");
    
    // Main error
    output.push_str(&format!("Error: {}\n", error));
    
    // Chain of causes
    let mut current = error.source();
    while let Some(cause) = current {
        output.push_str(&format!("Caused by: {}\n", cause));
        current = cause.source();
    }
    
    output
}

/// Format semantic analysis results
pub fn format_semantic_analysis(semantic_info: &SemanticInfo, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Pretty => {
            let mut output = String::new();
            output.push_str(&format!("Semantic Analysis Results:\n"));
            output.push_str(&"-".repeat(50));
            output.push_str("\n");
            
            // Symbols
            output.push_str(&format!("Symbols ({}):\n", semantic_info.symbols.len()));
            for (name, symbol) in &semantic_info.symbols {
                output.push_str(&format!("  {}: {:?}\n", name, symbol));
            }
            
            // Type information
            if !semantic_info.type_info.is_empty() {
                output.push_str("\nType Information:\n");
                for (name, type_info) in &semantic_info.type_info {
                    output.push_str(&format!("  {}: {}\n", name, type_info));
                }
            }
            
            // Errors
            if !semantic_info.errors.is_empty() {
                output.push_str(&format!("\nSemantic Errors ({}):\n", semantic_info.errors.len()));
                for error in &semantic_info.errors {
                    output.push_str(&format!("  {}\n", error));
                }
            }
            
            Ok(output)
        },
        OutputFormat::Json => {
            Ok(serde_json::to_string_pretty(semantic_info)?)
        }
    }
}

/// Format semantic errors for display
pub fn format_semantic_errors(errors: &[SemanticError]) -> String {
    if errors.is_empty() {
        return "[SUCCESS] No semantic errors found\n".to_string();
    }
    
    let mut output = String::new();
    output.push_str(&format!("[ERROR] {} Semantic Error(s):\n", errors.len()));
    output.push_str(&"-".repeat(50));
    output.push_str("\n");
    
    for (i, error) in errors.iter().enumerate() {
        output.push_str(&format!("{}: {}\n", i + 1, error));
    }
    
    output
}

/// Format success message
pub fn format_success(message: &str) -> String {
    format!("[SUCCESS] {}\n", message)
}

/// Extract lexeme text from TokenType
fn get_token_lexeme(token_type: &shared_types::TokenType) -> String {
    use shared_types::TokenType;
    match token_type {
        TokenType::Integer(s) => s.clone(),
        TokenType::Float(s) => s.clone(),
        TokenType::String(s) => format!("\"{}\"", s),
        TokenType::Boolean(b) => b.to_string(),
        TokenType::Identifier(s) => s.clone(),
        TokenType::Keyword(k) => format!("{:?}", k).to_lowercase(),
        TokenType::Plus => "+".to_string(),
        TokenType::Minus => "-".to_string(),
        TokenType::Star => "*".to_string(),
        TokenType::Slash => "/".to_string(),
        TokenType::Percent => "%".to_string(),
        TokenType::Assign => "=".to_string(),
        TokenType::Equal => "==".to_string(),
        TokenType::NotEqual => "!=".to_string(),
        TokenType::Less => "<".to_string(),
        TokenType::LessEqual => "<=".to_string(),
        TokenType::Greater => ">".to_string(),
        TokenType::GreaterEqual => ">=".to_string(),
        TokenType::LeftParen => "(".to_string(),
        TokenType::RightParen => ")".to_string(),
        TokenType::LeftBrace => "{".to_string(),
        TokenType::RightBrace => "}".to_string(),
        TokenType::LeftBracket => "[".to_string(),
        TokenType::RightBracket => "]".to_string(),
        TokenType::Semicolon => ";".to_string(),
        TokenType::Comma => ",".to_string(),
        TokenType::Dot => ".".to_string(),
        TokenType::Colon => ":".to_string(),
        TokenType::DoubleColon => "::".to_string(),
        TokenType::Arrow => "->".to_string(),
        TokenType::Newline => "\\n".to_string(),
        TokenType::EndOfFile => "EOF".to_string(),
        TokenType::Unknown(c) => c.to_string(),
    }
}