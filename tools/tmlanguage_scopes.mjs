#!/usr/bin/env node
// Print the TextMate scopes the editor grammar assigns to a Neuro source file.
//
// The grammar in `neuro-language-support/syntaxes/neuro.tmLanguage.json` is a pile of
// regexes whose behaviour cannot be read off the file: TextMate resolves two rules
// matching at the same offset by their position in the `patterns` array, so a rule can
// be dead without looking wrong. This runs the same tokenizer VS Code runs and shows
// what each rule actually produced.
//
// `compiler/lexical-analysis/tests/tmlanguage_sync.rs` covers the two failure modes
// that can be checked without a tokenizer (a missing keyword, an invented one, a
// declaration rule ordered behind `#keywords`). This tool is for everything else —
// escapes, interpolation holes, number shapes — and is deliberately not wired into CI,
// so the Rust workspace keeps no Node dependency.
//
//   mkdir -p /tmp/tmscopes && cd /tmp/tmscopes
//   npm init -y && npm install vscode-textmate vscode-oniguruma
//   node <repo>/tools/tmlanguage_scopes.mjs <repo>/examples/showcase/generic_toolkit.nr
//
// Every token is printed as `text  scope…`; a token with no scope prints `(none)`,
// which is the usual sign of a rule that never ran.

import fs from 'fs';
import path from 'path';
import { createRequire } from 'module';
import { fileURLToPath } from 'url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

// Resolve the tokenizer from the *current working directory*, not from this file, so
// the dependencies can be installed in a scratch directory and never land in the repo.
const requireFromCwd = createRequire(path.join(process.cwd(), 'package.json'));
let oniguruma;
let vsctm;
try {
  oniguruma = requireFromCwd('vscode-oniguruma');
  vsctm = requireFromCwd('vscode-textmate');
} catch {
  console.error(
    'vscode-textmate / vscode-oniguruma not found.\n' +
      'Install them somewhere scratch and run this from there:\n' +
      '  mkdir -p /tmp/tmscopes && cd /tmp/tmscopes\n' +
      '  npm init -y && npm install vscode-textmate vscode-oniguruma\n' +
      '  node <repo>/tools/tmlanguage_scopes.mjs <repo>/examples/hello.nr',
  );
  process.exit(2);
}

const GRAMMAR = process.env.NEURO_GRAMMAR
  ?? path.join(HERE, '..', 'neuro-language-support/syntaxes/neuro.tmLanguage.json');

const target = process.argv[2];
if (!target) {
  console.error('usage: node tools/tmlanguage_scopes.mjs <file.nr>');
  process.exit(2);
}

await oniguruma.loadWASM(
  fs.readFileSync(requireFromCwd.resolve('vscode-oniguruma/release/onig.wasm')).buffer,
);

const registry = new vsctm.Registry({
  onigLib: Promise.resolve({
    createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
    createOnigString: (source) => new oniguruma.OnigString(source),
  }),
  loadGrammar: async (scope) =>
    scope === 'source.neuro'
      ? vsctm.parseRawGrammar(fs.readFileSync(GRAMMAR, 'utf8'), GRAMMAR)
      : null,
});

const grammar = await registry.loadGrammar('source.neuro');
let stack = vsctm.INITIAL;

for (const line of fs.readFileSync(target, 'utf8').split('\n')) {
  const result = grammar.tokenizeLine(line, stack);
  for (const token of result.tokens) {
    const text = line.substring(token.startIndex, token.endIndex);
    if (!text.trim()) continue;
    const scopes = token.scopes.filter((s) => s !== 'source.neuro');
    console.log(JSON.stringify(text).padEnd(20), scopes.join(' ') || '(none)');
  }
  stack = result.ruleStack;
}
