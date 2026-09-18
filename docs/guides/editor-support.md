# Editor Support

Syntax highlighting for `.nr` files ships in the repository under
[`neuro-language-support/`](../../neuro-language-support/). There is no Language Server yet;
that is Phase 8 in the [Quick Roadmap](../../README.md#quick-roadmap).

## VS Code

```bash
cd neuro-language-support
npm install -g @vscode/vsce      # once
vsce package                     # -> neuro-language-support-<version>.vsix
code --install-extension neuro-language-support-*.vsix --force
```

Reload the window afterwards (`Developer: Reload Window`). A grammar change does not reach
editors that are already open.

## Working on the grammar

Symlink the folder into `~/.vscode/extensions/` instead of repackaging on every edit; a window
reload then picks up each change.

The grammar and the lexer must agree on the keyword set. That is not a convention, it is a test:
`compiler/lexical-analysis/tests/tmlanguage_sync.rs` fails when a keyword is added to one and not
the other.

## Other editors

The grammar is a standard TextMate `.tmLanguage.json` file, so any editor that reads TextMate
grammars (Sublime Text, Zed, the Monaco-based editors) can load it directly from
`neuro-language-support/syntaxes/`.
