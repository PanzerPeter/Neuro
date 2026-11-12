# Editor Support

Syntax highlighting for `.nr` files ships in the repository under [`neuro-language-support/`](../../neuro-language-support/). There is no Language Server yet; it is on the [Quick Roadmap](../../README.md#quick-roadmap).

## VS Code

The extension highlights `.nr` files and sets comment, bracket and auto-closing behavior from `language-configuration.json`. It is listed in the Extensions view as **Neuro Language Support** (publisher `PanzerPeter`). No build or packaging step is needed: open the Extensions view, search for `Neuro Language Support`, and make sure it is installed and enabled.

Run `Developer: Reload Window` after a grammar change. Editors that are already open do not pick up a new grammar on their own.

## Working on the grammar

The grammar is `neuro-language-support/syntaxes/neuro.tmLanguage.json`, and it has no structural link to the lexer. `compiler/lexical-analysis/tests/tmlanguage_sync.rs` is that link. It checks three things:

- every keyword the lexer tokenizes appears in the grammar;
- the grammar's `#keywords` rule lists only words the lexer tokenizes;
- the declaration rules come before `#keywords` in the top-level `patterns` array, because TextMate resolves a tie between two rules at the same offset by position.

Run it with `cargo test -p lexical-analysis --test tmlanguage_sync`. The test does not cover string bodies, escapes, interpolation holes, number shapes, `#types` or `#constants`, so change those by hand. To see the scopes the grammar assigns to a file, install `vscode-textmate` and `vscode-oniguruma` outside the repository, then run `node tools/tmlanguage_scopes.mjs <file.nr>` from that directory.

## Other editors

The grammar is a TextMate grammar in JSON form, with scope name `source.neuro`. Any tool that loads TextMate grammars can use it, together with `language-configuration.json` for comments and brackets. Consult your editor's documentation for how to register a language from a grammar file.

## See also

- [Troubleshooting](troubleshooting.md#vscode-syntax-highlighting-not-working)
- [Lexical analysis](../compiler/components/lexical-analysis.md)
