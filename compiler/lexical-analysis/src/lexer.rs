//! NEURO Lexer Implementation
//! 
//! High-performance lexer that converts raw source text into tokens for the NEURO compiler.
//! Handles all NEURO language constructs including ML-specific syntax and attributes.

use crate::error::LexError;
use shared_types::{Token, TokenType, Keyword, Span};

/// Main lexer for NEURO source code
pub struct Lexer<'a> {
    input: &'a str,
    position: usize,
    current_char: Option<char>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input
    pub fn new(input: &'a str) -> Self {
        let mut lexer = Self {
            input,
            position: 0,
            current_char: None,
            line: 1,
            column: 1,
        };
        lexer.current_char = lexer.input.chars().next();
        lexer
    }

    /// Tokenize the entire input into a vector of tokens
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        
        while let Some(token) = self.next_token()? {
            if let TokenType::EndOfFile = token.token_type {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        
        Ok(tokens)
    }

    /// Get the next token from the input
    pub fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_whitespace();
        
        let start_pos = self.position;
        let start_line = self.line;
        let start_column = self.column;
        
        match self.current_char {
            None => Ok(Some(Token::new(
                TokenType::EndOfFile,
                Span::new(self.position, self.position)
            ))),
            Some('\n') => {
                self.advance();
                Ok(Some(Token::new(
                    TokenType::Newline,
                    Span::new(start_pos, self.position)
                )))
            }
            Some(ch) if ch.is_ascii_digit() => self.read_number(start_pos),
            Some(ch) if ch.is_alphabetic() || ch == '_' => self.read_identifier_or_keyword(start_pos),
            Some('"') => self.read_string(start_pos),
            Some('+') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Plus, Span::new(start_pos, self.position))))
            }
            Some('-') => {
                self.advance();
                if self.current_char == Some('>') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::Arrow, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::Minus, Span::new(start_pos, self.position))))
                }
            }
            Some('*') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Star, Span::new(start_pos, self.position))))
            }
            Some('/') => {
                self.advance();
                if self.current_char == Some('/') {
                    self.skip_line_comment();
                    self.next_token()
                } else if self.current_char == Some('*') {
                    self.skip_block_comment(start_pos)?;
                    self.next_token()
                } else {
                    Ok(Some(Token::new(TokenType::Slash, Span::new(start_pos, self.position))))
                }
            }
            Some('%') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Percent, Span::new(start_pos, self.position))))
            }
            Some('=') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::Equal, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::Assign, Span::new(start_pos, self.position))))
                }
            }
            Some('!') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::NotEqual, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::LogicalNot, Span::new(start_pos, self.position))))
                }
            }
            Some('<') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::LessEqual, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::Less, Span::new(start_pos, self.position))))
                }
            }
            Some('>') => {
                self.advance();
                if self.current_char == Some('=') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::GreaterEqual, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::Greater, Span::new(start_pos, self.position))))
                }
            }
            Some('(') => {
                self.advance();
                Ok(Some(Token::new(TokenType::LeftParen, Span::new(start_pos, self.position))))
            }
            Some(')') => {
                self.advance();
                Ok(Some(Token::new(TokenType::RightParen, Span::new(start_pos, self.position))))
            }
            Some('{') => {
                self.advance();
                Ok(Some(Token::new(TokenType::LeftBrace, Span::new(start_pos, self.position))))
            }
            Some('}') => {
                self.advance();
                Ok(Some(Token::new(TokenType::RightBrace, Span::new(start_pos, self.position))))
            }
            Some('[') => {
                self.advance();
                Ok(Some(Token::new(TokenType::LeftBracket, Span::new(start_pos, self.position))))
            }
            Some(']') => {
                self.advance();
                Ok(Some(Token::new(TokenType::RightBracket, Span::new(start_pos, self.position))))
            }
            Some(';') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Semicolon, Span::new(start_pos, self.position))))
            }
            Some(',') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Comma, Span::new(start_pos, self.position))))
            }
            Some('.') => {
                self.advance();
                Ok(Some(Token::new(TokenType::Dot, Span::new(start_pos, self.position))))
            }
            Some(':') => {
                self.advance();
                if self.current_char == Some(':') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::DoubleColon, Span::new(start_pos, self.position))))
                } else {
                    Ok(Some(Token::new(TokenType::Colon, Span::new(start_pos, self.position))))
                }
            }
            Some('&') => {
                self.advance();
                if self.current_char == Some('&') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::LogicalAnd, Span::new(start_pos, self.position))))
                } else {
                    Err(LexError::UnexpectedCharacter {
                        char: '&',
                        line: start_line,
                        column: start_column,
                    })
                }
            }
            Some('|') => {
                self.advance();
                if self.current_char == Some('|') {
                    self.advance();
                    Ok(Some(Token::new(TokenType::LogicalOr, Span::new(start_pos, self.position))))
                } else {
                    Err(LexError::UnexpectedCharacter {
                        char: '|',
                        line: start_line,
                        column: start_column,
                    })
                }
            }
            Some(ch) => {
                self.advance();
                Ok(Some(Token::new(
                    TokenType::Unknown(ch),
                    Span::new(start_pos, self.position)
                )))
            }
        }
    }

    fn advance(&mut self) {
        if let Some(ch) = self.current_char {
            self.position += ch.len_utf8();
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        
        self.current_char = self.input[self.position..].chars().next();
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current_char {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.current_char {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self, start_pos: usize) -> Result<(), LexError> {
        self.advance(); // skip '*'
        
        while let Some(ch) = self.current_char {
            if ch == '*' {
                self.advance();
                if self.current_char == Some('/') {
                    self.advance();
                    return Ok(());
                }
            } else {
                self.advance();
            }
        }
        
        Err(LexError::UnterminatedBlockComment {
            start_position: start_pos,
        })
    }

    fn read_number(&mut self, start_pos: usize) -> Result<Option<Token>, LexError> {
        let mut has_dot = false;
        let mut number_str = String::new();
        
        while let Some(ch) = self.current_char {
            if ch.is_ascii_digit() {
                number_str.push(ch);
                self.advance();
            } else if ch == '.' && !has_dot {
                has_dot = true;
                number_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        
        let token_type = if has_dot {
            TokenType::Float(number_str)
        } else {
            TokenType::Integer(number_str)
        };
        
        Ok(Some(Token::new(token_type, Span::new(start_pos, self.position))))
    }

    fn read_identifier_or_keyword(&mut self, start_pos: usize) -> Result<Option<Token>, LexError> {
        let mut identifier = String::new();
        
        while let Some(ch) = self.current_char {
            if ch.is_alphanumeric() || ch == '_' {
                identifier.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        
        let token_type = match identifier.as_str() {
            "if" => TokenType::Keyword(Keyword::If),
            "else" => TokenType::Keyword(Keyword::Else),
            "while" => TokenType::Keyword(Keyword::While),
            "for" => TokenType::Keyword(Keyword::For),
            "break" => TokenType::Keyword(Keyword::Break),
            "continue" => TokenType::Keyword(Keyword::Continue),
            "return" => TokenType::Keyword(Keyword::Return),
            "fn" => TokenType::Keyword(Keyword::Fn),
            "let" => TokenType::Keyword(Keyword::Let),
            "mut" => TokenType::Keyword(Keyword::Mut),
            "const" => TokenType::Keyword(Keyword::Const),
            "type" => TokenType::Keyword(Keyword::Type),
            "struct" => TokenType::Keyword(Keyword::Struct),
            "enum" => TokenType::Keyword(Keyword::Enum),
            "Tensor" => TokenType::Keyword(Keyword::Tensor),
            "grad" => TokenType::Keyword(Keyword::Grad),
            "kernel" => TokenType::Keyword(Keyword::Kernel),
            "gpu" => TokenType::Keyword(Keyword::Gpu),
            "import" => TokenType::Keyword(Keyword::Import),
            "export" => TokenType::Keyword(Keyword::Export),
            "module" => TokenType::Keyword(Keyword::Module),
            "Arc" => TokenType::Keyword(Keyword::Arc),
            "Pool" => TokenType::Keyword(Keyword::Pool),
            "true" => TokenType::Boolean(true),
            "false" => TokenType::Boolean(false),
            _ => TokenType::Identifier(identifier),
        };
        
        Ok(Some(Token::new(token_type, Span::new(start_pos, self.position))))
    }

    fn read_string(&mut self, start_pos: usize) -> Result<Option<Token>, LexError> {
        self.advance(); // skip opening quote
        let mut string_value = String::new();
        
        while let Some(ch) = self.current_char {
            match ch {
                '"' => {
                    self.advance(); // skip closing quote
                    return Ok(Some(Token::new(
                        TokenType::String(string_value),
                        Span::new(start_pos, self.position)
                    )));
                }
                '\\' => {
                    self.advance();
                    match self.current_char {
                        Some('n') => {
                            string_value.push('\n');
                            self.advance();
                        }
                        Some('t') => {
                            string_value.push('\t');
                            self.advance();
                        }
                        Some('r') => {
                            string_value.push('\r');
                            self.advance();
                        }
                        Some('\\') => {
                            string_value.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            string_value.push('"');
                            self.advance();
                        }
                        Some(other) => {
                            return Err(LexError::InvalidEscapeSequence {
                                sequence: format!("\\{}", other),
                                line: self.line,
                                column: self.column,
                            });
                        }
                        None => {
                            return Err(LexError::UnterminatedString {
                                start_position: start_pos,
                            });
                        }
                    }
                }
                '\n' => {
                    return Err(LexError::UnterminatedString {
                        start_position: start_pos,
                    });
                }
                _ => {
                    string_value.push(ch);
                    self.advance();
                }
            }
        }
        
        Err(LexError::UnterminatedString {
            start_position: start_pos,
        })
    }
}
