# Syntax Parsing

**Crate**: `compiler/syntax-parsing`
**Entry Point**: `pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>`

## Overview

Syntax parsing turns one source file into the AST that every later stage reads. Expressions go
through a Pratt parser, which encodes the precedence ladder below in one table; statements and
declarations go through recursive descent. Several surface forms are desugared here and never
reach the AST: the [`CONTEXT.md`](../../../compiler/syntax-parsing/CONTEXT.md) lists them.

## Architecture

- **Dependencies**: `lexical-analysis` (`parse` calls `tokenize` itself), `ast-types` (the node
  types it builds), `shared-types`, `thiserror`. The dependency on `lexical-analysis` is the one
  slice-to-slice edge the architecture tests allow.
- **Public API**: `parse`, and `parse_expr` for tests and tooling (the driver never calls it),
  plus `ParseError`. The AST types are re-exported from `ast-types` for convenience.
- **Internally**: `parser/`, one module per construct (see the table under
  [Grammar to code](#grammar-to-code)), and `precedence.rs`.

## Behavior

### AST shape

The node set itself lives in the `ast-types` infrastructure crate, not here: that is what
lets semantic analysis, module resolution, and HIR lowering read the tree without depending
on this slice. This section describes the shape; the definitions are in
[`compiler/infrastructure/ast-types/src/`](../../../compiler/infrastructure/ast-types/src/).

**Items** (`Item`), the top level of a file: `Function`, `Struct`, `Enum`, `Trait`, `Impl`,
`Const`, `Newtype`, `Import`, `Module` (an inline `module { }` block), and `NoPrelude` (the
file-scope `@no_prelude` marker, consumed by module resolution). A function or struct carries
its generic parameters and lifetimes; an `impl` carries an optional trait name, so the
inherent and trait forms are one node. A method's receiver is `Option<SelfParam>`, absent for
an associated function, otherwise `&self`, `&mut self`, or owned `self`.

**Statements** (`Stmt`): bindings (`val` / `mut`), assignment and compound assignment, field
and index assignment, dereference assignment, `return`, `break`, `continue`, `if`, `while`,
`for`, `loop`, `match`, destructuring bindings, `val-else`, and a bare expression. An `if` in
statement position always parses to `Stmt::If`, never `Stmt::Expr(Expr::If)`, which is why the
type checker and HIR lowering each recognise a trailing `Stmt::If` as a block's value.

**Expressions** (`Expr`): literals, identifiers, unary and binary operators, calls (with
optional turbofish type arguments), index, field access, `Type::member` paths, struct and enum
literals, tuples and arrays, ranges, casts, closures, blocks, `unsafe` blocks, `if`, `match`,
`loop`, the `?` propagation operator, and `Paren` grouping (dropped during lowering).

### Operator precedence

The parser is a **Pratt parser** (precedence climbing). The ladder, loosest first:

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 (loosest) | `\|>` (pipeline) | Left |
| 2 | `>>` (function composition) | Left |
| 3 | `..`, `..=` (range) | Left |
| 4 | `??` (null-coalescing) | Right |
| 5 | `\|\|` | Left |
| 6 | `&&` | Left |
| 7 | `\|` (bitwise or) | Left |
| 8 | `^` | Left |
| 9 | `&` (bitwise and) | Left |
| 10 | `==`, `!=` | Left |
| 11 | `<`, `>`, `<=`, `>=` | Left |
| 12 | `<<` | Left |
| 13 | `+`, `-` | Left |
| 14 | `*`, `/`, `%` | Left |
| 15 | `@` (matrix multiplication) | Left |
| 16 | `as` (cast) | Left |
| 17 | `-`, `!`, `~` (unary) | Right |
| 18 | call `f(...)`, index `a[i]`, `?`, turbofish `::<...>` | Left |
| 19 (tightest) | `.` (field / method access) | Left |

`>>` composes functions rather than shifting bits: right shift is the `.shr(n)` method. It is
not a token, but two adjacent `>`, so a nested generic type still closes with two ordinary
`>` and only the expression parser reads the pair as one operator. `??` associates
right-to-left so `a ?? b ?? c` evaluates each fallback only when every left-hand side before
it was absent.

`@` spells both the matmul operator and the opening of an attribute. Attributes are read at
item level only, so the two never compete except across a newline: a line beginning with `@`
is therefore a statement boundary, alongside one beginning with `(`, `[` or `*`, and a module
`const`'s initializer does not swallow the `@derive` written under it.

```neuro
a + b * c       // a + (b * c)
a < b == c < d  // (a < b) == (c < d)
!a && b         // (!a) && b
f(x)? + 1       // (f(x)?) + 1
```

**Statement boundaries.** A newline ends a statement unless the line that just ended asks to
continue (it ends with a binary operator, a comma, or an opening delimiter) or the expression
is inside an unclosed `(`, `[`, or `{`. The decision belongs to the line that ended, so a line
*starting* with `(`, `[`, or `*` opens a new statement rather than continuing the one above as
a call, an index, or a multiplication.

## Errors

`ParseError` (see [`errors.rs`](../../../compiler/syntax-parsing/src/errors.rs)) covers the
token-level failures (`UnexpectedToken`, `UnexpectedEof`, a wrapped `LexError`, and
`MaxDepthExceeded`, which stops runaway nesting rather than overflowing the stack) plus the
grammar rules that are cheapest to enforce while parsing: `DuplicateParameter`,
`DuplicateTypeAlias`, `TypeAliasShadowsBuiltin`, `CyclicTypeAlias`, `EnumLifetimeParam`,
`ExportNotAllowed`, and `MisplacedNoPrelude`. Each carries the span of the offending token,
not the start of the enclosing construct, except `UnexpectedEof` and `MaxDepthExceeded`, which
have no single token to point at.

Parsing stops at the first error; reporting several per run is the type checker's job, and
parser error recovery is planned on the roadmap. `ParseError::span()` hands the span to the
driver, which renders a parse error the way it renders a type error. `UnexpectedEof` is pointed
at the end of the file:

```text
error: unexpected token RightBrace, expected expression
 --> bad.nr:3:1
  |
3 | }
  | ^

Error: Parsing failed
```

## Grammar to code

The language surface is documented feature by feature in the
[language reference](../../README.md#language-reference), where every construct is shown in a
program that compiles. That is the grammar's source of truth. This table maps it to the parser:

| Construct | Parsed by |
|---|---|
| Top-level dispatch (`func`, `struct`, `enum`, `trait`, `impl`, `const`, `newtype`, `type`, `import`, `module`, `@no_prelude`) | `parser/items.rs` |
| Functions, parameters, generic and lifetime lists | `parser/item_functions.rs` |
| Structs and their fields | `parser/item_structs.rs` |
| Enums and their variants | `parser/item_enums.rs` |
| Traits, `impl` blocks, methods, associated types | `parser/item_impls.rs` |
| `import` / `export import` forms | `parser/item_imports.rs` |
| Statements, bindings, control flow | `parser/statements.rs`, `stmt_loops.rs`, `stmt_assignments.rs` |
| `val PATTERN = expr else { ... }` | `parser/stmt_val_else.rs` |
| Destructuring bindings | `parser/stmt_destructure.rs` |
| Match patterns | `parser/patterns.rs` |
| Expressions (Pratt) | `parser/expressions.rs`, `expr_prefix.rs`, `expr_infix.rs`, `expr_turbofish.rs`, `expr_index.rs` |
| Index and tensor-slice arguments | `parser/expr_index.rs` |
| String interpolation holes | `parser/interpolation.rs` |
| Type syntax | `parser/types.rs` |
| `type` aliases | `parser/type_aliases.rs` |

## Testing

A few parser modules carry unit tests of their own; `compiler/syntax-parsing/tests/`, split
by subject, drives the parser from source strings. Those tests carry no link back to the code they exercise, so run
the crate's suite after any grammar change.

## Source

- [`compiler/syntax-parsing/src/lib.rs`](../../../compiler/syntax-parsing/src/lib.rs)
- [`compiler/syntax-parsing/CONTEXT.md`](../../../compiler/syntax-parsing/CONTEXT.md): parse-time
  desugars, the ambiguities the grammar resolves, and why

## See Also

- [Lexical Analysis](lexical-analysis.md)
- [Module Resolution](module-resolution.md)
- [Operators reference](../../language-reference/operators.md)
- [Simple but Powerful Pratt Parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
