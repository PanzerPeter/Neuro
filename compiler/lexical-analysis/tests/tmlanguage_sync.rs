//! Keeps the editor's TextMate grammar in sync with the lexer.
//!
//! `neuro-language-support/syntaxes/neuro.tmLanguage.json` is a hand-written regex
//! grammar with no structural link to `TokenKind` — nothing else in the workspace
//! fails when a keyword is added to the lexer and not to the grammar. These tests are
//! that link.
//!
//! Three properties are asserted, each standing for a bug class the grammar has
//! actually shipped:
//!
//! 1. **Coverage** — every `#[token("word")]` keyword appears in the grammar.
//! 2. **No invention** — the grammar's `#keywords` rule lists *only* words the lexer
//!    tokenizes, so a keyword planned for a later phase cannot be highlighted as if
//!    the compiler already accepted it.
//! 3. **Reachability** — the rules that name a declaration precede `#keywords` in the
//!    top-level `patterns` array. TextMate breaks a tie between two rules matching at
//!    the same offset by array position, never by match length, so `#keywords` listed
//!    first silently makes every `func f` / `struct S` rule dead code. Nothing about
//!    such a grammar looks wrong on inspection; only the order gives it away.
//!
//! Reading the lexer's source text (rather than reflecting over `TokenKind`) is
//! deliberate — logos consumes the attributes at compile time, so the literals are not
//! observable at runtime any other way, and a source scan needs no upkeep when a
//! keyword lands.
//!
//! Scope note: these cover keyword *words* and rule order. Grammar rules with no
//! one-to-one token counterpart — string bodies, escapes, interpolation holes, number
//! literal shapes — must still be updated by hand, and the names in `#types` and
//! `#constants` answer to the prelude and the type checker rather than to the lexer.
//! To inspect what the grammar actually produces, run `tools/tmlanguage_scopes.mjs`.

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

const GRAMMAR_PATH: &str = "neuro-language-support/syntaxes/neuro.tmLanguage.json";
const TOKENS_PATH: &str = "compiler/lexical-analysis/src/tokens.rs";

fn grammar_json() -> serde_json::Value {
    serde_json::from_str(&read(GRAMMAR_PATH)).expect("the TextMate grammar is valid JSON")
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

/// The identifier-shaped alternatives of every `match` regex under `repository.<rule>`,
/// e.g. `\b(if|else|while)\b` yields `if`, `else`, `while`.
fn words_in_rule(grammar: &serde_json::Value, rule: &str) -> Vec<String> {
    let patterns = grammar["repository"][rule]["patterns"]
        .as_array()
        .unwrap_or_else(|| panic!("grammar repository has no `{rule}` rule with `patterns`"));

    let mut out = Vec::new();
    for pattern in patterns {
        let Some(regex) = pattern["match"].as_str() else {
            continue;
        };
        for word in regex.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            // `b` is the `\b` word-boundary escape, not an alternative.
            if word.len() > 1 && word.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                out.push(word.to_string());
            }
        }
    }
    out
}

/// The order of `{ "include": "#name" }` entries in the grammar's top-level
/// `patterns` array. Position in this array is what decides which of two rules
/// matching at the same offset wins.
fn top_level_include_order(grammar: &serde_json::Value) -> Vec<String> {
    grammar["patterns"]
        .as_array()
        .expect("the grammar has a top-level `patterns` array")
        .iter()
        .filter_map(|p| p["include"].as_str())
        .map(str::to_string)
        .collect()
}

fn position_of(order: &[String], include: &str) -> usize {
    order
        .iter()
        .position(|entry| entry == include)
        .unwrap_or_else(|| {
            panic!("the grammar's top-level `patterns` no longer includes `{include}`")
        })
}

#[test]
fn textmate_grammar_covers_every_lexer_keyword() {
    let tokens = read(TOKENS_PATH);
    let grammar = read(GRAMMAR_PATH);

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
        "lexer keywords absent from {GRAMMAR_PATH}: {missing:?}\nAdd them to the matching \
         `keywords` or `constants` pattern — editor highlighting has no other link to the lexer."
    );
}

#[test]
fn textmate_keywords_rule_invents_nothing() {
    let tokens = read(TOKENS_PATH);
    let grammar = grammar_json();

    let lexed = lexer_keywords(&tokens);
    let invented: Vec<String> = words_in_rule(&grammar, "keywords")
        .into_iter()
        .filter(|word| !lexed.contains(word))
        .collect();

    assert!(
        invented.is_empty(),
        "the grammar's `#keywords` rule highlights words the lexer does not tokenize: \
         {invented:?}\nA word highlighted as a keyword tells the reader the compiler accepts \
         it. Reserve a future phase's vocabulary in the roadmap, not in the editor."
    );
}

#[test]
fn declaration_rules_outrank_the_keyword_rule() {
    let grammar = grammar_json();
    let order = top_level_include_order(&grammar);
    let keywords_at = position_of(&order, "#keywords");

    // Each of these begins with a keyword the `#keywords` rule also matches, at the same
    // offset. Listed after `#keywords`, it becomes unreachable and the declared name
    // loses its scope entirely.
    for rule in ["#function_declaration", "#type_declarations", "#imports"] {
        assert!(
            position_of(&order, rule) < keywords_at,
            "`{rule}` is listed after `#keywords` in the grammar's top-level `patterns`. \
             TextMate resolves a same-offset tie by array position, so `#keywords` claims \
             the leading keyword and `{rule}` never runs."
        );
    }

    // `'a'` and `'a` start identically; `#lifetimes` listed first swallows the opening
    // quote of every character literal.
    assert!(
        position_of(&order, "#chars") < position_of(&order, "#lifetimes"),
        "`#chars` must precede `#lifetimes`, or a character literal is scoped as a lifetime."
    );

    // A capitalised name is a type before it is a call target, and an all-caps name is a
    // constant before it is a type.
    assert!(
        position_of(&order, "#pascal_types") < position_of(&order, "#function_calls"),
        "`#pascal_types` must precede `#function_calls`."
    );
    assert!(
        position_of(&order, "#screaming_constants") < position_of(&order, "#pascal_types"),
        "`#screaming_constants` must precede `#pascal_types`."
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
