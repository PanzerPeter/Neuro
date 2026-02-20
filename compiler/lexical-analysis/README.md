# Slice: Lexical Analysis

## Business Intent
Transform raw NEURO source code into a validated stream of tokens for parsing.

## Public Interface
- **Trigger:** `tokenize(source: &str)` function called by syntax-parsing
- **Input:** Raw source code string
- **Output:** `Result<Vec<Token>, LexError>` - Token stream or lexical error
- **Reads:** Source code from files (via neurc driver)
- **Writes:** None (pure transformation)

## Data Ownership
- **Owns:** Token definitions, lexical grammar rules
- **Subscribes to:** None

## Implementation Details
Uses Logos crate for fast lexer generation via derive macros. Supports:
- Keywords (func, val, mut, if, else, return)
- Operators (+, -, *, /, ==, !=, <, >, <=, >=, &&, ||, !)
- Literals (integers, floats, strings, booleans)
- Identifiers and symbols
- Comments (single-line // and multi-line /* */)

Error handling includes precise span tracking for diagnostic reporting.

## Dependencies
- **shared-types**: Span, Literal for token location and values
- **logos**: Lexer generation
