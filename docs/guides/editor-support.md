# Editor Support

Syntax highlighting for `.nr` files ships in the repository under [`neuro-language-support/`](../../neuro-language-support/). There is no Language Server yet; it is on the [Quick Roadmap](../../README.md#quick-roadmap).

## VS Code

The extension highlights `.nr` files and sets comment, bracket and auto-closing behavior from `language-configuration.json`. Package it and install it:

```bash
cd neuro-language-support
npm install -g @vscode/vsce      # once
vsce package                     # -> neuro-language-support-<version>.vsix
code --install-extension neuro-language-support-*.vsix --force
```

Run `Developer: Reload Window` afterwards. A grammar change does not reach editors that are already open.

## Working on the grammar

Symlink the folder into `~/.vscode/extensions/` instead of repackaging on every edit. A window reload then picks up each change.

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
