// Unit tests for the tokenizer (moved out of lib.rs to keep the slice entry point lean).

use super::*;
use shared_types::{FloatSuffix, IntSuffix};

#[test]
fn tokenize_empty_source() {
    let result = tokenize("").unwrap();
    assert_eq!(result.len(), 1);
    assert!(matches!(result[0].kind, TokenKind::Eof));
}

#[test]
fn tokenize_keywords() {
    let result = tokenize("func val mut if else return").unwrap();
    assert_eq!(result.len(), 7); // 6 keywords + EOF

    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Val));
    assert!(matches!(result[2].kind, TokenKind::Mut));
    assert!(matches!(result[3].kind, TokenKind::If));
    assert!(matches!(result[4].kind, TokenKind::Else));
    assert!(matches!(result[5].kind, TokenKind::Return));
}

#[test]
fn tokenize_unsafe_keyword() {
    let result = tokenize("unsafe").unwrap();
    assert_eq!(result.len(), 2); // keyword + EOF
    assert!(matches!(result[0].kind, TokenKind::Unsafe));
}

#[test]
fn tokenize_loop_keyword() {
    let result = tokenize("loop").unwrap();
    assert_eq!(result.len(), 2); // keyword + EOF
    assert!(matches!(result[0].kind, TokenKind::Loop));
}

#[test]
fn tokenize_question_operators() {
    // Longest match decides: `??` stays one coalescing token, and a lone `?` is the
    // error-propagation operator rather than half of one.
    let result = tokenize("a? b ?? c").unwrap();
    assert!(matches!(result[1].kind, TokenKind::Question));
    assert!(matches!(result[3].kind, TokenKind::QuestionQuestion));
}

#[test]
fn tokenize_identifiers() {
    let result = tokenize("foo bar_baz _underscore").unwrap();
    assert_eq!(result.len(), 4); // 3 identifiers + EOF

    match &result[0].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "foo"),
        _ => panic!("Expected identifier"),
    }
    match &result[1].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "bar_baz"),
        _ => panic!("Expected identifier"),
    }
    match &result[2].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "_underscore"),
        _ => panic!("Expected identifier"),
    }
}

#[test]
fn tokenize_unicode_identifiers() {
    let result = tokenize("αβγ 変数 identifier_with_数字").unwrap();
    assert_eq!(result.len(), 4); // 3 identifiers + EOF

    match &result[0].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "αβγ"),
        _ => panic!("Expected identifier"),
    }
    match &result[1].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "変数"),
        _ => panic!("Expected identifier"),
    }
    match &result[2].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "identifier_with_数字"),
        _ => panic!("Expected identifier"),
    }
}

#[test]
fn tokenize_integers() {
    let result = tokenize("42 0 1234567890 100_000").unwrap();
    assert_eq!(result.len(), 5); // 4 integers + EOF

    assert!(matches!(result[0].kind, TokenKind::Integer(42)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0)));
    assert!(matches!(result[2].kind, TokenKind::Integer(1234567890)));
    assert!(matches!(result[3].kind, TokenKind::Integer(100000)));
}

#[test]
fn tokenize_integer_bases() {
    let result = tokenize("0b1010 0o755 0xDEADBEEF").unwrap();
    assert_eq!(result.len(), 4); // 3 integers + EOF

    assert!(matches!(result[0].kind, TokenKind::Integer(0b1010)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0o755)));
    assert!(matches!(result[2].kind, TokenKind::Integer(0xDEADBEEF)));
}

#[test]
fn tokenize_integer_suffixes_decimal() {
    let result = tokenize("42i64 255u8 1000i32 0u16").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed integers + EOF
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 42);
            assert_eq!(tok.suffix, IntSuffix::I64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 255);
            assert_eq!(tok.suffix, IntSuffix::U8);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 1000);
            assert_eq!(tok.suffix, IntSuffix::I32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[3].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0);
            assert_eq!(tok.suffix, IntSuffix::U16);
        }
        _ => panic!("expected IntegerSuffix"),
    }
}

#[test]
fn tokenize_integer_suffixes_other_bases() {
    let result = tokenize("0b1010i32 0o755u64 0xFFu8").unwrap();
    assert_eq!(result.len(), 4); // 3 suffixed integers + EOF
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0b1010);
            assert_eq!(tok.suffix, IntSuffix::I32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0o755);
            assert_eq!(tok.suffix, IntSuffix::U64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0xFF);
            assert_eq!(tok.suffix, IntSuffix::U8);
        }
        _ => panic!("expected IntegerSuffix"),
    }
}

#[test]
fn unsuffixed_integers_unchanged() {
    // Ensure plain integers still produce Integer tokens, not IntegerSuffix
    let result = tokenize("42 0 1000").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(42)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0)));
    assert!(matches!(result[2].kind, TokenKind::Integer(1000)));
}

#[test]
fn tokenize_floats() {
    let result = tokenize("3.15 0.5 2.0 1e10 1.5e-5").unwrap();
    assert_eq!(result.len(), 6); // 5 floats + EOF

    match result[0].kind {
        TokenKind::Float(f) => assert!((f - 3.15).abs() < 1e-10),
        _ => panic!("Expected float"),
    }
    match result[1].kind {
        TokenKind::Float(f) => assert!((f - 0.5).abs() < 1e-10),
        _ => panic!("Expected float"),
    }
    match result[2].kind {
        TokenKind::Float(f) => assert!((f - 2.0).abs() < 1e-10),
        _ => panic!("Expected float"),
    }
    match result[3].kind {
        TokenKind::Float(f) => assert!((f - 1e10).abs() < 1e-10),
        _ => panic!("Expected float"),
    }
    match result[4].kind {
        TokenKind::Float(f) => assert!((f - 1.5e-5).abs() < 1e-10),
        _ => panic!("Expected float"),
    }
}

#[test]
fn tokenize_float_suffixes() {
    let result = tokenize("1.5f32 2.0f64 1e10f32 1.5e-5f64").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed floats + EOF
    match &result[0].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[1].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2.0).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F64);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1e10).abs() < 1.0);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[3].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5e-5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F64);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn tokenize_half_precision_float_suffixes() {
    // `f16` / `bf16` half-precision literals. `bf16` must not be mis-split
    // as `b` + `f16`, and `1.5f16` must not be read as `1.5` + the `f16` ident.
    let result = tokenize("1.5f16 0.02bf16 2e3f16 1.0bf16").unwrap();
    assert_eq!(result.len(), 5); // 4 suffixed floats + EOF
    match &result[0].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.5).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::F16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[1].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 0.02).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::BF16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2e3).abs() < 1.0);
            assert_eq!(tok.suffix, FloatSuffix::F16);
        }
        _ => panic!("expected FloatSuffix"),
    }
    match &result[3].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 1.0).abs() < 1e-10);
            assert_eq!(tok.suffix, FloatSuffix::BF16);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn unsuffixed_floats_unchanged() {
    // Ensure plain floats still produce Float tokens, not FloatSuffix.
    let result = tokenize("3.15 0.5 1e10").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Float(_)));
    assert!(matches!(result[1].kind, TokenKind::Float(_)));
    assert!(matches!(result[2].kind, TokenKind::Float(_)));
}

#[test]
fn underscore_separators_decimal() {
    let result = tokenize("1_000_000 1_2_3").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(1_000_000)));
    assert!(matches!(result[1].kind, TokenKind::Integer(123)));
}

#[test]
fn underscore_separators_hex_binary_octal() {
    let result = tokenize("0xFF_FF 0b1010_0011 0o7_5_5").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Integer(0xFFFF)));
    assert!(matches!(result[1].kind, TokenKind::Integer(0b1010_0011)));
    assert!(matches!(result[2].kind, TokenKind::Integer(0o755)));
}

#[test]
fn underscore_separators_float() {
    let result = tokenize("1_000.000_5 1_0e1_0").unwrap();
    match result[0].kind {
        TokenKind::Float(f) => assert!((f - 1000.0005).abs() < 1e-9),
        _ => panic!("expected float"),
    }
    match result[1].kind {
        // 10e10 == 1.0e11
        TokenKind::Float(f) => assert!((f - 1.0e11).abs() < 1.0),
        _ => panic!("expected float"),
    }
}

#[test]
fn underscore_separators_suffixed() {
    let result = tokenize("1_000i64 0xFF_FFu32 2_000.5f32").unwrap();
    match &result[0].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 1000);
            assert_eq!(tok.suffix, IntSuffix::I64);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[1].kind {
        TokenKind::IntegerSuffix(tok) => {
            assert_eq!(tok.value, 0xFFFF);
            assert_eq!(tok.suffix, IntSuffix::U32);
        }
        _ => panic!("expected IntegerSuffix"),
    }
    match &result[2].kind {
        TokenKind::FloatSuffix(tok) => {
            assert!((tok.value - 2000.5).abs() < 1e-9);
            assert_eq!(tok.suffix, FloatSuffix::F32);
        }
        _ => panic!("expected FloatSuffix"),
    }
}

#[test]
fn leading_underscore_is_identifier_not_number() {
    // A leading underscore must bind as an identifier, never a numeric literal —
    // the digit-separator rule applies only between digits.
    let result = tokenize("_1000").unwrap();
    match &result[0].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "_1000"),
        _ => panic!("expected identifier"),
    }
}

#[test]
fn tokenize_strings() {
    let result = tokenize(r#""hello" "world" "with spaces""#).unwrap();
    assert_eq!(result.len(), 4); // 3 strings + EOF

    assert_eq!(plain_string(&result[0].kind), "hello");
    assert_eq!(plain_string(&result[1].kind), "world");
    assert_eq!(plain_string(&result[2].kind), "with spaces");
}

/// The decoded text of a plain (non-interpolated) string token; panics otherwise.
fn plain_string(kind: &TokenKind) -> &str {
    match kind {
        TokenKind::String(StringValue::Plain(s)) => s,
        other => panic!("expected plain string token, got {:?}", other),
    }
}

#[test]
fn tokenize_string_escapes() {
    let result = tokenize(r#""hello\nworld" "tab\there" "quote\"here""#).unwrap();

    assert_eq!(plain_string(&result[0].kind), "hello\nworld");
    assert_eq!(plain_string(&result[1].kind), "tab\there");
    assert_eq!(plain_string(&result[2].kind), "quote\"here");
}

#[test]
fn tokenize_string_unicode_escape() {
    let result = tokenize(r#""unicode: \u{1F600}""#).unwrap();
    assert_eq!(plain_string(&result[0].kind), "unicode: \u{1F600}");
}

/// The chunks of an interpolated string token; panics otherwise.
fn interp_chunks(kind: &TokenKind) -> &[InterpChunk] {
    match kind {
        TokenKind::String(StringValue::Interp(chunks)) => chunks,
        other => panic!("expected interpolated string token, got {:?}", other),
    }
}

#[test]
fn tokenize_interpolation_splits_text_and_holes() {
    //             0123456789012345678901
    let src = r#""Sum: {a + b}, n: {n}""#;
    let result = tokenize(src).unwrap();
    let chunks = interp_chunks(&result[0].kind);

    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0], InterpChunk::Text("Sum: ".to_string()));
    assert_eq!(
        chunks[1],
        InterpChunk::Hole {
            source: "a + b".to_string(),
            // Absolute file span: the hole text sits at bytes 7..12, braces excluded.
            span: Span::new(7, 12),
        }
    );
    assert_eq!(chunks[2], InterpChunk::Text(", n: ".to_string()));
    assert_eq!(
        chunks[3],
        InterpChunk::Hole {
            source: "n".to_string(),
            span: Span::new(19, 20),
        }
    );
}

#[test]
fn adjacent_holes_produce_no_empty_text() {
    let result = tokenize(r#""{a}{b}""#).unwrap();
    let chunks = interp_chunks(&result[0].kind);

    assert_eq!(chunks.len(), 2);
    assert!(chunks.iter().all(|c| matches!(c, InterpChunk::Hole { .. })));
}

#[test]
fn escaped_brace_is_plain_text() {
    // `\{` suppresses the hole; the trailing bare `}` stays literal too.
    let result = tokenize(r#""\{not a hole}""#).unwrap();
    assert_eq!(plain_string(&result[0].kind), "{not a hole}");
}

#[test]
fn lone_closing_brace_stays_literal() {
    // The language defines `\{` but no `\}`, so an unpaired `}` must survive as itself.
    let result = tokenize(r#""a}b""#).unwrap();
    assert_eq!(plain_string(&result[0].kind), "a}b");
}

#[test]
fn unterminated_hole_is_lex_error() {
    let err = tokenize(r#""x {a""#).unwrap_err();
    assert!(matches!(err, LexError::UnterminatedInterpolation { .. }));
}

#[test]
fn nested_string_inside_hole_is_rejected() {
    // A `"` inside a hole closes the enclosing literal, leaving the hole open.
    // Reported as an unterminated hole rather than silently mis-lexed.
    let err = tokenize(r#""pre {"inner"} post""#).unwrap_err();
    assert!(matches!(err, LexError::UnterminatedInterpolation { .. }));
}

#[test]
fn char_literal_unicode_braces_do_not_count_as_depth() {
    // The `}` inside `'\u{7D}'` belongs to the escape payload, not the hole.
    let result = tokenize(r#""{'\u{7D}'} tail}""#).unwrap();
    let chunks = interp_chunks(&result[0].kind);

    assert_eq!(chunks.len(), 2);
    match &chunks[0] {
        InterpChunk::Hole { source, .. } => assert_eq!(source, r"'\u{7D}'"),
        other => panic!("expected hole chunk, got {:?}", other),
    }
    assert_eq!(chunks[1], InterpChunk::Text(" tail}".to_string()));
}

#[test]
fn struct_literal_braces_nest_inside_hole() {
    let result = tokenize(r#""{Point { x: 1 }}""#).unwrap();
    let chunks = interp_chunks(&result[0].kind);

    assert_eq!(chunks.len(), 1);
    match &chunks[0] {
        InterpChunk::Hole { source, .. } => assert_eq!(source, "Point { x: 1 }"),
        other => panic!("expected hole chunk, got {:?}", other),
    }
}

#[test]
fn tokenize_operators() {
    let result = tokenize("+ - * / % = == != < > <= >=").unwrap();
    assert_eq!(result.len(), 13); // 12 operators + EOF

    assert!(matches!(result[0].kind, TokenKind::Plus));
    assert!(matches!(result[1].kind, TokenKind::Minus));
    assert!(matches!(result[2].kind, TokenKind::Star));
    assert!(matches!(result[3].kind, TokenKind::Slash));
    assert!(matches!(result[4].kind, TokenKind::Percent));
    assert!(matches!(result[5].kind, TokenKind::Equal));
    assert!(matches!(result[6].kind, TokenKind::EqualEqual));
    assert!(matches!(result[7].kind, TokenKind::NotEqual));
    assert!(matches!(result[8].kind, TokenKind::Less));
    assert!(matches!(result[9].kind, TokenKind::Greater));
    assert!(matches!(result[10].kind, TokenKind::LessEqual));
    assert!(matches!(result[11].kind, TokenKind::GreaterEqual));
}

#[test]
fn tokenize_logical_operators() {
    let result = tokenize("&& || !").unwrap();
    assert_eq!(result.len(), 4); // 3 operators + EOF

    assert!(matches!(result[0].kind, TokenKind::AmpAmp));
    assert!(matches!(result[1].kind, TokenKind::PipePipe));
    assert!(matches!(result[2].kind, TokenKind::Bang));
}

#[test]
fn tokenize_move_keyword() {
    // `move` is a keyword (closure capture), distinct from an identifier.
    let result = tokenize("move |x|").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Move));
    assert!(matches!(result[1].kind, TokenKind::Pipe));
}

#[test]
fn tokenize_delimiters() {
    let result = tokenize("( ) { } [ ] , : ;").unwrap();
    assert_eq!(result.len(), 10); // 9 delimiters + EOF

    assert!(matches!(result[0].kind, TokenKind::LeftParen));
    assert!(matches!(result[1].kind, TokenKind::RightParen));
    assert!(matches!(result[2].kind, TokenKind::LeftBrace));
    assert!(matches!(result[3].kind, TokenKind::RightBrace));
    assert!(matches!(result[4].kind, TokenKind::LeftBracket));
    assert!(matches!(result[5].kind, TokenKind::RightBracket));
    assert!(matches!(result[6].kind, TokenKind::Comma));
    assert!(matches!(result[7].kind, TokenKind::Colon));
    assert!(matches!(result[8].kind, TokenKind::Semicolon));
}

#[test]
fn tokenize_special_operators() {
    let result = tokenize("-> @ :: . .. ..=").unwrap();
    assert_eq!(result.len(), 7); // 6 operators + EOF

    assert!(matches!(result[0].kind, TokenKind::Arrow));
    assert!(matches!(result[1].kind, TokenKind::At));
    assert!(matches!(result[2].kind, TokenKind::ColonColon));
    assert!(matches!(result[3].kind, TokenKind::Dot));
    assert!(matches!(result[4].kind, TokenKind::DotDot));
    assert!(matches!(result[5].kind, TokenKind::DotDotEqual));
}

#[test]
fn tokenize_line_comments() {
    let result = tokenize("func // this is a comment\nval").unwrap();
    assert_eq!(result.len(), 4); // func, newline, val, EOF

    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Newline));
    assert!(matches!(result[2].kind, TokenKind::Val));
}

#[test]
fn tokenize_block_comments() {
    let result = tokenize("func /* block comment */ val").unwrap();
    assert_eq!(result.len(), 3); // func, val, EOF

    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Val));
}

#[test]
fn tokenize_multiline_block_comments() {
    let result = tokenize("func /*\nmulti\nline\ncomment\n*/ val").unwrap();
    assert_eq!(result.len(), 3); // func, val, EOF

    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Val));
}

#[test]
fn tokenize_simple_function() {
    let source = r#"
func add(a: i32, b: i32) -> i32 {
    return a + b
}
"#;
    let result = tokenize(source).unwrap();

    // Verify we got tokens (not checking exact count due to newlines)
    assert!(result.len() > 10);
    assert!(matches!(result[0].kind, TokenKind::Newline));
    assert!(matches!(result[1].kind, TokenKind::Func));
    // More detailed checks would go here
}

#[test]
fn tokenize_complex_expression() {
    let result = tokenize("val x = (a + b) * c - d / e").unwrap();

    assert!(matches!(result[0].kind, TokenKind::Val));
    match &result[1].kind {
        TokenKind::Identifier(s) => assert_eq!(s, "x"),
        _ => panic!("Expected identifier"),
    }
    assert!(matches!(result[2].kind, TokenKind::Equal));
    assert!(matches!(result[3].kind, TokenKind::LeftParen));
}

#[test]
fn error_on_unterminated_string() {
    let result = tokenize(r#""unterminated"#);
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::UnterminatedString { .. } => {}
        err => panic!("Expected UnterminatedString, got: {:?}", err),
    }
}

#[test]
fn error_on_invalid_escape() {
    let result = tokenize(r#""invalid\q""#);
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::InvalidEscape { .. } => {}
        err => panic!("Expected InvalidEscape, got: {:?}", err),
    }
}

#[test]
fn error_on_unexpected_char() {
    let result = tokenize("$invalid");
    assert!(result.is_err());
    match result.unwrap_err() {
        LexError::UnexpectedChar { character, .. } => assert_eq!(character, '$'),
        _ => panic!("Expected unexpected char error"),
    }
}

#[test]
fn span_tracking() {
    let result = tokenize("func add").unwrap();

    assert_eq!(result[0].span, Span::new(0, 4)); // "func"
    assert_eq!(result[1].span, Span::new(5, 8)); // "add"
}

#[test]
fn newline_handling() {
    let result = tokenize("func\n\nval").unwrap();

    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Newline));
    assert!(matches!(result[2].kind, TokenKind::Val));
}

#[test]
fn whitespace_handling() {
    let result = tokenize("func   \t  val").unwrap();

    // Whitespace should be skipped
    assert_eq!(result.len(), 3); // func, val, EOF
    assert!(matches!(result[0].kind, TokenKind::Func));
    assert!(matches!(result[1].kind, TokenKind::Val));
}

#[test]
fn boolean_literals() {
    let result = tokenize("true false").unwrap();

    assert!(matches!(result[0].kind, TokenKind::True));
    assert!(matches!(result[1].kind, TokenKind::False));
}

#[test]
fn is_valid_identifier_test() {
    assert!(Lexer::is_valid_identifier("foo"));
    assert!(Lexer::is_valid_identifier("_bar"));
    assert!(Lexer::is_valid_identifier("baz123"));
    assert!(Lexer::is_valid_identifier("αβγ"));
    assert!(Lexer::is_valid_identifier("変数"));

    assert!(!Lexer::is_valid_identifier(""));
    assert!(!Lexer::is_valid_identifier("123abc"));
    assert!(!Lexer::is_valid_identifier("-invalid"));
}

#[test]
fn tokenize_char_literals() {
    let result = tokenize("'a' '\\n' '\\u{1F44D}' '\\''").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Char('a')));
    assert!(matches!(result[1].kind, TokenKind::Char('\n')));
    assert!(matches!(result[2].kind, TokenKind::Char('\u{1F44D}')));
    assert!(matches!(result[3].kind, TokenKind::Char('\'')));
}

#[test]
fn empty_and_multi_char_literals_are_rejected() {
    // Neither `''` nor `'ab'` matches the single-scalar char-literal regex, so
    // both surface as lex errors rather than tokenizing. `'ab'` lexes its `'ab`
    // prefix as a lifetime, then the trailing stray `'` is the lex error.
    assert!(tokenize("''").is_err());
    assert!(tokenize("'ab'").is_err());
}

#[test]
fn tokenize_lifetimes() {
    // A quote-less `'ident` is a lifetime. The stored name drops the `'`.
    let result = tokenize("<'a, 'lt>").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Less));
    assert!(matches!(&result[1].kind, TokenKind::Lifetime(n) if n == "a"));
    assert!(matches!(&result[3].kind, TokenKind::Lifetime(n) if n == "lt"));
    assert!(matches!(result[4].kind, TokenKind::Greater));
}

#[test]
fn char_literal_wins_over_lifetime() {
    // `'a'` carries a closing quote, so it is a strictly longer match than the
    // lifetime `'a`; logos' longest-match rule keeps it a char literal.
    let result = tokenize("'a'").unwrap();
    assert!(matches!(result[0].kind, TokenKind::Char('a')));
}

#[test]
fn stress_test_large_input() {
    let mut source = String::new();
    for i in 0..1000 {
        source.push_str(&format!("val x{} = {}\n", i, i));
    }

    let result = tokenize(&source);
    assert!(result.is_ok());
    let tokens = result.unwrap();
    // Each line has: val, identifier, =, number, newline
    // Plus one EOF at the end
    assert_eq!(tokens.len(), 1000 * 5 + 1);
}

/// Regression: `/*` with no `*/` reported "unexpected character '/'".
#[test]
fn unterminated_block_comment_is_named_as_such() {
    let err = tokenize("func main() -> i32 {\n/* never closed\n    0\n}\n")
        .expect_err("an unterminated block comment must not lex");
    assert!(
        matches!(err, LexError::UnterminatedBlockComment { .. }),
        "expected UnterminatedBlockComment, got {err:?}"
    );
}

#[test]
fn a_closed_block_comment_and_division_still_lex() {
    let tokens = tokenize("/* fine */ 8 / 2").expect("closed comment and division lex");
    assert!(
        tokens.iter().any(|t| t.kind == TokenKind::Slash),
        "division was swallowed: {tokens:?}"
    );
}

/// The whole point of the depth counter: the FIRST `*/` closes only the inner
/// comment, so `still outer` must not reach the token stream and the final `*/`
/// must not lex as `Star` then `Slash`.
#[test]
fn block_comments_nest_one_level() {
    let tokens = tokenize("val /* outer /* inner */ still outer */ x")
        .expect("a nested block comment lexes");
    assert_eq!(tokens.len(), 3, "unexpected tokens: {tokens:?}");
    assert!(matches!(tokens[0].kind, TokenKind::Val));
    assert_eq!(tokens[1].kind, TokenKind::Identifier("x".to_string()));
}

#[test]
fn block_comments_nest_arbitrarily_deep() {
    let tokens = tokenize("1 /* a /* b /* c /* d */ c */ b */ a */ 2")
        .expect("a deeply nested block comment lexes");
    assert_eq!(
        tokens
            .iter()
            .filter(|t| matches!(t.kind, TokenKind::Integer(_)))
            .count(),
        2,
        "nesting swallowed or leaked tokens: {tokens:?}"
    );
}

#[test]
fn nested_block_comment_spanning_lines_lexes() {
    let tokens = tokenize(
        "val
/* outer
   /* inner
   */
   still outer
*/
x",
    )
    .expect("a multi-line nested comment lexes");
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::Identifier("x".to_string())),
        "the comment did not close: {tokens:?}"
    );
    assert!(
        !tokens
            .iter()
            .any(|t| t.kind == TokenKind::Identifier("outer".to_string())),
        "comment body leaked into the token stream: {tokens:?}"
    );
}

/// An inner `/*` needs its own `*/`; one closer for two openers leaves the file
/// inside a comment at EOF.
#[test]
fn an_unclosed_inner_comment_leaves_the_outer_open() {
    let err =
        tokenize("val /* outer /* inner */ x").expect_err("one closer cannot close two openers");
    assert!(
        matches!(err, LexError::UnterminatedBlockComment { .. }),
        "expected UnterminatedBlockComment, got {err:?}"
    );
}

/// Delimiters may abut without either being mis-split: `/**/` is empty, `/***/`
/// closes on the second star, and `/*/ */` does not read its `/` as an opener.
#[test]
fn adjacent_block_comment_delimiters_lex() {
    for source in ["/**/ 7", "/***/ 7", "/*/ */ 7", "/* /* */ */ 7"] {
        let tokens = tokenize(source).unwrap_or_else(|e| panic!("{source:?} failed: {e:?}"));
        assert_eq!(
            tokens[0].kind,
            TokenKind::Integer(7),
            "{source:?} produced {tokens:?}"
        );
    }
}

/// `/*/*/` opens twice and closes never — the classic maximal-munch trap.
#[test]
fn a_lone_opener_pair_is_unterminated() {
    let err = tokenize("/*/*/").expect_err("`/*/*/` never closes");
    assert!(
        matches!(err, LexError::UnterminatedBlockComment { .. }),
        "expected UnterminatedBlockComment, got {err:?}"
    );
}

#[test]
fn a_nested_comment_holding_non_ascii_text_lexes() {
    let tokens = tokenize("/* résumé /* 日本語 */ ✓ */ 1").expect("non-ASCII comment body lexes");
    assert_eq!(tokens[0].kind, TokenKind::Integer(1));
}

/// A line comment inside a block comment is raw text: its `*/` still closes.
#[test]
fn a_line_comment_inside_a_block_comment_is_inert() {
    let tokens = tokenize("/* // not a line comment */ 5").expect("mixed comment forms lex");
    assert_eq!(tokens[0].kind, TokenKind::Integer(5));
}

#[test]
fn triple_quoted_string_dedents_to_the_closing_delimiter() {
    let src = "\"\"\"\n    Hello\n    World\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "Hello\nWorld\n");
    assert!(matches!(result[1].kind, TokenKind::Eof));
}

/// The whole literal is one token: the lexer must not resume scanning inside the body.
#[test]
fn triple_quoted_string_is_a_single_token() {
    let src = "val a = \"\"\"\n  func val 1 + 2\n  \"\"\"\nval b = 1";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[3].kind), "func val 1 + 2\n");
    // A newline token separates the two bindings; `val b` must survive intact.
    assert!(matches!(result[5].kind, TokenKind::Val));
}

#[test]
fn triple_quoted_string_keeps_interior_quotes() {
    let src = "\"\"\"\n    say \"hi\" and \"\" too\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "say \"hi\" and \"\" too\n");
}

#[test]
fn triple_quoted_string_blank_lines_need_no_indentation() {
    let src = "\"\"\"\n    first\n\n    last\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "first\n\nlast\n");
}

#[test]
fn triple_quoted_string_keeps_indentation_beyond_the_delimiter() {
    let src = "\"\"\"\n  root\n      nested\n  \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "root\n    nested\n");
}

#[test]
fn triple_quoted_string_decodes_escapes() {
    let src = "\"\"\"\n    a\\tb\\u{21}\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "a\tb!\n");
}

/// Text after the opening `"""` is content, and is exempt from the dedent rule —
/// it sits flush against the delimiter and cannot carry the closing indentation.
#[test]
fn triple_quoted_string_keeps_text_on_the_opening_line() {
    let src = "\"\"\"lead\n    tail\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    assert_eq!(plain_string(&result[0].kind), "lead\ntail\n");
}

/// Dedent drops indentation characters from the indexed content rather than
/// rebuilding the text, so a hole's span still points at its real source offsets.
#[test]
fn triple_quoted_string_holes_report_real_spans() {
    //         0   1234  5678901234
    let src = "\"\"\"\n    v={value}\n    \"\"\"";
    let result = tokenize(src).expect("block string lexes");
    let chunks = interp_chunks(&result[0].kind);
    assert_eq!(chunks[0], InterpChunk::Text("v=".to_string()));
    match &chunks[1] {
        InterpChunk::Hole { source, span } => {
            assert_eq!(source, "value");
            assert_eq!(&src[span.start..span.end], "value");
        }
        other => panic!("expected a hole, got {other:?}"),
    }
    assert_eq!(chunks[2], InterpChunk::Text("\n".to_string()));
}

#[test]
fn triple_quoted_string_without_a_close_is_named_as_such() {
    let err = tokenize("\"\"\"\n    never closed\n")
        .expect_err("an unterminated block string must not lex");
    assert!(
        matches!(err, LexError::UnterminatedTripleQuotedString { .. }),
        "expected UnterminatedTripleQuotedString, got {err:?}"
    );
}

#[test]
fn triple_quoted_string_closing_delimiter_must_stand_alone() {
    let err =
        tokenize("\"\"\"\n    text \"\"\"").expect_err("a trailing closing delimiter must not lex");
    assert!(
        matches!(err, LexError::TripleQuoteClosingNotOnOwnLine { .. }),
        "expected TripleQuoteClosingNotOnOwnLine, got {err:?}"
    );
}

#[test]
fn triple_quoted_string_rejects_under_indented_lines() {
    let err = tokenize("\"\"\"\n    deep\n  shallow\n    \"\"\"")
        .expect_err("an under-indented content line must not lex");
    match err {
        LexError::TripleQuoteUnderIndented { indent, span } => {
            assert_eq!(indent, 4);
            assert_eq!(span.start, 13);
        }
        other => panic!("expected TripleQuoteUnderIndented, got {other:?}"),
    }
}

/// `""` must keep lexing as the empty string: logos' longest-match rule is the only
/// thing separating it from the three-quote delimiter.
#[test]
fn empty_string_still_lexes_next_to_block_strings() {
    let result = tokenize(r#""" "a""#).expect("empty string lexes");
    assert_eq!(plain_string(&result[0].kind), "");
    assert_eq!(plain_string(&result[1].kind), "a");
}
