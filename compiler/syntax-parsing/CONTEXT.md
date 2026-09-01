# syntax-parsing

## Purpose
Transform a Neuro token stream into a typed Abstract Syntax Tree for later compiler stages.

## Entry Point
- Type: Library function
- Input: `source: &str`
- Output: `Result<Vec<Item>, ParseError>`

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- ast-types — owns the AST node definitions, so semantic-analysis and the backends consume the
  tree without depending on this slice
- shared-types — `Span`, `Identifier`, `Literal`, `FormatSpec` used throughout the grammar
- lexical-analysis — direct consumer; `parse()` calls `tokenize()` internally

The `lexical-analysis` dependency is deliberate intra-pipeline coupling, not a VSA violation:
syntax-parsing is the sole token-stream consumer, and externalising tokenisation would add an
unnecessary neurc coordination step. The architecture test allowlists this pairing.

## Notes

### Standing obligation when adding a node
A Pratt parser plus two whole-program walkers means every new `Expr` / `Stmt` / `Type` variant
has three homes, and missing one fails silently rather than loudly:
1. the parser production itself;
2. `stmt_span` (for a `Stmt`), so diagnostics can point at it;
3. the alias-substitution walker in `parser/type_aliases.rs`, or an aliased type inside the new
   node never expands.

### Parse-time desugars: what never reaches ast-types
These run before any other slice sees the tree, so downstream passes never learn the sugar
existed:
- **Compound assignment** — `target OP= rhs` becomes
  `Stmt::Assignment { target, value: Expr::Binary { target, OP, rhs } }`, detected by one-token
  lookahead in `parse_statement`.
- **Type aliases** — collected separately from `items`, then `expand_type_aliases`
  (`parser/type_aliases.rs`) resolves alias chains (rejecting cycles, duplicates, and built-in
  shadows) and substitutes every aliased annotation across items/statements/expressions,
  preserving the use-site span. Scope is type-annotation positions only (var / const / param /
  return / field / cast); an alias as a value constructor or path name is out of scope. An
  unknown target hits the existing `UnknownTypeName` check downstream.
- **Trait default methods** — `inject_trait_defaults` copies each trait's defaults into the
  `impl Trait for Type` blocks that omit them, never replacing a method the implementor wrote.
  It runs *before* `expand_type_aliases`, so injected bodies are alias-expanded too. Downstream
  passes therefore see trait methods as ordinary inherent methods.
- **Argument-position `impl Trait`** — `parse_function` rewrites each occurrence (including
  nested under `&`/`&mut`, arrays, and tuples) into a fresh anonymous generic parameter
  `__implN: Trait` appended to the function's `generics`. Static dispatch then reuses the
  existing monomorphization machinery unchanged, and each `impl Trait` parameter is
  independently inferred. Return-position `impl Trait` is deliberately **not** desugared — it is
  one concrete type chosen by the body, not a caller-inferred parameter — and is resolved by
  semantic-analysis.
- **Destructuring binds** — `parse_stmt_into` detects `val`/`mut` followed by a tuple `(`, array
  `[`, or struct `Name {` pattern and routes to `parse_destructure_bind`, which binds the RHS to
  a `__destructure_N` temp and expands one projection per leaf through a parse-local
  `DestructurePattern` (supporting `_` wildcards and nesting). Struct shorthand fields become
  `FieldAccess` binds; array elements become `Index` binds. A trailing rest expands to
  `Expr::ArrayRest { exact: false }`, and a rest-less array pattern adds a discarded
  `ArrayRest { exact: true }` as an arity assertion.
- **Struct literal shorthand and functional update** — a field with no `: value` desugars to
  `field: field`; a trailing `..expr` sets `StructLiteral.base` and ends the field list.

`newtype` is the deliberate counter-example: unlike a `type` alias it is a distinct nominal type,
so it stays an `Item::Newtype`. Construction `Name(value)` reuses the call parse and `.0` reuses
the tuple-index parse, so it needs no expression grammar of its own.

### Ambiguities the grammar resolves, and how
- **Struct literal vs. block body.** `Parser` carries `no_struct_lit: bool`, raised while parsing
  a guarded header — an `if`/`else if`/`while` condition, a `for` iterable, a `match` scrutinee, a
  match-arm guard, a `where`-clause value predicate — so `Identifier { ... }` does not consume the
  `{` opening the body block (Rust's strategy). Two scoped helpers own the flag and nothing else
  assigns it: `guarded_header` raises it, `inside_delimiters` lifts it for `( ... )` and `[ ... ]`
  (grouping, tuple literals, call and turbofish argument lists, array literals, index brackets),
  where a `{` cannot be a body block. Both restore the previous value on the error path too, so
  nesting composes — `if check(Point { x: 1 }) && flag { }` reads the literal inside the argument
  list and the trailing brace as the body.
- **Statement boundaries.** `parse_expr_inner` treats a newline followed by `(`, `[`, or `*` as a
  statement boundary. The rule is that **the line that ENDED decides**: a continuing line ends
  with an operator, a comma, or an opening delimiter, all of which arrive here with no pending
  newline. Consulting the *following* token instead inverted it — `val a = f()` followed by a line
  `(2 + 3)` parsed as `f()(2 + 3)`, and a following `[1, 2]` as an index, both of which
  type-checked with a callable on the left. A leading `.` still continues, since it cannot start a
  statement.
- **Array type vs. slice type.** `[T; N]` and `[T]` share their opening bracket, so `parse_type`
  parses the element type first and then looks at what follows: a `]` closes an unsized
  `Type::Slice`, anything else must be the `;` of a sized `Type::Array`. Whether the slice is
  legal where it was written is not decided here — semantic analysis rejects one outside a
  reference, exactly as it does for a bare `dyn Trait`.
- **Prefix vs. infix `&` and `*`.** Purely parser position: prefix `&` is a borrow
  (`Expr::Reference`, operand at `Precedence::Unary`), infix `&` is `BinaryOp::BitAnd`; prefix
  `*` is `Expr::Deref`, infix `*` is multiply. A leading `*` in statement position is a deref
  expression statement, or a `Stmt::DerefAssignment` when followed by `=`.
- **`as` is both a cast and an import rename.** An import reads it as a rename marker only when
  an identifier follows; a cast keeps its meaning.
- **`val-else` vs. a plain binding or a destructure.** `starts_val_else` fires on an `Identifier`
  followed by `ColonColon` or `LeftParen` after the keyword. That two-token marker is unambiguous
  because a binding name is only ever followed by `:`, `=`, or a newline, and a payload therefore
  makes the reading certain. It is checked *ahead of* `starts_destructure_pattern`, so
  `val Point { x, y } = p` still desugars as a struct destructure while
  `val Shape::Circle { r } = s else { ... }` parses as a `val-else`. A `val Name {` stays a struct
  destructure and a payload-less `val None = ...` stays a binding, both because a bare `None`
  pattern is ambiguous until module resolution. The `|name|` after `else` is a dedicated
  production, not a closure literal.
- **`.enumerate()` in a `for` head is grammar, not a method call.** `parse_for_stmt` reads the
  head as one loop variable or the pair `(index, value)`, then strips a trailing `.enumerate()`
  off the iterable into `Stmt::ForRange`/`ForEach`'s `index` field. It cannot be left as an
  expression for semantic analysis to resolve: a range has no value form to be the receiver of a
  method, and there is no iterator type for one to return. A pair head and `.enumerate()` imply
  each other, so either alone is a parse error (`PairWithoutEnumerate` / `EnumerateWithoutPair`).
  `(0..n).enumerate()` must be parenthesised — `..` binds looser than a call, so the parenthesised
  range is unwrapped here into the range loop's bounds, and `for i in (0..n)` follows from the
  same unwrap.
- **`@no_prelude` vs. an attribute list.** `parse_item_list` checks for the marker *before*
  reading attributes, because an attribute list is claimed by the declaration that follows it and
  this marker has none. It is accepted only as the first thing in a file — not after a
  declaration, and not inside a `module { }` block, which is not a file — reported as
  `ParseError::MisplacedNoPrelude`.
- **Nested generics close cleanly.** `>` closes an argument list one token at a time; nested
  `Foo<Bar<T>>` lexes as two separate `>` tokens, never a `>>`. The `::<` turbofish check wins at
  every step, so `f::<i32>(x)` and `math::helper::<i32>(x)` both parse.
- **A valueless `return`/`break` may end at a comma.** `parse_match_arm` dispatches
  `Return`/`Break`/`Continue` to `parse_stmt` and wraps the result in `Expr::Block` — exactly the
  node a braced arm already produced, so nothing downstream sees a new shape. A `Comma` therefore
  has to end a valueless `return`/`break`, which is safe because a comma cannot begin an
  expression (BUG-012).

### Items
`parse_program` delegates to `parse_item_list`, which runs to end of input at file level and to
the closing brace inside a `module Name { ... }` block, so a block body is parsed by the same code
as the file around it and blocks nest for free. Type-alias declarations from every nesting level
are collected into one list and expanded in a single pass, and trait-default injection walks
blocks too — both are erased at parse time, which is what makes an alias declared beside a block
still read inside it.

`export` is consumed between the attributes and the item keyword — the position `pub` takes in
Rust, so `@derive(Copy)` still reads as attached to the declaration — and per field inside
`parse_struct_def`, since an exported struct may keep a field to itself. It is rejected on an
`impl` block (which declares no name), on a `type` alias (expanded at parse time, so nothing of
it survives to reach another module), and on a `module` block (an inline module's name is reached
only from the file declaring it), all through `ParseError::ExportNotAllowed`. Nothing here
*enforces* visibility — the parser records what was written.

`parse_attributes` collects `@name` / `@name(arg, ...)` ahead of every `func` (free and impl
methods) and `struct`. Any attribute name is accepted, so future `@grad` / `@gpu` need no grammar
churn; semantics live in semantic-analysis. An attribute preceding neither is rejected
(`UnexpectedToken`).

`parse_import` (`parser/item_imports.rs`) reads all five surface forms into one `Item::Import`: an
optional leading `./`, a `::`-separated path, then an optional `{a, b as c}` list or a trailing
`as` alias.

`parse_impl_def` accepts both inherent `impl TypeName { … }` and trait `impl TraitName for
TypeName { … }` (a `for` after the first identifier selects the trait form). Its body loop takes
associated-type bindings `type Name = Type` alongside methods, collected into
`ImplDef.assoc_types`, which is how an operator-trait impl declares its `type Output = T`. Each
method goes through `parse_method_def`, which calls `try_parse_self_param` to detect
`&self` / `&mut self` / `self` before the parameter list.

`parse_trait_def` reads method signatures via `parse_trait_method_def`: a `{` after the return
type opens a default-method body, otherwise the method is required and ends at the newline.

### Parameters and call arguments
One `parse_parameter_list` serves `parse_function`, `parse_method_def`, and
`parse_trait_method_def`. It accepts the external-label forms — a second identifier before the `:`
is the internal name, making the first the call-site label, and a first identifier of `_`
suppresses the call-site name entirely. Two parameters answering to one call-site name is
`ParseError::DuplicateParameterLabel`, because a named argument must identify exactly one
parameter. Sharing the loop is also what gave methods and trait methods the duplicate-name check
that only free functions had.

`parse_call_arguments` returns the argument expressions plus a **parallel label list**, reading
`ident :` at the start of an argument as a label (`::` is one token, so a qualified path cannot be
mistaken for one) and clearing the list when nothing was named. The parser records what was
written and resolves nothing: matching a label to a parameter needs the callee, which is
`argument-binding`'s job.

### Expressions
- **Paths.** When `parse_prefix` sees `Identifier` `::`, it produces `Expr::Path`. A path may
  carry more than two segments now that modules exist (`utils::io::read`): the loop folds
  everything ahead of the final segment into one qualifier identifier whose `name` holds the `::`
  separators, and `parse_type` does the same for a qualified type annotation. No AST node
  changed — `module-resolution` splits the qualifier again, verifies it against the module that
  owns the name, and erases it, so no `::` name ever reaches the type checker.
- **Closures.** A leading `|` / `||` / `move` in prefix position parses as a closure literal.
  Parameters take an optional `: T`; an optional `-> R` follows; the body is a brace block or a
  single expression at `Precedence::Lowest`, so it stops at a `,`, `)`, or newline. `parse_type`
  reads a parenthesized type list followed by `->` as `Type::Function` (zero or more params),
  keeping the ≥2-element tuple form for a list with no arrow.
- **Match.** `parse_match_expr` parses the scrutinee with struct literals suppressed, then arms of
  the form `pattern ('|' pattern)* ('if' guard)? '=>' body`. `parse_pattern` reads
  wildcard / binding / literal / range / `E::V` variant patterns with `(tuple)` or `{ named }`
  payloads; a leading `-` on a numeric literal and the `..` / `..=` range forms are handled in
  `parse_pattern_literal`. A bare identifier followed by `(` or `{` becomes
  `Pattern::UnqualifiedEnum`, sharing the payload parser with the qualified form; a bare `None`
  stays `Pattern::Binding`, since nothing at parse time tells the two apart.
- **String interpolation.** `parser/interpolation.rs` turns the lexer's text/hole chunks into
  `Expr::InterpString`: each hole's raw source is re-lexed, its token spans shifted onto absolute
  file coordinates, and parsed by a nested `Parser` at `Precedence::Lowest` — so a hole may hold a
  call, a struct literal, or an `if`, and any diagnostic inside it points at the real file column.
  Anything after the expression must be `:` plus a specifier, which is read from the **raw text**
  rather than tokens (`.2`, `08d`, `<10` are not well-formed token sequences) and parsed into a
  `FormatSpec`. Errors: `EmptyInterpolationHole`, `InvalidFormatSpec`, and
  `InterpolationInPattern` — an interpolated literal is not a constant, so it cannot be a pattern.
- **Loops and labels.** `try_parse_labeled_loop` dispatches `ident : <for|while|loop>` to the
  matching loop parser (labels reuse the existing `Identifier` + `Colon` tokens, so no lexer
  change). `parse_loop_stmt` returns `Stmt::Expr(Expr::Loop { .. })`. In `parse_break_stmt` a
  trailing identifier is read as a label only when it names an in-scope loop
  (`Parser::active_labels`), otherwise it begins the value expression; `continue` reads an
  optional trailing same-line label with no newline skip, so a line-final `break` is never a
  labeled break.

### Precedence table facts worth knowing
`?` maps to `Precedence::Call` — postfix, binding as tightly as a call or index, so `f(x)? + 1`
adds to the unwrapped payload and `parse(s)?.field` reads a field of it. No new level was needed.
`??` sits at `Precedence::NullCoalesce`, between `Lowest` and `LogicalOr`, and gets its
right-to-left associativity by recursing on the right operand at `Precedence::Lowest`. `..` /
`..=` sit at `Precedence::Range`, below `??`; `parse_for` parses its range start bound at
`Precedence::Range` so the loop's own separator is not swallowed. Indexing `a[i]` is at call
precedence.

### Generics
`parse_generic_params` returns `(Vec<GenericParam>, Vec<Identifier>)` — a leading `Lifetime`
token in a `<...>` list is collected into a separate `lifetimes` vector, kept apart from
type/const generics because a lifetime does not monomorphize, with a duplicate-lifetime check.
It also accepts `const N: T` (setting `GenericParamKind::Const`). Bounds after `:` are
`+`-separated trait names, recorded but not enforced here. `parse_where_clause` folds trait
bounds into the matching parameter and collects value predicates. `parse_type`'s reference branch
parses an optional lifetime after `&` and before `mut` (`&'a T` / `&'a mut T`).

`parse_enum_def` takes an optional generic parameter list, but rejects a lifetime parameter with
`ParseError::EnumLifetimeParam`: enum payloads are scalars this phase, so there is nothing for a
lifetime to annotate.

`Type::Generic`'s span covers its **closing `>`** — `parse_generic_type_args` returns the closing
token's span alongside the arguments. When it ended at the last argument instead, every
diagnostic pointing at a generic type application underlined one byte short.
