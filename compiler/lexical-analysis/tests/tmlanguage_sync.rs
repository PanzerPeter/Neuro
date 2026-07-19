//! Keeps the editor's TextMate grammar in sync with the lexer.
//!
//! `neuro-language-support/syntaxes/neuro.tmLanguage.json` is a hand-written regex
//! grammar with no structural link to `TokenKind` — nothing else in the workspace
//! fails when a keyword is added to the lexer and not to the grammar. This test is
//! that link: it re-reads the lexer's own source, extracts every keyword literal
//! declared with `#[token("...")]`, and asserts each one is matched by the grammar.
//!
//! Reading the source text (rather than reflecting over `TokenKind`) is deliberate —
//! logos consumes the attributes at compile time, so the literals are not observable
//! at runtime any other way, and a source scan needs no upkeep when a keyword lands.
//!
//! Scope note: this covers keyword *words* only. Grammar rules with no one-to-one
//! token counterpart — string bodies and their escapes above all — must still be
//! updated by hand. That matters for the planned stateful-lexer rewrite behind
//! string interpolation, which changes how a string literal is tokenized without
//! adding a single keyword.

use std::path::PathBuf;

fn workspace_file(relative: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/compiler/lexical-analysis
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read(relative: &str) -> String {
    let path = workspace_file(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Every `#[token("word")]` literal in the lexer that is a bare identifier-shaped
/// keyword. Punctuation tokens (`->`, `::`, …) are covered by separate grammar rules
/// and are not asserted here.
fn lexer_keywords(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#[token(\"") else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let literal = &rest[..end];
        if !literal.is_empty() && literal.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
            out.push(literal.to_string());
        }
    }
    out
}

/// True when `keyword` appears in the grammar delimited by non-word characters —
/// i.e. as its own alternative inside a `\b(a|b|c)\b` match, not as a substring of
/// a longer word (`in` inside `continue`).
fn grammar_matches(grammar: &str, keyword: &str) -> bool {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    grammar.match_indices(keyword).any(|(at, _)| {
        let before = grammar[..at].chars().next_back();
        let after = grammar[at + keyword.len()..].chars().next();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

#[test]
fn textmate_grammar_covers_every_lexer_keyword() {
    let tokens = read("compiler/lexical-analysis/src/tokens.rs");
    let grammar = read("neuro-language-support/syntaxes/neuro.tmLanguage.json");

    let keywords = lexer_keywords(&tokens);
    assert!(
        keywords.len() > 20,
        "keyword extraction found only {} literals — the `#[token(\"...\")]` \
         attribute layout in tokens.rs changed and this test no longer sees them",
        keywords.len()
    );

    let missing: Vec<&String> = keywords
        .iter()
        .filter(|kw| !grammar_matches(&grammar, kw))
        .collect();

    assert!(
        missing.is_empty(),
        "lexer keywords absent from neuro-language-support/syntaxes/neuro.tmLanguage.json: \
         {missing:?}\nAdd them to the matching `keywords` or `constants` pattern — editor \
         highlighting has no other link to the lexer."
    );
}

#[test]
fn grammar_matcher_respects_word_boundaries() {
    let grammar = r#""match": "\\b(if|else|in)\\b""#;
    assert!(grammar_matches(grammar, "in"));
    assert!(grammar_matches(grammar, "else"));
    assert!(!grammar_matches(grammar, "el"));
    assert!(!grammar_matches(grammar, "loop"));
}
