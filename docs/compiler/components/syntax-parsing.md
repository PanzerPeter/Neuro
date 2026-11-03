# Syntax Parsing

**Crate**: `compiler/syntax-parsing`
**Entry Point**: `pub fn parse(source: &str) -> Result<Vec<Item>, ParseError>`

Syntax parsing turns one source file into the AST that every later stage reads. Read this page when you add syntax, change a desugaring, or trace a parse error back to its rule.

## Overview

Expressions go through a Pratt parser. Statements and declarations go through recursive descent. Several surface forms are rewritten during parsing, so the AST never carries them, and [CONTEXT.md](../../../compiler/syntax-parsing/CONTEXT.md) lists them. Parsing stops at the first error. The AST node types live in `ast-types`, so later slices read the tree without depending on this one.

## Architecture

- **Dependencies**: `lexical-analysis` (`parse` calls `tokenize` itself; the architecture test allows this one slice-to-slice edge), `ast-types`, `shared-types`, `thiserror`.
- **Public API**: `parse`, `ParseError`, and `parse_expr`. `parse_expr` parses a bare expression for tests. No slice or driver calls it. The AST types are re-exported for convenience.
- **Files**: [`src/parser/`](../../../compiler/syntax-parsing/src/parser/) has one module per construct (see [Code map](#code-map)). [`precedence.rs`](../../../compiler/syntax-parsing/src/precedence.rs) holds the binding powers, and [`errors.rs`](../../../compiler/syntax-parsing/src/errors.rs) defines `ParseError`.

### Code map

The [language reference](../../README.md#language-reference) documents the surface feature by feature. This table maps each construct to the parser file that reads it.

| Construct | Parsed by |
|---|---|
| Top-level dispatch (`func`, `struct`, `enum`, `trait`, `impl`, `const`, `newtype`, `type`, `import`, `module`, `@no_prelude`) | `parser/items.rs` |
| Functions, parameters, generic and lifetime lists | `parser/item_functions.rs` |
| Structs and their fields | `parser/item_structs.rs` |
| Enums and their variants | `parser/item_enums.rs` |
| Traits, `impl` blocks, methods, associated types | `parser/item_impls.rs` |
| `import` and `export import` forms | `parser/item_imports.rs` |
| Statements, bindings, control flow | `parser/statements.rs`, `stmt_loops.rs`, `stmt_assignments.rs` |
| `if` chains, `if val`, `while val` | `parser/conditionals.rs` |
| `val PATTERN = expr else { ... }` | `parser/stmt_val_else.rs` |
| Destructuring bindings | `parser/stmt_destructure.rs` |
| Match patterns | `parser/patterns.rs` |
| Expressions (Pratt) | `parser/expressions.rs`, `expr_prefix.rs`, `expr_infix.rs`, `expr_turbofish.rs`, `expr_index.rs` |
| Index and tensor-slice arguments | `parser/expr_index.rs` |
| String interpolation holes | `parser/interpolation.rs` |
| Type syntax | `parser/types.rs` |
| `type` aliases | `parser/type_aliases.rs` |

## Behavior

### AST shape

[`ast-types`](../../../compiler/infrastructure/ast-types/src/) defines the node types. This section says what the tree covers.

- **Items**: functions and methods, structs, enums, traits, `impl` blocks, constants, newtypes, imports, inline `module` blocks, and the file-level `@no_prelude` marker that module resolution consumes. Type aliases never reach the tree, because parsing expands them.
- **Statements**: bindings (`val`, `mut`), assignments (plain, compound, field, index and dereference targets), `return`, `break`, `continue`, `if`, `while`, `for`, `loop`, `match`, destructuring bindings, `val ... else`, and bare expressions. A plain `if` in statement position is always `Stmt::If`, never an expression statement. That is why the type checker and HIR lowering each treat a trailing `Stmt::If` as the value of its block. `if val` and `while val` have no node of their own. They parse to the `match` they mean.
- **Expressions**: literals, names and `Type::member` paths, operators, calls with optional turbofish arguments, indexing (including tensor indexing), field access, struct and enum literals, tuples and arrays, ranges, casts, closures, blocks, `unsafe` and `pool` blocks, `if`, `match`, `loop`, the `?` operator, and `Paren`, which lowering drops.

### Desugaring

These rewrites happen during parsing, so no later slice sees the sugar:

- `value |> f` becomes the call it names. `f >> g` becomes a closure that applies `f` and then `g`.
- `if val` and `while val` become `match`.
- An `impl Trait` parameter becomes an anonymous generic parameter. A return-position `impl Trait` is kept for semantic analysis.
- A destructuring binding expands into one binding per leaf.
- Trait default methods are copied into the `impl` blocks that omit them. `Self` inside an `impl` is substituted, and type aliases are expanded.
- The `.enumerate()`, `.rev()`, `.step(n)`, `.map(f)` and `.filter(p)` calls on a `for` head become fields of the loop.
- Struct field shorthand and the `..base` update are normalised.

Compound assignment is the exception that stays put. `place op= rhs` remains a `Stmt::Assign` with an operator, because choosing between `place = place op rhs` and an in-place update depends on types.

### Operator precedence

The binding powers are one table in [`precedence.rs`](../../../compiler/syntax-parsing/src/precedence.rs), and this page does not repeat it. Rules that readers hit:

- `|>` is the loosest operator and `.` the tightest. `@` shares a level with `*`, `/` and `%`, and `as` binds tighter than `@`, so `a @ b as f32` casts the right operand.
- The comparison operators share one level and associate to the left. `a < b == c` is `(a < b) == c`, and the checker rejects the chain with `comparison operators cannot be chained: use `&&` to combine separate comparisons`.
- `&`, `|` and `^` bind tighter than the comparisons, so `x & 3 == 2` is `(x & 3) == 2`.
- `??` associates to the right, so `a ?? b ?? c` evaluates a fallback only when every value before it was absent.
- `?` is postfix at call level: `f(x)? + 1` adds 1 to the unwrapped value.
- `>>` is composition, not shift. It is two adjacent `>` tokens and binds tighter than `|>`, so `x |> f >> g` applies the composed function to `x`. Right shift is the `.shr(n)` method.

### Statement boundaries

A newline ends a statement unless the line that ended asks to continue. A line continues when it ends with a binary operator, a comma or an opening delimiter. Inside an unclosed `(` or `[`, every line continues. A line that begins with `(`, `[`, `*`, `-`, `&`, `|` or `@` starts a new statement after a complete line, because each of those can also begin an expression. A line that begins with `|>` or `.` continues the one above, which is how multi-line pipelines and method chains are written.

Nothing may follow a complete statement on its line except the `}` that closes its block. `val x = 5 6` is an error, not a binding of `5`.

### Ambiguities the grammar resolves

- **A struct literal in a header.** In the header of an `if`, `while`, `for` or `match`, an identifier followed by `{` opens the body, so a struct literal must be parenthesised there. `if (Point { x: 1 }).x == 1 {` parses, and `if Point { x: 1 }.x == 1 {` does not.
- **`&&` in a type or prefix position** is two borrows: `&&i32` reads as `& &i32`.
- **`[` after a type name.** `Tensor<f32, [2, 3]>` takes a shape, because an integer, `?` or an immediate `]` cannot open a type. Elsewhere `[T; N]` and `[T]` are told apart by what follows the element type.
- **Prefix and infix `&` and `*`** are separated by position alone. A leading `*` in statement position is a dereference, or the target of an assignment.
- **`.enumerate()` and the other loop adapters** in a `for` head are loop syntax, not method calls. A range has no value to be a method's receiver.
- **`val PATTERN = e else`** is a `val ... else` binding only when the pattern carries a payload, as in `val Shape::Circle { r } = s else { ... }`. `val Point { x, y } = p` remains a struct destructure.
- **`as`** is a cast in an expression and a rename in an import, depending on what follows it.

The full list, with the reasoning, is in [CONTEXT.md](../../../compiler/syntax-parsing/CONTEXT.md).

### Nesting limit

Expressions, statements, types, patterns and `module` blocks share one depth counter. Nesting past 256 levels of them combined is `MaxDepthExceeded`, which stops the parse before the recursion overflows the stack. A new recursive production must enter the same counter.

## Diagnostics

`ParseError` covers token-level failures and the grammar rules that are cheapest to check while parsing. `ParseError::span()` returns the span of the offending token. `UnexpectedEof` and `MaxDepthExceeded` have no token to point at, so `UnexpectedEof` is shown at the end of the file. A lexical error reaches the parser wrapped, and it keeps its own span.

The driver renders a parse error as it renders a type error:

```text
error: unexpected token RightBrace, expected expression
 --> bad.nr:3:1
  |
3 | }
  | ^

Error: Parsing failed
```

Messages a user hits most often:

| Source | Message |
|---|---|
| `val x = 5 6` | `unexpected token Integer(6), expected a newline or '}' to end the statement` |
| `if Point { x: 1 }.x == 1 {` | `unexpected token Colon, expected a newline or '}' to end the statement` (the struct-header rule above) |
| `val a = 42f32` | `unexpected token Identifier("f32"), expected a newline or '}' to end the statement` (write `42.0f32`) |

Parsing stops at the first error. Parser error recovery is on the [Quick Roadmap](../../../README.md#quick-roadmap).

## Testing

- [`compiler/syntax-parsing/tests/`](../../../compiler/syntax-parsing/tests/) has one file per subject. They drive `parse` from source strings, and `error_tests.rs` drives `parse_expr` on fragments.
- A few parser modules carry unit tests of their own.
- The end-to-end programs are in `compiler/neurc/tests/suite/`.

The tests in `tests/` carry no link to the code they exercise, so run the crate's suite after any grammar change.

## Source

- [`compiler/syntax-parsing/src/lib.rs`](../../../compiler/syntax-parsing/src/lib.rs)
- [`compiler/syntax-parsing/CONTEXT.md`](../../../compiler/syntax-parsing/CONTEXT.md): parse-time desugars, the ambiguities the grammar resolves, and why

## See Also

- [Lexical Analysis](lexical-analysis.md)
- [Module Resolution](module-resolution.md)
- [Operators reference](../../language-reference/operators.md)
- [Simple but Powerful Pratt Parsing](https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html)
