# Changelog

All notable changes to the Neuro programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

## [2.1.4] - 2026-08-30

### Changed

- **Every slice's `CONTEXT.md` compacted and de-rotted.** The files had accumulated an
  append-only, dated "Recent Updates" log beside their contract sections — a changelog in a
  file that VSA defines as a contract, duplicating what `CHANGELOG.md` and git history already
  hold. Each file is now organized by subject and states current behaviour with the design
  rationale that the source cannot; the dates, the "added field X" announcements, and the
  historical narration are gone. 27% smaller in total.

### Fixed

- **Stale claims in `llvm-backend/CONTEXT.md`.** Roughly a hundred lines described machinery
  the move to HIR deleted — a backend type-collection pass (`type_pass.rs`, `expr_types`), the
  span-keyed side tables that fed it (`builtin_methods`, `fa_struct_names`, `binary_left_types`,
  `index_object_types`), and `global_const_types` — while the file's own header already said no
  such pass exists. The Struct ABI section still said a struct was unusable as a function
  parameter or return type, which the by-value struct work had made false.
- **`ast-types/CONTEXT.md` misreported method receiver support**, claiming `&self` was the only
  variant working end to end and that `&mut self` and `self` were rejected. All three reach
  codegen; owned `self` is accepted on a `Copy` receiver.
- **`module-resolution/CONTEXT.md`** said per-module private namespaces would arrive with the
  `export` rules. Those rules shipped; the merge is still flat, so a name collision between two
  modules is a hard error regardless of visibility.
- **`project-config/CONTEXT.md`** attributed dependency resolution to a phase number that is not
  in the roadmap. The field parses and nothing reads it.

## [2.1.3] - 2026-08-30

### Fixed

- **Binding a `void` initializer is a type error now, not an internal compiler error.**
  `val x = println("hi")` — and `val x = nothing()` for any user function with no return
  type — passed `neurc check` and then aborted code generation with `function call returned
  void when value expected`. A combination sweep found the same class reachable through six
  more spellings that BUG-016's report did not cover and its fix sketch would have missed:
  a `mut` binding, an `if` whose branches are both `void`, a `match` whose arms are, a bare
  block with a `void` tail, `loop { break }`, and an explicit `: void` annotation. The last
  four produce a different backend message (`void type cannot be used as a value`), which is
  why they read as a separate defect and were not.

  The check is on the binding's **type**, not on whether its initializer is a call — only two
  of the eight spellings have a callee at all — so one guard in `semantic-analysis`
  (`type_checkers/statements.rs`, the `Stmt::VarDecl` arm, beside the existing `Type::Unknown`
  guard) covers every one of them with the new `TypeError::VoidBinding`. The backend is
  unchanged: its two errors were the right last line of defence in the wrong place, and are now
  genuinely unreachable. Statement position never had the problem and still does not, so
  `println("hi")` on its own line, a `void` call on its own line, and a `void` `if` used as a
  statement all compile exactly as before.

## [2.1.2] - 2026-08-30

### Fixed

- **Windows CI: the `print` / `println` tests no longer fail on line endings.** On Windows
  fd 1 is a CRT text-mode descriptor, so the one `\n` byte the builtin writes leaves the
  process as `\r\n` — the same translation a C `printf` gets there. The builtins keep that
  platform convention; the `print_builtins` and `examples` harnesses now compare stdout
  with line endings normalized, which also stops the example golden files from depending
  on how git was configured to check them out.

### Changed

- **README roadmap collapses Phase 1 and opens Phase 2's sub-phases.** Phase 1 is one
  Complete row now that it has shipped; Phase 2 lists 2A–2E the way Phase 1 listed its own
  sub-phases while it was open.

### Documentation

- Corrected a false limitation in the struct reference: a *free* function may return a
  struct, and does — the by-value ABI covers free functions, associated functions, and
  methods alike. Consuming `self` methods remain unsupported.
- Retired stale "Phase 1" framing across `docs/` where it described the project's current
  state rather than a design decision.

## [2.1.1] - 2026-08-29

### Changed

- **The example harness now checks standard output, not just the exit code.** An example's
  expected stdout lives beside it in a sibling `.out` file, asserted byte for byte;
  `examples/expected.txt` keeps owning the exit code. An example with no `.out` file must
  print nothing, so the absence is itself an assertion and a silent program that starts
  printing fails rather than passing unnoticed. A `.out` file whose `.nr` has been deleted
  fails too, the same way a stale `expected.txt` entry already did. A mismatch reports the
  first differing line and then both texts with every line quoted, so trailing whitespace
  is visible.
- **Every example program now prints what it computes**, across `basics/`, `types/`,
  `operators/`, `control_flow/`, `structs/`, `modules/`, and `showcase/`. Not one exit code changed and `expected.txt` is untouched: the printing is
  added on top of the assertion that was already there, so an example is now readable by
  running it rather than by reading `echo $?`. Recursive ones print their own unwinding
  (`factorial.nr` each multiplication, `fibonacci.nr` every term); the self-checking ones
  print a labelled line per check plus a verdict, so a failure names itself instead of
  collapsing into a bare exit `1`; and the examples that previously computed values and
  discarded them — `extended_types.nr`, `float_suffixes.nr`, `strings.nr` — now report
  them, which is the only thing that ever made those files worth running.
- `types/string_interpolation.nr` and `types/triple_quoted.nr` print every rendering they
  check, so their golden files are now readable tables of the format mini-language and of
  block-string dedenting.
- `showcase/status_report.nr` gained the golden file its `println` output had always
  lacked.

### Fixed

- `.gitignore`: `examples/**` is an ignore-everything-then-allow list, so the new `.out`
  golden files would have been invisible to `git add`. Added `!examples/**/*.out`.
- `README.md`'s closures block is labelled "verbatim from
  `examples/showcase/closures.nr`" and had drifted from it; it is verbatim again. The
  quick-example perceptron block, which had never matched `examples/structs/neuron.nr`
  (it returned `0` where the file returns `4`), now does, and both blocks show the output
  the programs produce.

## [2.1.0] - 2026-08-29

### Added

- **`print` / `println` — standard output.** A compiled program can now say something to a
  terminal without dying: both builtins take one `string` and write it to stdout (fd 1),
  `println` appending a newline, and both return `void`. The exit code is no longer a
  program's only result channel. They mirror the panic family — resolved by name in
  semantic analysis with a hard-coded twin in the backend, so a user function of the same
  name shadows one, and neither is a prelude declaration, so `@no_prelude` does not remove
  them.
  - Nothing new was needed for formatting: string interpolation already renders every hole,
    format mini-language and all, into one ordinary `string` before the call is reached, so
    `println("x = {x:.2}")` is a plain one-argument call. There is no variadic form, no
    call-site format string, and no `Display` trait.
  - The argument is an owned `string` or an immutable `&string` sub-slice (what
    `.slice(range)` returns). A non-string argument is a type error rather than an implicit
    interpolation, and `&mut string` — a pointer to the fat pointer rather than the fat
    pointer itself — is rejected with it.
  - The text is **read, not consumed**: no move is recorded, so a value stays usable after
    being printed.
  - Output is **unbuffered**; a buffered standard-output type remains later work. Both
    builtins route through one module-private helper carrying a short-write retry loop, so a
    large buffer written to a pipe — where `write` routinely consumes less than it is
    offered — is not silently truncated.
- `examples/basics/greeting.nr`, a runnable tour of both builtins including an interpolated
  line and a printed `&string` slice.
- `docs/BUGS.md`: **BUG-016** — binding a `void`-returning call (`val x = println("hi")`, or
  any user function with no return type) passes the type checker and then aborts code
  generation with `function call returned void when value expected`. Pre-existing for user
  `void` functions; the new builtins make the shape easier to write by accident. Statement
  position is unaffected.

### Changed

- `examples/showcase/status_report.nr` now prints the report it builds, so the showcase
  exercises `println` alongside structs with `impl` methods, an enum matched by `match`, a
  fixed-size array walked by `for`-in, string concatenation, and the interpolation format
  mini-language. Its exit code is unchanged; each line is printed and then checked against
  the exact text it should produce.
- The quick start, the functions reference, the modules reference, and both capability
  tables describe standard output. The quick start no longer says a program has no way to
  print.

## [2.0.3] - 2026-08-29

### Fixed

- **BUG-015: `==` on a type with no equality passed the type checker and crashed the
  backend.** `a == b` was accepted for any two operands of compatible type, so a struct
  with no `impl PartialEq` — and equally an array, a tuple, an enum, a collection, or a
  non-string reference — reached codegen, which asked the aggregate value for its integer
  variant and aborted the compiler with `Found StructValue ... but expected the IntValue
  variant`. Equality is now checked against the types that have it: the scalars, `string`
  (and a `&string` slice), and a newtype forwarding one of those. A struct without the
  impl is named along with the trait it is missing; every other operand is reported as the
  operator error the ordering comparisons already gave. The supported path — a `Copy`
  struct with an explicit `impl PartialEq` — is unchanged, and so is `==` on a newtype
  over a scalar.
- A generic body types `a == b` as its type parameter and is checked once as a template,
  so an instantiation whose argument is an aggregate could still reach the backend. HIR
  lowering now refuses a binary operator whose operand type has no instruction sequence
  and no operator-trait impl, and the LLVM backend answers an operand that is neither an
  integer nor a float with an error rather than panicking — the whole family of
  `into_int_value` aborts on this path is closed.
- **BUG-014: a named argument was evaluated in declaration order, not source order.**
  Binding permuted the arguments into the callee's declaration order, which is also the
  order every later stage evaluates them in, so `combine(second: b(), first: a())` ran
  `a()` first — the positional form `combine(b(), a())` ran `b()` first, and the two call
  forms the specification calls equivalent could compute different answers. A call whose
  permutation would reorder arguments that carry (or can observe) an effect is now
  rewritten into a block that binds each of them to a temporary in the order it was
  written and passes the temporaries in declaration order; each temporary carries its
  parameter's declared type, so an argument is still typed by the parameter it binds to.
  A call whose arguments are literals, or whose permutation moves nothing observable, is
  still permuted in place and still compiles to the same IR as the positional call.
- A bare block whose last statement calls a unit function aborted code generation with
  `function call returned void when value expected`: the block's tail was read as its
  value regardless of type. A `void` tail is now emitted as an ordinary statement, and the
  block yields unit.

## [2.0.2] - 2026-08-29

### Fixed

- **Documentation: `print` / `println` were documented as available and do not exist.**
  Three documents stated that the implicit prelude puts `println` and `print` in scope in
  every module. No such builtin is implemented in any slice — `neurc check` on a call to
  either reports `undefined function 'println'`. The prelude claim is corrected in the
  documentation index, the quick-start feature list, and the modules reference, and the
  quick-start now says plainly that a program's exit code is currently its only result
  channel, since the sole text a compiled binary writes is a panic diagnostic on stderr.
  Printing to stdout is now the first tracked item of Phase 2.
- Corrected the spread operator's spelling in the planning documents from `...expr` to
  `..expr`, matching the specification and the struct-update form that already parses.

### Added

- `docs/BUGS.md`: **BUG-015** — `==` on a struct that implements no `PartialEq` passes the
  type checker and then crashes the backend with an internal `Found StructValue ... but
  expected the IntValue variant` error. The supported path (`@derive(Copy, Clone)` plus an
  explicit `impl PartialEq`) compiles and runs correctly; what is missing is the rejection
  of the unimplemented one. Also records that `@derive(PartialEq)` — and any unknown
  derive — is currently accepted and silently ignored.

### Changed

- A sub-feature audit of the language specification against the roadmap found six
  constructs that are specified but were never given a roadmap item, and are not
  implemented: `print` / `println`, `.is_nan()`, the codepoint-aware string APIs
  (`.chars()`, `.char_indices()`, `.char_slice(range)`), `.enumerate()`, the
  `IntoIterator` / `Iterator` protocol with its adapters, and `if val` / `while val`.
  All six are now tracked. The audit's own coverage rule was tightened at the same time:
  checking that each specification section maps to some roadmap item read 100% clean while
  every one of these was missing, so coverage is now defined over the constructs a section
  names, and a deferral written into the prose of a completed item no longer counts as
  tracking it.
- The specification's syntax-summary table used a phase numbering of its own that no
  longer matched the roadmap's (it listed tensors as phase 3, autodiff as 4, and so on).
  Its phase column is now the roadmap's numbering, with a note saying so.

## [2.0.1] - 2026-08-29

A stabilisation pass: a combination sweep over the most recently landed features, the two
fixes it and the open bug register called for, and one new register entry.

### Fixed

- `parser`: `return`, `break`, and `continue` are accepted as an unbraced `match` arm body
  (BUG-012). `0 => return 1,` failed with `unexpected token Return, expected expression`
  while `0 => { return 1 }` compiled and ran, so the braces carried no information. The arm
  body now wraps such a statement in the single-statement block the braced form already
  produced — the arm-divergence rule types it unchanged, and no existing program changes
  meaning. A comma additionally terminates a valueless `return` / `break`, which is what a
  comma-separated arm needs and which no expression can begin with.
- `semantic`: a function name in value position reports that it is a function (BUG-013).
  `apply_twice(inc, 10)` claimed `undefined variable 'inc'` — denying a name the program
  declares — because functions live in their own namespace and resolution fell through to
  the variable lookup. The new `TypeError::FunctionUsedAsValue` names the function and
  suggests the closure wrapper. There is still no fn-item-to-fn-pointer coercion; only the
  diagnostic changed.

### Added

- `tests`: regression coverage for both fixes — four `match` arm bodies in
  `pattern_matching.rs` (valued and valueless `return`, `break` with a value, `continue`,
  valueless `break`) and three diagnostics in `functions.rs`, including one asserting that
  a genuinely undeclared name still reports `undefined variable`.
- `docs`: BUG-014 filed in `docs/BUGS.md` — a named call evaluates its arguments in the
  callee's declaration order while the positional form evaluates them in source order,
  because argument binding permutes the argument expressions themselves. The specification
  calls the two forms equivalent and does not define evaluation order, so the entry asks
  for a ruling rather than proposing a patch.

## [2.0.0] - 2026-08-29

**Phase 1 — Core Language is complete.** Every sub-phase 1A–1H has shipped, so this is the
milestone release that closes the phase and opens Phase 2 (Tensors). The language surface a
general-purpose program needs is in: primitives and casts, ownership with a borrow checker,
arrays, tuples, structs, enums and pattern matching, generics, traits with static and dynamic
dispatch, closures, `Option` / `Result` with `?` and `??`, the standard collections, multi-file
modules with an implicit prelude, string interpolation, and named arguments. No language feature
is added here — the version number is the deliverable, alongside the cleanup below.

### Removed

- `build`: twenty dependencies that no crate referenced. `shared-types` and `source-location`
  dropped `serde`, `shared-types` also `thiserror`; `diagnostics` dropped `miette` and
  `source-location`; `project-config` and `semantic-analysis` dropped `anyhow`, as did
  `llvm-backend`; `lexical-analysis` dropped `unicode-segmentation` and `source-location`;
  `syntax-parsing` and `semantic-analysis` dropped `source-location`; `control-flow` dropped
  `shared-types` and `diagnostics`; `neurc` dropped `shared-types`, `source-location`,
  `project-config`, `lexical-analysis`, `control-flow`, and `clap_complete`. `neurc` reaches the
  lexer through `syntax-parsing::parse`, which is why it never named the lexer itself.
- `build`: eleven `[workspace.dependencies]` declarations no member ever claimed — `miette`,
  `unicode-segmentation`, `clap_complete`, `proptest`, and the whole Concurrency (`rayon`,
  `crossbeam`), Utilities (`itertools`, `once_cell`, `parking_lot`), and LSP (`tower-lsp`,
  `lsp-types`) blocks. A phase that needs one declares it then.

### Changed

- `docs`: `compiler/control-flow/CONTEXT.md` now states what the slice is. It claimed to enable
  unreachable-code detection and return-path analysis; both actually live in `semantic-analysis`
  and neither waits on it. The slice has no caller, `build_cfg()` returns an empty graph, and the
  file says so.
- `docs`: `compiler/neurc/CONTEXT.md` Shared Kernel matches its manifest again after the
  dependency removals.
- `docs`: `docs/compiler/components/lexical-analysis.md` dropped two claims the slice does not
  make good on — a `unicode-segmentation` dependency it no longer has, and string interning for
  identifiers, which it has never done.

## [1.80.0] - 2026-08-29

### Added
- `semantic`: **growable strings** (§2.8). `String` is the mutable, heap-backed counterpart
  to the immutable `string` — the same pair `[T; N]` / `Vec<T>` already is — for text that is
  assembled rather than passed around. Surface: `String::new()`, `.push_str(text)`, `.len()`,
  `.clear()`, and `.to_string()`. Building an n-piece string with `+` copies everything
  accumulated so far on every step; a `String` appends in amortized O(1) into one buffer.
- `semantic`: `String` needs no type annotation. It takes no type arguments, so
  `mut s = String::new()` infers, unlike `Vec::new()`, and its bare name is a complete type
  in an annotation. A program that declares its own `String` still shadows the built-in one.
- `codegen`: new `codegen/collections/strings.rs` and the shared
  `__neuro_string_reserve(header, extra)` helper, which grows capacity to
  `max(cap * 2, len + extra, 8)` — so appending one large string is a single `realloc`
  rather than a chain of doublings, while repeated small appends stay amortized O(1).
- `tests`: `compiler/neurc/tests/string_builder.rs` covers construction, owned and borrowed
  appends, byte length, buffer-retaining `clear`, the copy back out, `&mut String`
  parameters, move-on-assign, use-after-move rejection, growth past the initial capacity,
  and 2000 builders in a loop completing without unbounded heap growth.
- `docs`: `examples/types/string_builder.nr` and the `examples/showcase/log_builder.nr`
  showcase, which assembles a run transcript in one buffer while combining `Vec<T>` + `for`-in,
  a `@derive(Copy, Clone)` struct with `&self` methods, an enum with a payload + `match`,
  string interpolation with the format mini-language, and `&mut String` parameters.

### Changed
- `infra`: `HirCollectionKind` gains a `String` variant plus `arity()` and `mangle_tag()`.
  `String` is the one **nullary** collection kind, so it reuses the entire standard-collection
  machine — the shared `{ buffer, len, cap, used }` header, `realloc` growth, move-on-assign,
  never `Copy`, and the `DropTarget::Collection` scope-exit `free` — and needs no new
  `Type` / `HirType` variant in any slice. Its buffer is a byte run, so `len` and `clear` fall
  out of the existing kind-agnostic entries and only appending is new. `String` mangles as
  `strbuf`, which cannot collide with the primitive `string`.
- `semantic`: the "a collection's bare name is not a type" rule now applies only to kinds that
  actually take arguments (`Vec`, `HashMap`, `BTreeMap`); a nullary kind resolves directly.
  `Type`'s `Display` omits `<>` for a nullary collection, so diagnostics read `String`.
- `docs`: the README alpha memory warning is narrowed. A `String` builder's buffer is freed at
  scope exit like any other collection; what still leaks is the *anonymous* heap `string` that
  `+`, interpolation, and `String::to_string` produce, which no tracked binding owns.

### Notes
- `.push_str` accepts a `string` or an immutable `&string` and **reads** it rather than moving
  it, the same latitude `+` gives its operands.
- `.to_string()` copies. A borrowed view into the builder's buffer would be free, but a later
  `push_str` may reallocate and leave it dangling, and the borrow checker does not yet track a
  builder's outstanding views — undefined behavior is not a valid outcome for well-formed input.
- Deferred and documented in §2.8: `.push(char)`, `String::with_capacity`, `String::from`,
  `.is_empty()`, a borrowed `.as_str()`, `String` as a collection element or map key, and
  `String` in an interpolation hole or as a `+` operand (each gives a clean diagnostic today).

## [1.79.0] - 2026-08-29

### Added
- `parser`: **named arguments** (§3.13). An argument may be passed by name —
  `connect("localhost", port: 8080)` — in any order, after any number of positional ones.
  A parameter may be declared with two names, `external internal: T`: the caller writes the
  external label, the body uses the internal name, and the label is **required** at every
  call site. An external label of `_` (`_ value: f32`) is the opposite — the argument is
  positional and naming it is an error. Free functions, associated functions, and instance
  methods all accept named arguments; closures, the panic builtins, enum variants, and
  newtype constructors declare no parameter names, so a label on one is rejected.
- `infra`: new slice `compiler/argument-binding`. It builds a callee → parameter-names table
  from the whole program, then permutes every call's arguments into the callee's declaration
  order and drops the labels. It runs after module resolution has merged every file and the
  prelude is in place (a callee may be declared anywhere in that list) and before type
  checking (which would otherwise pair the wrong argument with the wrong parameter), so
  semantic analysis, HIR lowering, and both backends are unchanged — a named argument
  produces exactly the IR the positional call produces. A call that named nothing, and whose
  callee requires no label, is left byte-identical.
- `docs`: `docs/compiler/components/argument-binding.md`; a Named Arguments section in
  `docs/language-reference/functions.md` replacing its stale "Not Yet Implemented" entry;
  `examples/basics/named_arguments.nr` and the 1H showcase
  `examples/showcase/render_settings.nr`, which combines named arguments with string
  interpolation, a triple-quoted block, a nested block comment, `&self` / `&mut self`
  methods, an enum + `match`, an array + `for`-in loop, and `??`.

### Changed
- `parser`: the three duplicated parameter-list loops (free function, method, trait method)
  collapsed into one `parse_parameter_list`. Methods and trait methods consequently gained
  the duplicate-parameter-name check that only free functions had. Two parameters answering
  to one call-site name is a new `DuplicateParameterLabel` error — a named argument must
  identify exactly one parameter.
- `infra`: `ast_types::Parameter` gains `label: ParamLabel`; `Expr::Call` gains
  `arg_labels: Vec<Option<Identifier>>` beside `args`, empty when the call named nothing.
  The labels sit beside the arguments rather than wrapping each one because they are
  parse-only surface that `argument-binding` erases before type checking, so every pass
  downstream keeps reading `args` as the positional list it always was.
- `codegen`: HIR lowering refuses a call whose `arg_labels` survived. A label reaching
  lowering would mean the binding pass never visited that call, and the arguments would then
  lower in the order they were written rather than the callee's — a wrong program instead of
  a failed build.
- `infra`: the editor TextMate grammar gained a `parameter_labels` rule so the
  `external internal: T` form is coloured. The existing `#parameters` rule keys on a name
  sitting directly before the `:` and matched neither half of it. The packaged `.vsix` is
  still 1.1.0 and does not carry the new rule.

## [1.78.2] - 2026-08-27

### Fixed
- `lexer`: triple-quoted block strings leaked the `\r` of a CRLF source into their value,
  and read the CRLF trailing the opening delimiter as content rather than punctuation, so
  a block string gained a leading blank line and a `\r` on every line. Windows checkouts
  get CRLF from git's `core.autocrlf`, which made every block-string comparison fail there
  while passing on Linux and macOS. `dedent_block_body` now drops a line's trailing `\r`
  with its newline, so a block string's value is identical under either line ending.

## [1.78.1] - 2026-08-27

### Fixed
- `infra`: seven rules in the editor TextMate grammar were unreachable. TextMate breaks a
  tie between two rules matching at the same offset by their position in the top-level
  `patterns` array, never by match length, so `#keywords` — listed ahead of every
  declaration rule — claimed the leading `func` / `struct` / `enum` / `trait` / `impl` /
  `type` / `newtype` and the rule naming the declared symbol never ran. Declared type
  names were left unscoped entirely; a declared function name was picked up by the
  call rule and coloured as a call. The declaration rules now precede `#keywords`.
- `infra`: character literals had no rule at all, so `#lifetimes` claimed the opening
  quote of `'a'` and left the closing quote stranded. Added `#chars`, ordered ahead of
  `#lifetimes`, matching the one-scalar forms the lexer accepts.
- `infra`: the grammar's `\\.` escape rule matched `\u` and left `{1F600}` to the
  interpolation rule, which scoped a Unicode escape as a `{expr}` hole. Escapes are now
  matched whole (`\u{...}`, `\xNN`, and the single-character forms), and an escape the
  lexer rejects scopes as `invalid.illegal` instead of as a valid one.
- `infra`: exponent-only float literals (`1e10`, `2e3f32`) matched no numeric rule and
  went unhighlighted; the grammar required a decimal point. Integer literals no longer
  accept float suffixes, and floats no longer accept integer suffixes, matching the lexer.
- `infra`: `#attributes` began on a bare `@`, so it claimed every `@` in the file and the
  matmul operator rule behind it could never run. It now requires an identifier after the
  `@`. The `&mut` and dereference rules were likewise unreachable behind the bitwise and
  arithmetic rules.
- `infra`: interpolation holes included `$self`, which admitted a `"` string literal the
  lexer rejects — a quote ends the enclosing token. Holes now use a restricted rule set,
  balance nested braces, and scope a `{value:>8.2}` format specifier as one, rather than
  as a comparison operator followed by a float.
- `infra`: the grammar matched ASCII identifiers only, while the lexer accepts
  `XID_Start`/`XID_Continue`. A function named `számol` went unscoped. All identifier
  patterns are now Unicode.
- `infra`: `=>` scoped as an assignment followed by a comparison; `void` was missing from
  the primitive types; a loop label's scope covered the trailing colon and whitespace.

### Added
- `infra`: the editor grammar now scopes user-defined types at their *use* sites, not just
  their declarations, along with `SCREAMING_SNAKE` constants, function parameters, import
  path segments, the prelude's types and variants, the compiler-known collections, and the
  `panic` / `assert` / `unreachable` builtins.
- `tests`: `tmlanguage_sync.rs` grew from one test to three. Beyond asserting every lexer
  keyword appears in the grammar, it now asserts the grammar's keyword rule contains *only*
  words the lexer tokenizes, and that the declaration rules stay ordered ahead of
  `#keywords`. Both new tests were confirmed to fail when their bug is reintroduced.
- `infra`: `tools/tmlanguage_scopes.mjs` prints the scopes the grammar assigns to a source
  file, driving the same tokenizer VS Code runs. Outside CI, so the workspace keeps no
  Node dependency.
- `infra`: the extension's `language-configuration.json` gained a Unicode-aware
  `wordPattern`, block-comment continuation on Enter, and `//#region` folding markers.
  `'` was removed from `autoClosingPairs`: it opens a lifetime far more often than a
  character literal, and auto-closing turned every `<'a>` into `<'a'>`.

### Removed
- `infra`: vocabulary the compiler does not accept is no longer highlighted as though it
  did — the keywords `async`, `await`, `spawn`, `defer` and `pool`, the operators `>>`
  (compose), `|>` (pipeline) and `@` (matmul), the type names `Tensor`, `Future`,
  `ParameterList`, `KernelOut` and `Device`, and the constants `NaN` and `Inf`. None of
  them is a token the lexer produces. `Option`, `Result`, `Some`, `None`, `Ok` and `Err`
  stay: the prelude declares them in every program.

## [1.78.0] - 2026-08-27

### Added
- `lexer`: block comments now **nest** (roadmap 1H, spec §1.1). `/* outer /* inner */
  still outer */` is one comment: the first `*/` closes only the inner one, so a block
  that already contains a comment can be commented out wholesale. Each `/*` needs its
  own `*/`, and a file that ends while a comment is still open is
  `LexError::UnterminatedBlockComment` spanning from the opening delimiter to EOF.
  A comment body is scanned as raw text — `/*` and `*/` inside a string or char literal
  within a comment still count toward depth, matching how `//` already swallows a quote
  to end of line.

### Changed
- `lexer`: `_BlockComment` is no longer a regex. Nesting is not a regular language, and
  logos matches the longest run its DFA accepts, so the old pattern closed at the first
  `*/` and left the remaining body to lex as garbage. It is now a bare `#[token("/*")]`
  whose callback counts depth over the remainder and returns `FilterResult::Skip` or
  `FilterResult::Error` — the same "match the opener, hand the body to a scanner" shape
  triple-quoted strings use. Comments still produce no tokens, so no parser, semantic,
  or codegen behavior changed.
- `lexer`: `Lexer::classify_error` no longer rewrites a lex failure sitting on `/*` into
  `UnterminatedBlockComment`. The scanner raises that error itself with a precise span,
  making the reclassification branch unreachable; it was removed.
- `infra`: the editor TextMate grammar's block-comment rule became its own
  `#block_comment` repository entry that recurses only into itself. Recursing through
  `#comments` let the line-comment rule consume a `*/` inside a block comment, which the
  lexer does not do.

### Fixed
- `codegen`: `format_float` tripped clippy's `byte_char_slices` lint on
  `[b'.', b'e', b'n', b'i']`, failing the `-D warnings` CI gate. Written as `*b".eni"`.

## [1.77.0] - 2026-08-27

### Added
- `lexer`: triple-quoted (block) string literals `"""…"""` (roadmap 1H, spec §1.7).
  A block spans lines and is dedented to the column its closing `"""` sits at, so a
  literal can be indented with the code around it without that indentation reaching
  the value. The newline after the opening delimiter is punctuation and is dropped;
  text trailing the opening delimiter on the same line is content and is exempt from
  the dedent rule; blank lines normalize to empty without an indent check;
  indentation deeper than the closing prefix survives. A single `"` or `""` inside
  the block needs no escape, and escapes plus `{...}` interpolation holes behave
  exactly as in a `"…"` literal.

  Implemented as a bare `"""` token whose callback scans and bumps the body itself:
  logos has no non-greedy repetition, so a regex ending in `"""` would run to the
  last one in the file. `decode_string_literal` now delegates to a shared
  `decode_chunks` over `(absolute_offset, char)` pairs, which lets dedent work by
  omitting characters rather than rebuilding text — so interpolation holes inside a
  block string still report at real source columns. The token payload is an ordinary
  `StringValue`, so the parser, type checker, and backend are unchanged.

  Three new `LexError` variants carry the failures: `UnterminatedTripleQuotedString`,
  `TripleQuoteClosingNotOnOwnLine`, and `TripleQuoteUnderIndented`.

  New examples: [`examples/types/triple_quoted.nr`](examples/types/triple_quoted.nr)
  checks each rule against its expected text, and
  [`examples/showcase/config_manifest.nr`](examples/showcase/config_manifest.nr)
  renders a config manifest combining block strings with a `@derive(Copy)` struct +
  `impl` methods, an enum + `match`, a fixed-size array + `for`-in, `+` concatenation,
  and interpolation format specs.

## [1.76.1] - 2026-08-26

### Fixed
- `docs`: full documentation audit against the compiler. Corrected the operator
  precedence tables in [operators.md](docs/language-reference/operators.md) and
  [expressions.md](docs/language-reference/expressions.md) to match the parser's
  ladder (comparison binds tighter than equality; ranges loosest; no merged level),
  removed a duplicated Operator Overloading section, and replaced stale pasted
  definitions (lexer error variants, type-error list, checker state) with prose and
  source links. Captured real CLI output for check/compile/error examples, documented
  the actual `prefer-loop-over-while-true` lint in place of an unreachable-code warning
  that never existed, rewrote the docs index example programs to quote runnable
  `examples/` files verbatim, and fixed broken anchors plus sweep punctuation.

## [1.76.0] - 2026-08-25

### Added
- `lexer`, `parser`, `semantic`, `codegen`: string interpolation with the format
  mini-language — sub-phase 1H, item 1. A string literal may embed expressions in `{...}`
  holes and render each through an optional `:spec`:

  ```neuro
  val message = "Welcome to {name} v{version}"
  val report  = "pi to 2dp is {pi:.2}, flags are {n:08b}"
  ```

  The whole specifier table is supported: `?` debug, `.N` fixed-point, `e` / `.Ne`
  scientific, `d` / `x` / `X` / `b` / `o` radix, `W` field width with `<` / `>` / `^`
  alignment, `0W` zero fill, and the `+` sign flag. Interpolation renders integers,
  floats, `bool`, `char`, and `string`.

  A hole holds any expression — a call, a field access, a struct literal, an `if`, a
  nested block — because the lexer only locates the hole's bounds by brace matching and
  the parser re-parses its text as ordinary code, with spans shifted onto real file
  coordinates so diagnostics inside a hole point at the right column. Write a literal `{`
  as `\{`; an unpaired `}` needs no escape.

  Which specifiers a value accepts is checked against its type, so a mismatch is a
  compile error rather than a surprising rendering: radix kinds require an integer,
  fixed-point and scientific require a float, `+` requires a signed integer or a float,
  and zero fill does not combine with `<` or `^`. Field width and precision are bounded.

  Runtime rendering is emitted once per module as internal helper functions rather than
  inlined at every hole: `snprintf`-backed integer and float conversion, hand-written
  binary digits, sign-aware field padding (`-42` to width 6 is `-00042`), UTF-8 encoding
  of a `char`, and the two fix-ups that reconcile C's float output with the specifier
  table — restoring the point `%g` drops (`2.0`, not `2`) and normalizing `e+00` to `e0`.

  Two limits, both documented and diagnosed: a hole may not contain a `"` string literal,
  because the quote ends the enclosing token (reported as an unterminated hole); and an
  interpolated literal is not a constant, so it cannot appear in a pattern.

- `tests`: `compiler/neurc/tests/string_interpolation.rs` covers every specifier
  end-to-end plus the rejection paths; `examples/types/string_interpolation.nr` checks
  each row of the table against its exact output, and `examples/showcase/status_report.nr`
  combines interpolation with a `@derive(Copy)` struct, `impl` methods, an enum matched by
  `match`, a fixed-size array walked by `for`-in, and `+` concatenation.

### Fixed
- `lexer`: brace-depth scanning inside an interpolation hole advanced twice past a nested
  char literal, so `"{'\u{7D}'} tail}"` ran the hole on to the final `}`. The escape
  payload's brace no longer closes the hole it sits in.

## [1.75.6] - 2026-08-25

### Changed
- `docs`: the open-defect register is now public and tracked at `docs/BUGS.md` instead of
  living in internal notes. Each entry is a self-contained task — minimal reproduction,
  known root cause, workaround, fix sketch — so contributors can pick up bug fixes
  without reading compiler internals first. Entries are removed once fixed; ids are never
  reused.
- `docs`: CONTRIBUTING.md now leads its contribution priorities with "fix a known bug"
  and points at `docs/BUGS.md`, ahead of the roadmap features section; README.md links
  the register from its Contributing line.

### Added
- `docs`: BUG-013 filed in `docs/BUGS.md` — using a named function as a value where a
  closure-typed parameter is expected fails with a misleading `undefined variable`
  diagnostic (no fn-item-to-fn-pointer coercion exists, and nothing says so). Workaround:
  wrap it in a closure literal.

## [1.75.5] - 2026-08-24

### Changed
- `docs`: the README demo GIF is re-recorded against the current compiler. The old
  recording compiled `examples/neuron.nr`, a path that no longer exists, and showed only
  compile-and-run. The new one type-checks (`neurc check`, reporting modules and lowered
  HIR items), compiles with `-O2`, runs the binary, and times it, on
  `examples/showcase/perceptron.nr`.
- `docs`: the README badge row links the documentation site.

### Added
- `docs`: `assets/demo.tape` — the VHS script the demo GIF is recorded from, so the
  recording is reproducible instead of hand-made. Regenerate with `vhs assets/demo.tape`
  from the repository root after `cargo build --release -p neurc`.

## [1.75.4] - 2026-08-24

### Fixed
- `semantic`: an `if` or `match` arm that leaves the scope no longer forces its type on the
  arms that stay. `if n > 0 { return 1 } else { 2 }` was rejected as `expected void, found
  i32` — the returning arm typed as `void`, so the arm carrying the actual value became the
  error, and the diagnostic named the diverging arm as what was expected. A `return`,
  `break`, or `continue` never reaches the point where the expression has a value, so it
  describes nothing about it: such an arm now contributes no type at all, which is the
  contract `panic` and `unreachable` already had. The rule reuses the existing divergence
  analysis and applies to `match` arms and to expression position (`val v = if c { return 1 }
  else { 2 }`) alike; a body whose every arm returns is still a statement, and a non-void
  function that can fall off the end is still an error.
- `parser`: a newline before a line beginning with `(` or `[` ends the statement. The
  expression parser skipped newlines before consulting the next token's precedence, so the
  *following* line decided whether the expression continued, inverting the documented rule
  that the line which just *ended* decides — by ending with an operator, a comma, or an
  opening delimiter. `val a = f()` followed by a line `(2 + 3)` therefore parsed as
  `f()(2 + 3)`, and a following `[1, 2]` as an index into it. When the value on the left was
  callable — a closure binding — the misparse type-checked, and the program silently computed
  something its source never asked for. A leading `.` still continues a method chain, and a
  leading `*` was already handled.
- `semantic`: a parameter whose type failed to resolve is still bound, so its uses in the
  body no longer produce a second round of "undefined variable" errors chasing the one that
  was already reported.

### Changed
- `docs`: the compiler-architecture documents now describe the pipeline that exists — module
  resolution has its own component document and appears in the compilation pipeline, the
  parser and semantic-analysis documents no longer reproduce AST and type definitions that
  had drifted from the source, the operator-precedence table covers the whole ladder, and the
  backend's type table covers the aggregates, references, and trait objects it lowers.
  Statement boundaries and arm divergence are documented in the language reference.
- `docs`: `CONTRIBUTING.md` states the active sub-phase's four tasks as the contribution
  priorities instead of restating the changelog; per-sub-phase status now has one home.
- `build`: `tools/check_docs_hygiene.py` rejects any written-down test total, not only the
  three phrasings it recognised before.

## [1.75.3] - 2026-08-24

### Fixed
- `semantic`: a non-void function or method that falls off the end of its body is now a
  compile error instead of undefined behaviour. The checker recognised only a trailing
  bare expression as the implicit return, so a body ending in anything else — a trailing
  `if` (which the parser always shapes as a statement), a loop, a binding, nothing at all
  — was neither checked against the declared return type nor checked for producing a
  value. The backend left the exit block without a return, LLVM terminated it with
  `unreachable`, and the verifier stayed silent because `unreachable` is a legal
  terminator; the compiled program then ran off the end of the function and crashed. The
  same body checked out and ran correctly whenever the input happened to take a path that
  did return, so the crash depended on the argument, not the program. Inherent methods and
  trait default methods had the same hole.
- `semantic`: a trailing `if`/`else` is now checked against the declared return type. It is
  the function's implicit return and always was — lowering has treated it as one all along
  — but nothing type-checked it, so `func f(n: i32) -> i32 { if n > 0 { true } else { false } }`
  reached the LLVM verifier and failed there as an internal error. Mixing a `return` and a
  value across the arms (`if n > 0 { return 1 } else { 2 }`) silently evaluated to `0` on
  the else path; it is now the same type error the identical expression already was when
  bound to a `val`.
- `semantic`, `codegen`: a divergent arm no longer decides an `if`'s or a `match`'s type.
  `panic` and `unreachable` never return, so they adopt whatever type their context
  demands and describe nothing about the expression around them — yet both the checker and
  HIR lowering took the first arm's type unconditionally. With the divergent arm written
  first, `val v = if c { panic("x") } else { 2 }` was rejected as an undefined variable,
  while the same two arms in the other order compiled. Both now take the type from the
  first arm that carries one.
- `parser`: the span of a generic type application covers its closing `>`. It ended at the
  last type argument, so every diagnostic pointing at one underlined a byte short —
  `Box<i32, i32` for `Box<i32, i32>`.
- `lexer`: `/*` with no closing `*/` reports an unterminated block comment. The comment
  regex consumes to end-of-file without completing, so logos failed sitting on the opening
  slash and the reader was told a `/` was unexpected. `LexError::UnterminatedBlockComment`
  already existed; nothing constructed it.
- `infra`: compiling a program with no `main` reports a missing entry point instead of
  reaching the system linker, which answered with `undefined reference to 'main'` naming
  the C runtime's startup object rather than the user's program. `check` is unaffected:
  type-checking a module with no `main` is legitimate.

## [1.75.2] - 2026-08-23

### Fixed
- `codegen`: a `&string` returned from a function no longer points into the dead
  callee frame. `&string` is now the `{ ptr, i64 }` fat pointer **by value** — the
  representation the string ABI already documented — rather than a pointer to one.
  A borrow of a live binding was always fine, but `s.slice(a..b)` computes a fat
  pointer with no home, so it was spilled to a function-local stack slot whose
  address was handed back; the value survived only until the next call reused that
  region, making the result pure frame-layout luck. By value there is no slot: a
  slice is returned like any other aggregate and `.len()` is an `extractvalue`.
  `&mut string` keeps the referent's address, since a store through it has to reach
  the caller's binding, so the backend's reference type now carries its mutability.
  This also removes the last stack slot allocated outside the entry block, so a
  `.slice()` inside a loop stops growing the stack.
- `parser`: `val Some(v) = f() else { ... }` parses. The `val-else` marker was
  `val Name::`, so only the qualified spelling reached it, while the implicit
  prelude had made the unqualified variant idiomatic in value position, in `match`
  arms, and in every other pattern. `val Name(` is now a second marker — a binding
  name is never followed by `(` — and a `val-else` missing its `else` reports the
  missing `else` instead of an unexpected `=`. `val Name { ... }` remains a struct
  destructure and a payload-less `val None = ...` remains a binding, both because
  they cannot be told from the alternative without an import table.
- `infra`: an `import` whose path names no module is no longer read as an enum
  regardless of whether one exists. The single-segment fallback is what lets
  `import Option::{Some}` resolve before the prelude is prepended; it now requires
  the head to be a declared `enum` or a prelude enum, so `import Nothing::{AtAll}`
  is reported rather than silently bound to nothing. A path naming a sibling inline
  `module { }` block — which a block cannot see — says exactly that, instead of
  claiming the imported name is an enum variant. Under `export import` that claim
  was doubly wrong: it named a `const` or a `func` as a variant, and advised
  dropping the `export`, which only exchanged the error for the silence.

## [1.75.1] - 2026-08-23

### Fixed
- `semantic`, `codegen`: an `if`/`else` in value position now carries its context
  into its arms, the way a `match` always has. The arms were typed against
  nothing, so an arm naming no type of its own — a bare `None`, an untyped
  integer literal — had nothing to resolve against even when the `val` it
  initialized was annotated or the parameter it was passed to was typed. The two
  spellings of one computation disagreed: `match n { 0 => None, _ => Some(4) }`
  compiled where `if n > 0 { Some(4) } else { None }` did not, and the diagnostic
  advised annotating the target, which did not help. The type checker and HIR
  lowering both mirror `match` now — the arm hint is the caller's expected type
  if there is one, else the first arm's type once known.

## [1.75.0] - 2026-08-23

### Added
- `semantic`: the implicit prelude. Every module now begins with `Option`,
  `Result`, their variants `Some` / `None` / `Ok` / `Err`, and `println` / `print`
  in scope with no `import` of any kind, so `Some(n)` and `Err(e)` read bare in
  value and pattern position alike. Module resolution seeds each module's import
  scope with the prelude's variants — the same table an explicit
  `import Option::{Some}` fills — so nothing after that pass learns a new concept.
- `parser`: `@no_prelude`, written on a file's first line, opts that file out of
  the prelude. It is rejected after a declaration and inside a `module { }` block,
  which is not a file (`ParseError::MisplacedNoPrelude`). On a non-root file it
  drops that file's bindings; on the root it also drops the prelude's declarations
  from the whole program, since the merged namespace is flat.
- `infra`: `ast-types` gains `Item::NoPrelude`, a parse-only marker consumed by
  module resolution like `Item::Import` and `Item::Module`.
- `semantic`: `module-resolution` gains the public `PreludeVariant` input and
  reports the root file's opt-out as `ResolvedProgram.no_prelude`. The prelude's
  contents are injected by the driver for the same reason the parser is — a
  feature slice may not reach into another one.

### Changed
- `infra`: `neurc`'s prelude module became a `Prelude` value: one parse supplies
  both the declarations it prepends and the variant names it hands to module
  resolution, so `prelude.nr` stays the single place the prelude's contents are
  stated.
- `docs`: a variant import of a prelude name is now redundant rather than
  required. An explicit import of one still wins over the implicit binding, as
  does a local declaration of the same name — neither is a collision.

## [1.74.0] - 2026-08-23

### Added
- `parser`: inline `module Name { ... }` blocks. A block groups items inside one
  file and is a module in every sense a `.nr` file is one — its items are private
  unless written with `export`, a qualified path reaches into it, an `import` binds
  from it, and blocks nest. `parse_program` now delegates to a shared item-list
  parse that runs to end of input at file level and to the closing brace inside a
  block, so a block's body is parsed by exactly the code the file around it is.
- `parser`: `export import` re-exports. `export` before an `import` marks the
  declaration as a re-export instead of being rejected, and is now rejected on a
  `module` block instead — an inline module's name is reached only from the file
  that declares it.
- `infra`: `ast-types` gains `ModuleDef` / `Item::Module` and `ImportDef.exported`.
  Both are consumed by module resolution, so no pass after it sees either.
- `semantic`: a re-exported name is reachable *through* the module that re-exported
  it. A facade may re-export what another facade re-exported, and an `as` rename is
  undone on the way through — the program is built with the declaration's own name.

### Changed
- `semantic`: module resolution registers each inline block as a module of its own
  rather than special-casing it, so the visibility rule, the flat merge, the
  collision check, and the `ModuleId` stamp reach a block unchanged. A block
  resolves paths against the containing file's directory, holds no file children,
  and wins over a same-named file beside it — the rule a locally declared type
  already follows.

### Fixed
- `parser`: a `type` alias declared beside an inline block is expanded inside it,
  and a trait's default methods are injected into impls written inside one. Both
  are parse-time erasures, so they stay file-scoped rather than stopping at a brace.

### Known limitations
- An inline block buys a private *surface*, not a private namespace: its items
  still collide with same-named declarations elsewhere in the program, because the
  merge is still flat.
- Only an item can be re-exported. `export import` naming a module or an enum
  variant is an error rather than a silent no-op.
- A nested block reaches its own children and the declaring file's siblings, not
  its parent's other blocks — there is no `super`.

## [1.73.0] - 2026-08-22

### Added
- `parser`: `export` visibility markers. A `func`, `struct`, `enum`, `trait`,
  `const`, or `newtype` declaration is private to its own module unless written
  with `export`, and each struct field carries the marker independently — an
  exported struct may still keep a field to itself. `export` goes between any
  `@derive(...)` attributes and the item keyword. It is rejected on an `impl`
  block (which declares no name of its own), on a `type` alias (expanded at parse
  time, so nothing of it survives to be reached), and on an `import`
  (`export import` re-export is a later item).
- `semantic`: struct field visibility. From another module a private field can be
  neither read nor assigned, listed in a struct literal, bound by destructuring,
  nor copied through the `..base` update form, which supplies every field the
  literal did not list. A monomorphized generic instance inherits its template's
  field visibility. New `PrivateField` diagnostic.
- `infra`: `module-resolution` enforces item visibility. It is the last pass that
  knows both which file declared a name and which file reaches for it, so the
  qualified-path and import routes are gated through one check; a module naming
  itself always passes, and `mod::Type::member` gates the type, since a method or
  a variant carries the visibility of the type it belongs to. New `PrivateItem`
  diagnostic.
- `infra`: `ast_types::ModuleId`. The merge is flat, so the file a declaration
  came from is otherwise unrecoverable downstream — and field visibility needs
  both it and the receiver's type. Module resolution stamps each `FunctionDef`,
  `StructDef`, `ImplDef`, and `ConstDef`; the parser alone leaves 0, so a
  single-file program is one module and the rule is inert there. HIR lowering and
  both backends still see nothing of modules.

## [1.72.1] - 2026-08-18

### Fixed
- `ci`: bounded the Linux LLVM install. GitHub's Ubuntu runners reach
  `archive.ubuntu.com` through an Azure mirror list that intermittently
  blackholes connections, and apt applies no network timeout by default, so
  `apt-get update` could wait indefinitely — the Benchmark Regression and Test
  Suite jobs sat in "Setup LLVM 20" for over an hour without failing. The
  composite action now writes an apt config with fetch retries and 20s
  transport timeouts, waits up to five minutes for the dpkg lock instead of
  aborting on it, and runs each apt invocation through a bounded three-attempt
  retry that repairs a half-unpacked dpkg state between tries. `wget` of the
  LLVM signing key is bounded the same way.
- `ci`: every job now carries a `timeout-minutes` cap. The GitHub default is six
  hours, so a stalled step previously burned the whole budget before anyone saw
  a red X.

## [1.72.0] - 2026-08-18

### Added
- `parser`: `import` declarations, in every form the module system specifies —
  `import math`, the explicitly relative `import ./utils::io`, the name list
  `import math::{sqrt, sin}`, the renames `import math::sin as sine` and
  `import math::sqrt as root`, the module alias `import math::matrix as mat`, and
  variant imports `import Option::{Some, None}`. New `Item::Import(ImportDef)`
  carries the written syntax and nothing more: whether a path segment names a
  module file, an item inside one, or an enum is a question about the file system,
  so module resolution answers it.
- `infra`: import binding in `module-resolution`. The new `imports.rs` walks each
  import's path greedily as modules and binds its selection against however far
  that reached, producing one `ImportScope` per file — module aliases, item
  renames, and variant bindings. An import's path is a discovery chain like any
  other, and each `{...}` entry extends it, so an import pulls its module into the
  build when no qualified path reaches it and a listed name may be a child module
  (`import ./utils::{io}`) as easily as an item. The rewriting pass consults the
  scope of the module it is walking, which is the one place the slice is not flat:
  two modules may bind one name differently, while one module may not bind one name
  twice.
- `parser`: unqualified variant patterns. A bare `Some(n)` parses as the new
  `Pattern::UnqualifiedEnum`, which module resolution rewrites into `Pattern::Enum`
  against the importing file's table; a payload-less `None` is written exactly like
  a catch-all binding, so it arrives as `Pattern::Binding` and is rewritten by the
  same table. A variant pattern no import accounts for is reported rather than
  silently read as a binding.
- `docs`: `examples/modules/imports.nr` demonstrates every import form over the
  existing `geometry` / `shapes` modules (exit code `40`), and the
  `examples/showcase/telemetry/` program now names its three modules through a name
  list, a rename, a module alias, and a variant import alongside structs, generics,
  arrays, `Vec<T>`, `Option`, and enums (exit code `75`, unchanged).

### Changed
- `docs`: the modules language reference gains an "Importing" section and the
  import diagnostics; the README, `docs/README.md`, and CONTRIBUTING no longer say
  there is no `import`.

## [1.71.0] - 2026-08-16

### Added
- `infra`: multi-file compilation. Every `.nr` file is a module and a directory
  holding a `mod.nr` is a module with children, so a program can span files and
  directories. The new `compiler/module-resolution` slice expands the root file
  into one item list: `resolve_program(root, parse)` loads each reachable module,
  verifies every qualified path against the module that owns the name, and strips
  the qualifier. Semantic analysis, HIR lowering, and both backends are unchanged
  — they still see a single-file program.
- `infra`: reference-driven module discovery. There is no `import` yet, so
  writing a qualified path is what pulls a module into the build:
  `math::sqrt`, `utils::io::read`, `geometry::Point` in value and type position
  alike. A directory of unrelated single-file programs therefore still compiles
  one program at a time. A leaf `math.nr` has no children; only a `mod.nr`
  directory opens a level, and a directory named in a path but missing its
  `mod.nr` is an error rather than a silent miss. A locally declared type wins
  over a same-named file, so `Point::new` keeps meaning the associated function.
- `parser`: paths accept more than two segments (`utils::io::read`) and type
  annotations accept a module qualifier (`geometry::Point`). No AST node changed
  — the qualifier rides inside the identifier until module resolution splits and
  erases it, and the `::<` turbofish check still wins at every step.
- `tests`: `compiler/neurc/tests/modules.rs` compiles and runs multi-file
  programs end to end — a sibling module supplying functions, structs, methods
  and constants; a directory module and its child; two modules referencing each
  other; a module enum matched from the root — plus the three module
  diagnostics. The slice carries its own unit tests against a stub parser.
- `docs`: `docs/language-reference/modules.md`, the `examples/modules/` teaching
  program (exit `40`), and the `examples/showcase/telemetry/` cumulative
  showcase (exit `75`) combining modules with structs, methods, a generic
  function, arrays, `Vec<T>`, an enum, `match`, and `??`.

### Changed
- `neurc`: `check` and `compile` no longer parse the input themselves; both call
  `resolve_modules`, which hands `syntax_parsing::parse` to the resolver. The
  parser is injected rather than imported because a feature slice may not depend
  on another — neurc is the one place the two meet. `check` now reports how many
  modules were loaded.
- `tests`: the example manifest accepts `module` in place of an exit code,
  marking a non-root module that is compiled as part of its root rather than on
  its own.

### Known limitations
- Modules share one flat namespace: a name declared by two loaded modules is a
  reported collision naming both files, not a silent winner. Per-module privacy
  needs `export`, which lands with the visibility item, as do `import`, inline
  `module { }` blocks, and re-exports.
- A `match` pattern takes the bare enum name (the flat namespace makes it reach);
  a module-qualified trait in `impl` / `dyn` position and the functional-update
  form `mod::Point { x: 1.0, ..base }` do not parse.
- Merged modules share one span space, so a panic diagnostic from a non-root
  module reports a position in the root file's coordinates — the same
  approximation the implicit prelude already carries.

## [1.70.0] - 2026-08-13

### Added
- `codegen`: error-path outlining. Every panic-family failure path — `panic`,
  `assert`, `unreachable`, and the array / `Vec` bounds, string-slice bounds, and
  UTF-8 codepoint-boundary guards — is emitted into a module-private cold
  function (`cold noreturn noinline minsize`), leaving a single call behind at
  the failure site. The diagnostic machinery is several `write(2, …)` calls plus
  `abort()` plus the string globals they reference, and it previously sat inline
  between the guard branch and the code that follows it, at every check. Thunks
  are deduplicated by their rendered diagnostic text, so the copies
  monomorphization makes of one generic body share a single thunk.
- `codegen`: `!prof` branch weights on every guard branch and on the `-O0`
  integer-overflow check, marking the failure edge as the improbable one so block
  placement keeps it off the fall-through path. The overflow check is weighted
  but not outlined — its trap block is a single `llvm.trap`, so a call would
  trade one instruction for another.
- `tests`: `compiler/neurc/tests/cold_outlining.rs` drives a failure through an
  outlined thunk at both `-O0` and `-O2` in programs combining collections,
  structs, methods, enums, and `match`, asserting the diagnostic text and the
  abort are unchanged — and that a runtime `panic` message, which travels to its
  thunk as a `(ptr, len)` pair rather than being baked in, still prints in full.

### Changed
- `codegen`: `abort` is declared `cold` alongside `noreturn`. A block whose
  terminator is `unreachable` is only treated as unlikely-executed by LLVM's
  placement heuristics when the call preceding it is itself marked cold.

## [1.69.0] - 2026-08-03

### Added
- `parser`: the error-propagation operator `?`. `expr?` unwraps an `Option<T>` /
  `Result<T, E>` to its success payload, or leaves the enclosing function
  immediately carrying the failure variant (`None` / `Err(e)`) on. It is postfix
  and binds as tightly as a call, so `f(x)? + 1` adds to the unwrapped payload
  and `parse(s)?.field` reads a field of it. New `TokenKind::Question` — logos'
  longest-match rule keeps `??` a single coalescing token.
- `semantic`: `?` requires the enclosing function to return the same fallible
  enum the operand is (`TryOutsideFallibleFunction`), and the operand itself to
  be an `Option` / `Result` (`TryOnNonFallible`). A `Result`'s error must already
  be the function's error type — the error is propagated as-is, with no implicit
  conversion — so a mismatch is an ordinary type error, resolved explicitly.
  Success payloads are unconstrained: `?` on a `Result<bool, E>` inside a
  `-> Result<i32, E>` function is fine.
- `docs`: `examples/operators/error_propagation.nr` and
  `examples/showcase/sample_audit.nr`, which propagates a validation error with
  `?` out of a `for`-in loop body alongside a `@derive(Copy)` struct with a
  `&self` method, arrays, `match`, `val-else`, and `??`.

### Changed
- `codegen`: unchanged. `?` desugars during HIR lowering into a two-arm `match`
  whose failure arm is a block terminating in a `return`, so both backends stay
  unaware of the operator — the same treatment `??` received.

## [1.68.3] - 2026-07-31

### Removed
- `ci`: the Dependabot configuration. It proposed bumps to the newest release of
  every dependency regardless of what the pinned toolchain supports, so
  dependency updates are made by hand from now on.

## [1.68.2] - 2026-07-31

### Fixed
- `tests`: the integration suite located the `neurc` binary by walking two
  directories up from the test executable, which hard-codes Cargo's legacy
  `target/<profile>/deps/` layout. Under Cargo's build-dir layout test binaries
  live elsewhere, so every compile-and-run test aborted with
  `Failed to execute neurc: NotFound` on Linux, macOS, and Windows alike. All
  fifteen lookups now use `env!("CARGO_BIN_EXE_neurc")`, Cargo's documented,
  layout- and platform-independent path to the built binary.
- `tests`: added an architecture test that fails if any `neurc` integration test
  reintroduces a `current_exe()`-derived compiler path.

### Documentation
- Added a Windows (MSVC) installation section to the getting-started guide,
  covering the LLVM 20 development-build requirement, the `/MD` CRT variant, the
  x86-only target set, and the matching troubleshooting entries. The guide now
  covers every platform CI builds and releases for.

## [1.68.1] - 2026-07-31

### Fixed
- `semantic`, `codegen`: a **nested** trailing `if`/`else` is now the value of the
  block that contains it. Only the function-body tail recognised a trailing `if` as
  value-producing, so an `if` written as the last thing inside another block — an
  if-branch, a bare block, a block with statements before it — lowered as a plain
  statement and whatever it evaluated to was discarded:

  ```neuro
  func classify(c: i32) -> i32 {
      if c <= 0 { 7 } else { if c > 50 { 3 } else { c } }   // returned garbage
  }
  ```

  The same shape returning a generic enum constructed the right variant with a
  **zeroed payload**, because a branch that is not a value position never inherits
  the enclosing function's return instance and so lowered against the erased
  template. Both are the one defect. The checker also typed such an `if` as `void`,
  making `val r = if a { x } else { if b { y } else { z } }` a spurious type error.

  The promotion now lives in exactly one place — hir-lowering, for the function-body
  tail and for every nested block alike — and the backend's separate copy of the rule
  is gone.

- `semantic`: a function may now be **called before it is defined**. Free-function
  signatures were registered by the body-checking pass itself, which runs in source
  order, so any forward reference was `undefined function` and mutual recursion could
  not be written at all:

  ```neuro
  func is_even(n: i32) -> bool { if n == 0 { true } else { is_odd(n - 1) } }
  func is_odd(n: i32) -> bool { if n == 0 { false } else { is_even(n - 1) } }
  ```

  Every other item kind — structs, enums, traits, constants — was already
  order-independent, and the LLVM backend already pre-declared every signature; only
  the type checker insisted on definition-before-use. Signatures are now registered in
  their own pass ahead of any body check.

- `codegen`: a loop no longer grows the stack once per iteration. Every local binding and
  every result or scratch slot was `alloca`'d at the current builder position, so a slot
  inside a loop body was allocated again on each pass and an ordinary counted loop
  segfaulted once it ran long enough:

  ```neuro
  mut i = 0
  while i < 2000000 {
      val v = i % 2      // one fresh stack slot per iteration
      i = i + 1
  }
  ```

  `mem2reg` could not rescue it — the pass only promotes allocas already in the entry
  block — so the crash reproduced at every optimization level, `-O0` through `-O3`. All
  such slots now go through a new `entry_alloca`, which allocates in the function's entry
  block and leaves the initializing store where it was. Recursion is unaffected (each call
  gets its own frame), and the loops become promotable, so they optimize as well.

- `codegen`: every `unsafe` block in the LLVM backend carries a `// SAFETY:` rationale
  naming the bound that makes its pointer arithmetic in-range.

### Known Issues
- `codegen`: a `&string` produced by `.slice(range)` and **returned from a function**
  points into the callee's dead frame, so a later call can clobber it. Pre-existing at
  v1.68.0 (verified against a clean tree). Consuming a slice in the function that produced
  it is correct; only returning one across a call boundary is affected. The real fix is to
  give `&string` the by-value `{ ptr, i64 }` representation the string ABI already
  documents, which is an ABI change and gets its own pass. This is also why the one
  `slice.slot` allocation is deliberately left out of the entry-block hoisting above — the
  code comment there explains it.

- `tests`: dropped an internal spec-section marker from a parser test comment, which
  the documentation hygiene check rejects.

## [1.68.0] - 2026-07-31

### Added
- `semantic`: `val-else` binding — `val PATTERN = value else |binding| { ... }`
  unwraps a refutable pattern or leaves the enclosing scope.

  ```neuro
  func doubled_or_error(raw: i32) -> i32 {
      val Result::Ok(value) = parse(raw) else |err| { return err }
      value + 1        // `value` is in scope for the rest of the block
  }
  ```

  The pattern's bindings live for the **rest of the enclosing block**, not just one
  arm — the difference from a one-armed `match`, and the reason the construct exists.
  The `else` branch must **exit the scope** (`return`, `break`, `continue`,
  `panic(...)`, `unreachable()`); one that can fall through is rejected with
  `ValElseMustDiverge`, which is what guarantees the binding is initialized on the
  path that continues.

  The optional `else |name|` is a dedicated production, **not** a closure literal, and
  what it names is decided by the scrutinee's type: a `Result<T, E>` binds the `Err`
  payload, an `Option<T>` binds nothing (only `|_|` or a bare `else` is accepted —
  `None` is empty, so a named binding is `ValElseBindingOnOption`), and any other enum
  binds the whole scrutinee, unmodified, for the else branch to discriminate with a
  nested `match`. Because `break` counts as leaving the scope, `val-else` is also the
  natural drain loop:

  ```neuro
  loop {
      val Option::Some(v) = next(i) else { break }
      total = total + v
      i = i + 1
  }
  ```

  Parsing keys off the two-token marker `val Name::` — a binding name is always
  followed by `:`, `=`, or a newline — so `val Point { x, y } = p` still desugars as a
  struct destructure. The pattern, its success test, and its bindings reuse the `match`
  machinery unchanged (`Pattern`, `HirMatchTest`, `HirMatchBinding`); the one new node
  is `Stmt::ValElse` / `HirStmt::ValElse`, a *statement* rather than a `Match` variant
  precisely because its bindings outlive it.

### Known Issues
- `docs`: a function returning a generic enum from a *nested* `if`/`else` tail
  constructs the right variant with a zeroed payload. Pre-existing at v1.67.0 (verified
  against a clean tree) and independent of this release; the examples use the
  early-`return` form as the workaround. Fixed in 1.68.1.

## [1.67.0] - 2026-07-28

### Added
- `semantic`: `??` full implementation — the operator unwraps an `Option<T>` or
  `Result<T, E>` to its payload and otherwise evaluates a fallback.

  ```neuro
  val present = lookup(1) ?? 0        // the Some payload
  val absent  = lookup(7) ?? 5        // 5 — the fallback
  val failed  = divide(1, 0) ?? 4     // 4 — the Err payload is discarded
  val chained = lookup(7) ?? lookup(1) ?? 99
  ```

  The expression's type is the unwrapped payload `T`, and the fallback must produce
  that `T`. For a `Result` the error payload is discarded — `??` says "I do not care
  why it failed, use this instead"; `match` remains the way to inspect it. The
  fallback is **lazy**: it is only evaluated when the left operand is absent or
  failed. `a ?? b ?? c` associates right-to-left, so every operand but the last must
  itself be fallible.

  `??` had been tokenized and parsed since v1.19.0 purely to pin its associativity;
  it is now type-checked and executable.

### Changed
- `semantic`: `check_binary_expr` routes `??` to `check_null_coalesce` before the
  shared operand check. `??` is the one binary operator whose operands are not
  symmetric — the right side is typed by the left's *payload*, not by the left — so
  the general path would have typed a bare fallback literal as an `Option`.
- `parser`/`codegen`: no change. HIR lowering desugars `lhs ?? fallback` into a
  two-arm `match` (success-tag test binding payload slot 0; wildcard → fallback), so
  no HIR node was added and neither backend learned anything about `??`. The
  fallback's laziness comes free from the per-arm basic-block chain the backend
  already emits for `match`.

### Removed
- `semantic`: the `OperatorNotYetSupported` diagnostic. `??` was its only user.

## [1.66.1] - 2026-07-28

### Fixed
- `codegen`: a `loop` in tail position — the function's implicit return — spun forever
  instead of exiting on `break`. Binding the same loop to a `val` and returning that
  worked, so the defect was in the tail path.

  ```neuro
  func count_to(n: i32) -> i32 {
      mut i: i32 = 0
      loop {
          if i == n { break i }
          i += 1
      }
  }
  ```

  Root cause: `loop` had two AST shapes — `Stmt::Loop` in statement position and
  `Expr::Loop` in value position — while every "is the trailing statement
  value-producing?" test in the pipeline keys on `Stmt::Expr`. A trailing `loop` was
  therefore lowered as a discard-value statement, and the backend terminated its exit
  block with `unreachable`, which emits no instruction at `-O0` and leaves the `break`
  target as a jump to itself.

  `Stmt::Loop` and `HirStmt::Loop` are removed. `Expr::Loop` is the sole loop node and
  carries the label; a statement-position `loop` parses to `Stmt::Expr(Expr::Loop)`, so
  every tail-value site sees it with no special case.

### Changed
- `semantic`: a `loop` that no `break` targets now takes its context's expected type.
  Such a loop never reaches its exit block — it runs forever or leaves via `return` — so
  it satisfies any declared return type, the same divergent contract the panic-family
  builtins carry. This keeps `func f() -> i32 { loop { ... return x } }` valid now that a
  trailing `loop` is checked as the implicit return.
- `parser`/`semantic`/`codegen`: split the nine largest modules along the seams they
  already had, with no behaviour change. `type_checkers/expressions.rs` and
  `declarations.rs`, `hir-lowering/expressions.rs`, `codegen/collections/maps.rs`, and
  the parser's `items.rs` and `statements.rs` each become a directory (or a set of
  siblings) holding one module per category, with the dispatch left in place. The three
  largest test modules are split by subject the same way.

## [1.66.0] - 2026-07-28

### Added
- `semantic`/`codegen`: `checked_add`, `checked_sub`, and `checked_mul` on every integer
  type. Each takes one same-typed argument and returns `Option<T>` over the receiver's
  own type — `Option::Some(result)` when the arithmetic fits, `Option::None` when it
  would overflow — so an overflow is reported to the caller instead of wrapping,
  clamping, or trapping.

  ```neuro
  val a: u8 = 200
  val total: u8 = match a.checked_add(100u8) {
      Option::Some(v) => v,
      Option::None => 0u8          // 300 does not fit in a u8
  }
  ```

  The result is the ordinary monomorphized prelude `Option<T>`, built through the same
  helper the fallible collection readers use, so it deconstructs with `match` and passes
  to any function taking that instance. Codegen is branchless: the LLVM
  `{s,u}{add,sub,mul}.with.overflow` intrinsic's overflow bit selects between the two
  variants, with signedness taken from the receiver type.

### Fixed
- `codegen`: the `mlir-backend` slice builds again under `--all-features`. Its HIR type
  mapping did not cover the `Collection` variant added with the standard collections, and
  because the crate compiles only behind the off-by-default `mlir` feature, the breakage
  surfaced on CI rather than in a default build. Collections now map to `!llvm.ptr` like
  every other aggregate, with a regression test pinning it.

## [1.65.1] - 2026-07-27

### Fixed
- `codegen`: an unsuffixed literal is emitted at the type the frontend resolved for it
  instead of the suffix default. `take(0.75)` where `take` wants `f32` no longer emits a
  `double` the LLVM verifier rejects, and the same applies in return position
  (`func half() -> f32 { 0.5 }`) and to integers reaching an `i64`/`u8` parameter.
- `parser`: a struct literal is accepted inside a delimiter pair in a guarded header —
  `match grid.get(Point { x: 3, y: 4 }) { ... }`, and the same in `if`, `while`, and both
  `for` forms. The guard that keeps `if x {` from reading `x {` as a struct literal is now
  lifted inside `( ... )` and `[ ... ]`, where a `{` is unambiguous, and restored on the
  way out — it is scoped rather than assigned, so nesting no longer drops it.
- `codegen`: a struct can be a free function's parameter and return type. The type mapper
  holds a struct-layout table, so a struct also works as a *field* of another struct, and
  a return-position `impl Trait` yielding a struct now compiles. A by-value `Drop`
  parameter of a free function is destroyed at function exit, as it already was for
  methods.
- `codegen`: chained field reads (`o.inner.v`) — an intermediate struct value is read
  with `extractvalue` rather than requiring a named binding to address.
- `semantic`: a move in a tail (implicit-return) expression is no longer reported as a use
  of the value it moved itself. The trailing expression was type-checked twice; it is now
  checked once, against the declared return type, which also stops tail-expression
  diagnostics from being emitted twice.

## [1.65.0] - 2026-07-27

### Added
- `semantic`, `codegen`: standard collections — `Vec<T>`, `HashMap<K, V>`, and
  `BTreeMap<K, V>`. They are specified as library types, but the language exposes no
  allocator and no raw pointers, so nothing in `.nr` source could implement them: all
  three are compiler-known, with a new `Type::Collection { kind, args }` in semantic
  analysis, `HirType::Collection` + `HirExprKind::CollectionNew` in the HIR, and a
  `codegen/collections/` module in the backend. None is `Copy` — assignment moves them,
  and each frees its buffer at scope exit.
  - `Vec<T>`: `Vec::new()`, `push`, `pop`, `get`, `len`, `clear`, `v[i]` read and write,
    and `for x in v`. Indexing is bounds-checked in *every* build, unlike `[T; N]`, whose
    length is a compile-time constant the optimizer can fold.
  - `HashMap<K, V>`: open-addressed with linear probing over `{ state, key, value }`
    slots, a power-of-two capacity, tombstoned removal, and a rehash at a 3/4 load factor
    measured on occupied slots — so a table churned by insert/remove reclaims its
    tombstones instead of growing its probe runs without bound.
  - `BTreeMap<K, V>`: key-sorted slots with binary-search lookup, so its entries are
    ordered. `keys()` returns a `Vec<K>` — ascending for the ordered map — which is how
    both maps are iterated.
  - Elements and values may be any `Copy` type or `string`. Keys are int-like or `string`
    (the compiler supplies equality, order, and hash), or a struct that supplies them
    itself via `impl PartialEq` plus `impl Hashable` or `impl Comparable`.
- `semantic`: `Hashable` — a compiler-known lang-item trait like `Drop` and the operator
  traits, holding exactly `func hash(&self) -> u64`. Write the `impl`, never a `trait`
  declaration.
- `semantic`: `OrderedF32` / `OrderedF64` in the prelude — validating wrapper structs whose
  `new` panics on NaN. A raw float key is now rejected with a diagnostic naming them:
  IEEE-754 comparison is a partial order, so a map keyed on a raw float could hold a key it
  could never find again.
- `tests`: `compiler/neurc/tests/collections.rs` — 18 end-to-end tests covering the method
  surface, `Option`-returning readers, growth and tombstone churn, ordered iteration, struct
  keys, `OrderedF32` keys, the move rule, and the bounds panic.
- `docs`: `examples/types/collections.nr` (exit 101) and
  `examples/showcase/inventory_ledger.nr` (exit 179), the latter combining all three
  collections with `impl` methods, an enum + `match`, `Option` matching, and arrays.

### Changed
- `codegen`: the drop machinery is no longer struct-only. `DropEntry` carries a
  `DropTarget` — a user `impl Drop for T`, or a collection whose buffer is `free`d — so a
  collection binding is destroyed through the same flag-guarded, move-aware path.
- `codegen`: `codegen_enum_construct` was split so `codegen_enum_value` can build a tagged
  union from already-evaluated values, which is how a fallible collection reader returns
  its `Option<T>`. The codegen context gained an enum variant-order table so a synthesized
  construction resolves `Some` / `None` by name rather than assuming a declaration order.

## [1.64.1] - 2026-07-26

### Changed
- `docs`: README trimmed to a shop window. The "Current Capabilities" table went from
  30 rows — several paragraph-length — to 14 one-line rows grouped by area; per-feature
  detail now lives in `docs/` and this changelog. The alpha memory-model warning was cut
  to three lines and given an explicit removal trigger. The status line no longer
  enumerates sub-phases: the Quick Roadmap table is the single public statement of
  progress, and the table of contents anchor that pointed at it was broken and is fixed.
- `docs`: the speculative `Tensor<f32, [784, 128]>` / `@grad(model)` snippet was replaced
  with `examples/showcase/closures.nr`, which compiles, links, and exits 90. README code
  blocks are now sourced from runnable example files only; unimplemented features are
  described in prose pointing at the roadmap.
- `ci`: `tools/check_docs_consistency.py` replaced by `tools/check_docs_hygiene.py`. The
  old gate existed only to keep three hand-written copies of the test total in agreement
  with each other. The new gate scans every tracked text file and fails on a hard-coded
  test count, a hard-coded workspace version in prose, a reference to a gitignored
  local-only path, or an internal spec-section marker.
- `docs`: the README test-count badge became a CI status badge — the workflow result is
  the only test claim that cannot go stale. Counts removed from `docs/README.md` and
  `docs/getting-started/installation.md` (which still said 806).

### Fixed
- `infra`: stripped internal spec-section markers that the closures work reintroduced
  into six tracked files. They resolve only against a document that is not published.

## [1.64.0] - 2026-07-26

### Added
- `parser`, `semantic`, `codegen`: **`Option<T>` and `Result<T, E>`.** The standard
  library's absence and failure types are now available in **every program without a
  declaration** — the compiler prepends an implicit prelude holding
  `enum Option<T> { Some(T), None }` and `enum Result<T, E> { Ok(T), Err(E) }`. A program
  that declares its own `Option` or `Result` shadows the prelude entry. Construct with
  `Option::Some(3)` / `Option::None` / `Result::Ok(v)` / `Result::Err(e)`, deconstruct with
  `match`, and pass or return them across functions and struct fields.
- `parser`, `semantic`, `codegen`: **Generic enums.** An enum may take type parameters
  (`enum Slot<T> { Filled(T), Vacant }`, `enum Tagged<T, U> { Left(T), Right(U) }`) — the
  machinery `Option` and `Result` are built from. Each distinct set of type arguments is
  monomorphized into its own nominal tagged union, so `Slot<i32>` and `Slot<i64>` are
  different types with their own payload widths and no runtime cost. Type arguments come
  from the expected type, from the payload (`Slot::Filled(4)` gives `T = i32`), or from the
  enclosing function's declared return type — the last of which is what makes the common
  fallible shape work, where a tail `if` branch has no other context:
  `func divide(a: i32, b: i32) -> Result<i32, i32> { if b == 0 { Result::Err(1) } else { Result::Ok(a / b) } }`.
  A `match` pattern names the base enum and binds payloads at the scrutinee instance's
  concrete types.
  - Diagnostics: a construction no context can complete, a generic enum's bare name used as
    a type, a wrong type-argument count, and a payload that is not a scalar (e.g.
    `Option<string>`) each report a clear, located error.
  - Not yet supported (planned): unqualified variant names (`Some(x)`), which arrive with
    the module system's prelude imports; the `?` and `??` operators; `impl` blocks on enums,
    and therefore helper methods such as `.map_err`; non-scalar payloads
    (`Option<string>`, `Option<Point>`); and lifetime parameters on an enum.
- `tests`: `examples/types/option_result.nr` (focused walkthrough of both types plus a
  user-declared generic enum) and `examples/showcase/sensor_pipeline.nr` — a lookup that may
  find nothing and a validation that may fail, combined with structs + `impl` methods, a
  borrowed struct parameter, arrays + `for`-in, a generic function used at two type
  arguments, and a guarded `match` arm.

### Fixed
- `codegen`: a `return` operand inside a function body was lowered to HIR without its
  contextual type (the lowering never recorded the body's declared return type), so an
  unsuffixed integer literal in `return 1` was typed `i32` regardless of the declared
  return type.

### Added (previously unreleased)
- `tests`: `examples/showcase/scan_guard.nr` — deterministic `Drop` and labeled loop breaks
  had showcase coverage nowhere, so both were only ever exercised in isolation. The new
  program runs two `Drop` guards over a shared `&mut i32` while a labeled `break` exits two
  nested loops at once, asserting the destructors still fire on that path, and combines it
  with a `Copy` struct + `&self` method, array indexing, and `match` range patterns.
- `tests`: the architecture tests now cover `mlir-backend` and `neuro-hir`, which were
  absent from every slice list and so were never checked for cross-slice imports or a
  present `CONTEXT.md`.

### Fixed (previously unreleased)
- `docs`: corrected claims that no longer matched the compiler. Operator overloading was
  documented as unsupported, trait bounds as unenforced, turbofish as unavailable, and
  generics/higher-order functions as unimplemented — all have shipped. Struct return types
  are now described accurately (rejected for free functions, allowed for associated
  functions and methods), as is consuming `self` (rejected only on non-`Copy` structs).
- `docs`: the quick-start feature summary was frozen at sub-phase 1D and omitted the type
  system, generics, traits, dispatch, closures and ownership; the README binary path in the
  usage example pointed at the wrong directory.

## [1.63.1] - 2026-07-25

### Fixed
- `codegen`: The MLIR scaffold (`mlir-backend`, feature `mlir`) failed to compile after
  closures landed — `lower_program`'s `HirItem` match did not cover the new
  `HirItem::Closure` variant. Lifted closures now emit a `func.func` declaration whose
  implicit first parameter is the captured-environment pointer, matching the LLVM
  backend's calling convention for `__closure_N`.

---

## [1.63.0] - 2026-07-24

### Added
- `parser`, `semantic`, `codegen`: **Closures and lambdas.** Anonymous callable values with
  the `|params| body` syntax — `|x: i32| x * x` (single-expression), `|x: i32| -> i32 { ... }`
  (block body with an explicit return type), the zero-parameter `|| expr`, and the `move`
  capture prefix. A closure captures the free variables its body reads from the enclosing
  scope **by value** (they must be `Copy` this phase); the captured variable stays usable
  afterwards because it is copied, not moved. Closure and function values have the type
  `(T1, ...) -> R`, so a **higher-order function** can take one as a parameter and call it:
  `func apply(v: i32, f: (i32) -> i32) -> i32 { f(v) }`. Each closure compiles to a
  `{ function pointer, environment pointer }` value with no heap allocation.
  - Diagnostics: capturing a non-`Copy` value, assigning to a captured variable, an
    unannotated parameter, and a block body without a return type each report a clear,
    located error.
  - Not yet supported (planned): closure parameter-type inference (`|x| x*x` without an
    annotation), passing a closure to a *generic* higher-order function `f<T, U>(.. : (T)->U)`,
    the `Fn`/`FnMut`/`FnOnce` distinction with by-reference/mutable/`move`-of-owned capture,
    and returning or storing a closure so it escapes its defining scope.

---

## [1.62.3] - 2026-07-19

### Fixed
- `semantic`: `__` is now reserved for compiler-generated symbols. Methods live in the flat
  function table as `Receiver__method` and the backend recovers a method's receiver by
  splitting that symbol on `__`, so the separator has to appear exactly once. Two things
  could break the property: a monomorphized *function* instance carried its own `__` marker
  (`identity__g_i32`), and a user-declared name was free to contain one. The instance marker
  is now the single-underscore `_g_` already used for generic-struct instances
  (`identity_g_i32`), and any declared name containing `__` is rejected with
  `ReservedNameSeparator`. Without this, a generic method on a generic struct — the next
  combination on the way — could mint a symbol already spoken for.
- `tests`: `type_checkers/tests/{decl,expr,stmt}_tests.rs` were never declared as modules, so
  ~40 type-checker tests had not run since they were written. Declared them (all pass) and
  deleted the empty `stmt_tests.rs` stub.
- `lexer`: added `dyn` to the editor grammar's keyword pattern, and `f16`/`bf16` to its
  primitive-type and numeric-suffix patterns — all three were in the lexer but unhighlighted.

### Added
- `tests`: `lexical-analysis/tests/tmlanguage_sync.rs` links the lexer to the editor's
  TextMate grammar, which nothing in the build referenced before. It scans `tokens.rs` for
  `#[token("…")]` literals and fails when a keyword is absent from the grammar, so a new
  keyword can no longer land silently unhighlighted. Keyword words only — string-body and
  escape rules still need hand-updating, which is what the coming stateful-lexer rewrite for
  string interpolation will touch.

### Changed
- `docs`: `docs/compiler/components/semantic-analysis.md` described a three-pass type checker
  and separately claimed single-pass checking; `check_program` has run many more passes than
  that for several sub-phases. Replaced with the real pass table (including the lettered
  sub-passes) plus a section on method-name mangling. `docs/README.md` gave three different
  answers about the active sub-phase; all now read 1F.

---

## [1.62.2] - 2026-07-19

### Changed
- `docs`: removed the internal spec section markers (`(§3.17)` and friends) from every
  tracked file outside the private notes directory. The references pointed at a document
  that ships with neither the repository nor the docs site, so in code comments,
  diagnostics, `CONTEXT.md` files, examples, and the changelog they carried no meaning for
  a reader. Prose was rewrapped at the removal sites only; wording, diagnostics text
  (beyond the dropped marker), and behavior are unchanged. License section references
  (`§ 12.3`), which point at real clauses in `LICENSE`, are untouched.

---

## [1.62.1] - 2026-07-19

### Fixed
- `codegen`: the MLIR scaffold lowering handled neither the `HirItem::Trait` item nor the
  `HirType::DynObject` type introduced with trait dispatch, so `mlir-backend` failed to
  compile under `--all-features`. Trait items now lower to nothing (their methods reach
  the module through implementors' `impl` blocks), and an unsized `dyn Trait` in value
  position reports `UnsupportedType` — it is only ever the referent of a reference.

---

## [1.62.0] - 2026-07-19

### Added
- `parser`: static and dynamic dispatch. A trait bound can now be satisfied two ways, and
  the keyword chooses. `impl Trait` works in argument position
  (`func train(m: &impl Model)`) and return position (`func make() -> impl Shape`); the
  new `dyn` keyword introduces a trait object, written `&dyn Trait` or `&mut dyn Trait`.
- `parser`: each `impl Trait` parameter is its own anonymous type parameter, so a single
  call may bind two different concrete types — unlike one shared `<T>`.
- `semantic`: `impl Trait` is static dispatch and costs nothing at runtime. In argument
  position it is shorthand for a trait-bounded type parameter and is monomorphized like
  one. In return position it resolves to the single concrete type the body constructs,
  which is checked to implement the trait, so the caller receives that type directly.
- `semantic`: object safety is enforced for trait objects. Every method of a trait used
  as `dyn` must take `&self` or `&mut self`, so the method table has a fixed layout; a
  method that consumes `self` or takes no receiver is reported with the offending method
  named. A bare `dyn Trait` is rejected as unsized, with the `&dyn Trait` form suggested.
- `codegen`: dynamic dispatch through a method table. `&dyn Trait` is a two-word value
  carrying a pointer to the data and a pointer to the implementing type's method table,
  so one function body can serve values of different concrete types, including reaching
  an inherited default method on one type and an override on another. Passing `&T` where
  `&dyn Trait` is expected converts automatically.
- `docs`: new `types/dispatch.nr` example contrasting the two dispatch forms, an extended
  `showcase/shape_traits.nr` combining both with traits, generics, structs, and arrays,
  and a "Static and Dynamic Dispatch" section in the function reference.

---

## [1.61.0] - 2026-07-18

### Added
- `parser`: operator overloading. Operators on a custom type are now sugar for method
  calls. A `Copy` struct can implement `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`, `Not`,
  `BitAnd`, `BitOr`, `BitXor`, `Shl`, `PartialEq`, and `Comparable` to get `+`, `-`, `*`,
  `/`, `%`, unary `-`, `~`, `&`, `|`, `^`, `<<`, `==`/`!=`, and `<`/`<=`/`>`/`>=`. The impl
  declares its result with `type Output = T`; the user writes only the `impl` (these are
  built-in traits, no declaration needed).
- `semantic`: operator-trait checking. The receiver must be `Copy`; a `type Output` must
  match the method's return type; and `Comparable` requires `PartialEq` on the same type.
  Using an operator on a type without the matching impl is still a clear error.
- `codegen`: operator overloading carries no runtime cost. Each operator lowers to the
  ordinary method call it stands for and is monomorphized to a plain call — no vtable.
- `docs`: new `operators/operator_overloading.nr` example and `showcase/vector_physics.nr`
  showcase (a `Vec2` with `+`/`-`/unary `-`/`==` driving a small physics step loop).

### Notes
- Compound assignment on a custom type (`v += w`) works through the by-value operator (it
  desugars to `v = v + w`). The dedicated in-place `*Assign` traits, matrix-multiply `@`,
  and auto-derived comparison default methods are tracked for later phases.

---

## [1.60.0] - 2026-07-16

### Added
- `parser`: trait declarations. A `trait` groups method signatures that types can
  implement — either **required** methods (a signature with no body) or **default**
  methods (a signature with a body that implementors inherit). Implement one with
  `impl Trait for Type`. An omitted default method is inherited automatically; writing it
  explicitly overrides the default.
- `semantic`: trait-impl conformance checking. A trait implementation must provide every
  required method, may only contain methods the trait declares, and each method's
  signature must match the trait's — otherwise the compiler reports a precise error
  (missing method, non-member method, signature mismatch, or unknown trait).
- `semantic`: trait bounds on generics are now enforced. A generic parameter bounded by a
  trait (`func f<T: Shape>(x: &T)`) may call the trait's methods on the parameter, and the
  call site is checked to ensure the concrete type argument implements the trait.
- `codegen`: traits carry no runtime cost. Each implementation lowers to ordinary methods
  and each trait-bounded generic is specialized per concrete type — there is no vtable and
  no pointer indirection. (Supertraits, associated types, dynamic dispatch, and the
  operator traits are planned follow-ups.)

---

## [1.59.0] - 2026-07-13

### Added
- `parser`: explicit lifetime annotations. A lifetime parameter is declared in the
  generic-parameter list as `'a` and used on reference types as `&'a T` / `&'a mut T`,
  as in the canonical `func longest<'a>(a: &'a string, b: &'a string) -> &'a string`.
  Lifetimes may be mixed with type and const parameters (`func f<'a, T>(...)`); only the
  type/const parameters drive monomorphization.
- `semantic`: lifetime well-formedness checking. Every lifetime used on a reference must
  be declared in the enclosing generic-parameter list; an undeclared lifetime is a compile
  error. Annotations are then erased — `&'a T` and `&T` are the same type, so a lifetime
  carries zero runtime cost and never changes which values a signature accepts. The
  existing returned-reference outlives analysis does the real borrow checking.

---

## [1.58.1] - 2026-07-10

### Changed
- `build`: bump patch-level dependencies `log` 0.4.32→0.4.33,
  `env_logger` 0.11.10→0.11.11, `clap_complete` 4.6.5→4.6.7, and
  `cc` 1.2.63→1.2.66 (lockfile only; all within existing version requirements).
- `ci`: pin Dependabot away from auto-bumping `logos` and `melior`, which are
  held back deliberately (logos 0.16 is a breaking lexer-engine rewrite; melior
  0.26+ targets MLIR 21/22 while the compiler is built against LLVM/MLIR 20).

---

## [1.58.0] - 2026-07-10

### Added
- `parser`/`semantic`/`codegen`: **const generic parameters, `where` clauses, and
  turbofish**. A generic parameter list may now declare a compile-time *value*
  parameter — `func sum<const N: u32>(a: [i32; N]) -> i32` and
  `struct Buffer<T, const CAP: u32> { data: [T; CAP] }` — usable as an array length and
  as a value in the body. Each distinct value is monomorphized into its own specialized
  code, so a const parameter carries zero runtime cost. Const parameters are inferred
  from array-argument lengths (or a struct literal's field values) and may also be
  supplied explicitly by a **turbofish** — `identity::<i32>(x)`, `zeros::<4>()` — the
  only call-site form for explicit generic arguments, useful when inference cannot reach
  a parameter. A **`where` clause** keeps complex signatures readable: it carries trait
  bounds (parsed, still unenforced until traits land) and **value predicates** over const
  parameters (`where N > 0`), which are checked at every instantiation and reported at the
  offending call. Const parameter types must be an integer type; type arguments remain
  restricted to `Copy` this phase.

### Changed
- `semantic`: a generic type parameter that cannot be inferred from a call's arguments is
  now reported at the **call site** (supply it with a turbofish) rather than at the
  function declaration.

---

## [1.57.0] - 2026-07-06

### Added
- `parser`/`semantic`/`codegen`: **generic structs and generic impls** with
  monomorphization. A struct may declare type parameters —
  `struct Pair<T, U> { first: T, second: U }` — and an inherent `impl` block may be
  generic — `impl<T> Wrapper<T> { func get(&self) -> T { self.value } }`. Each distinct
  set of concrete type arguments produces its own specialized struct and methods, so
  type parameters carry zero runtime cost. Type arguments are inferred from the field
  values at a struct literal (`Pair { first: 1, second: 2.0 }`) or written explicitly
  in a type annotation (`Pair<i32, f64>`). A generic struct is usable only with type
  arguments; its bare name is rejected. Type arguments are restricted to `Copy` types
  this phase, matching generic functions.

---

## [1.56.0] - 2026-07-03

### Added
- `parser`/`semantic`/`codegen`: **generic functions** with monomorphization.
  A function may declare type parameters in angle brackets —
  `func identity<T>(x: T) -> T { x }`, `func choose<T>(c: bool, a: T, b: T) -> T`,
  multiple parameters `<T, U>` — and the compiler emits one specialized copy per
  distinct set of concrete type arguments, so a type parameter carries zero
  runtime cost. Type arguments are inferred from the call's value arguments (for
  example `identity(5)` selects the `i32` instance and `identity(2.0)` the `f64`
  instance). Trait bounds (`<T: Bound>`) parse but are not yet enforced (the trait
  system is a separate, later feature), so a generic body may use only operations
  valid for any type — binding, returning, passing to another function, and
  building/observing tuples. Type arguments are restricted to `Copy` types this
  phase. Generic structs, generic `impl` blocks, const (value) parameters, `where`
  clauses, and explicit turbofish type arguments are tracked as follow-on work.
- New showcase example `showcase/generic_toolkit.nr` combining generic functions
  with enums, pattern matching, and tuples.

### Changed
- `codegen`: the LLVM backend now declares every function/method signature in a
  pre-pass before emitting any body, so a call resolves regardless of definition
  order — required so a monomorphized generic instance can be called by, or call,
  items that appear before it.

---

## [1.55.1] - 2026-07-03

Documentation-accuracy audit follow-up. No code or behavior change.

### Fixed
- `docs`: resolved three `cargo doc` intra-doc-link warnings so the workspace
  documentation builds cleanly. The `mlir-backend` crate-level docs referenced
  `lower_program` / `emit_smoke_module` (both `#[cfg(feature = "mlir")]`-gated, so
  unresolvable in the default doc build) and `semantic-analysis`'s public
  `Type::is_float` linked to the `pub(crate)` `Type::is_half_float`; all three are
  now plain code spans.
- `docs`: refreshed the `README.md` **Workspace Layout** diagram, which had gone
  stale — it now lists every workspace member, including the `neuro-hir`,
  `source-location`, and `project-config` infrastructure crates and the
  `hir-lowering` and `mlir-backend` slices added in sub-phase 1D.
- `docs`: corrected the `README.md` compilation-pipeline diagram to show HIR
  lowering (`neuro-hir`) in the **current** Phase-1 path — the LLVM backend has
  consumed typed HIR since v1.49.0 — instead of listing HIR only under the
  Phase-2+ planned extension.
- `docs`: updated `AGENTS.md` (the `@compiler-dev` scope and `@architect`
  infrastructure-crate list now include `hir-lowering`, `mlir-backend`, and
  `neuro-hir`) and `DESIGN.md` (the memory-model paragraph now reflects landed
  move/borrow/`Drop` semantics rather than a blanket "heap is leaked").
- `docs`: refreshed stale test-count references (746 / 727 → **806**) in
  `docs/compiler/compilation.md` and `docs/getting-started/installation.md`.

---

## [1.55.0] - 2026-07-02

### Added
- Newtype declarations. `newtype Name = T` creates a **distinct nominal type**
  that wraps an inner type — unlike a transparent `type` alias, the newtype and its inner
  type are not interchangeable, so `Meters` and `Seconds` over `i32` are different types.
  Construct a value with `Name(value)` and read the wrapped value back with `.0`. A newtype
  forwards `Copy`/`Clone` from its inner type and may be a binding, a function parameter or
  return type, and a struct field. New `newtype` keyword, `Item::Newtype` AST, semantic
  `Type::Newtype` (name→inner table with builtin/nominal collision, cycle, and non-Copy-inner
  rejection), typed-HIR `HirType::Newtype { name, inner }` plus transparent
  `NewtypeConstruct` / `NewtypeAccess` nodes, and full backend erasure (a newtype lowers to
  its inner type at zero runtime cost). **Completes sub-phase 1E (Type System Expansion).**
  Phase-1E limit: the inner type must be `Copy` (non-Copy wrappers, and operator/trait impls
  on a newtype, await 1F+).

---

## [1.54.0] - 2026-07-02

### Added
- Pattern matching. `match` is an exhaustive expression that deconstructs a
  value. Patterns cover the `_` wildcard, a bare binding, literals, inclusive/exclusive
  ranges (`a..=b` / `a..b`) over ordered scalars, `|` or-patterns, and enum variant
  patterns (`E::Unit`, `E::Tuple(a, b)`, `E::Struct { field }`) that bind their payload.
  Arms may carry an `if` guard; all arm bodies unify to one type and a `match` is usable
  as a value or in statement position. Exhaustiveness is enforced: an enum match must
  cover every variant or add `_`; an integer/`char` match needs `_`; a `bool` match needs
  both `true` and `false` (or `_`); guarded arms do not count. New `=>` (`FatArrow`) token;
  new `Expr::Match` / `Pattern` AST, `HirExprKind::Match` typed-HIR node, and a
  test-block-chain codegen with enum-payload decode. Phase-1E limits: the scrutinee must be
  an enum / integer / `char` / `bool`; enum-payload sub-patterns must be a binding or `_`
  (match a payload value with a guard); `|`-alternatives may not bind.

---

## [1.53.1] - 2026-06-30

### Security
- Bump `anyhow` 1.0.102 → 1.0.103 to resolve RUSTSEC-2026-0190
  (unsoundness in `Error::downcast_mut()`). Clears the OSV-Scanner CI
  failure on `--fail-on-vuln`.

---

## [1.53.0] - 2026-06-30

### Added
- Enums with associated data. `enum Foo { Bar, Baz(i32), Qux { x: f64 } }`
  declares a tagged union of unit, tuple, and struct-field variants. Construction
  uses `Foo::Bar` (unit), `Foo::Baz(1)` (tuple), and `Foo::Qux { x: 1.0 }` (struct).
  Enum values bind to `val`/`mut`, cross function boundaries (parameter and return),
  and live in struct fields; an enum is `Copy`. Backends lower each enum to a tagged
  union `{ i32 tag, [W x i64] payload }`, packing each scalar payload field into its
  own 64-bit slot. New `Item::Enum` / `EnumDef` / `VariantPayload` AST nodes,
  `Expr::EnumStructLiteral` for the brace form, nominal `Type::Enum` / `HirType::Enum`,
  and the single `HirExprKind::EnumConstruct` node all three forms normalize to.
  Limitations (Phase 1E): variant payloads are restricted to scalar `Copy` primitives,
  enums are non-generic, and deconstruction (reading a variant) awaits pattern
  matching (the next 1E item).

---

## [1.52.0] - 2026-06-29

### Added
- Struct and array destructuring patterns. `val Point { x, y } = p` binds
  each named struct field; `val [a, b, c] = arr` binds array elements positionally;
  `val [first, second, ..rest] = arr` captures the remainder as a fresh
  `[T; N - k]` array, and a bare `..` ignores it. A rest-less array pattern must
  match the array's length exactly (`ArrayPatternLengthMismatch` otherwise).
  Patterns nest and `mut` makes the bindings mutable. Like tuple destructuring,
  these are parse-time desugars; the only new node is the internal `ArrayRest`
  remainder, threaded through HIR and lowered to a sub-array copy in the backend.

---

## [1.51.2] - 2026-06-29

### Changed
- Cleaned up comments across the compiler: removed repeated file-header banners
  and VSA boilerplate, trimmed verbose crate/function doc blocks to their useful
  core, and dropped inline comments that merely restated the code. Substantive
  "why" comments and spec references were preserved. No behavioral change.

---

## [1.51.1] - 2026-06-29

### Changed
- `ci`: include the mlir-backend `mlir` feature in the coverage job. The `coverage` job now provisions MLIR 20 (`setup-llvm` with `mlir: "true"`) and runs `cargo tarpaulin --workspace --features mlir`, so the Phase 1.8 HIR→MLIR lowering is measured instead of reporting as 0% covered.

---

## [1.51.0] - 2026-06-28

### Added
- `parser`: tuples and destructuring (Phase 2A). Adds the anonymous tuple type `(T1, T2, ...)`, tuple literals `(e0, e1, ...)` (a single `(x)` stays grouping), constant element access `t.0` / `t.1`, and destructuring binds `val (a, b) = e` — with `_` wildcards and arbitrary nesting (`val ((a, b), c) = ...`). Tuples cross function boundaries, so a function may take and return them (`func swap(a, b) -> (i32, i32)`). The feature spans the pipeline: new `Type::Tuple` / `Expr::TupleLiteral` / `Expr::TupleIndex` AST nodes, the `HirType::Tuple` / `HirExprKind::TupleLiteral` / `HirExprKind::TupleIndex` typed-HIR mirror, and an LLVM lowering to an anonymous struct (`insert_value` for a literal, `extract_value` for an index). Destructuring needs no new node downstream — the parser desugars it to a fresh temp binding plus one projection per leaf, so semantic analysis, HIR, and codegen see ordinary bindings. Tuple elements are restricted to `Copy` types for now (so a tuple is itself `Copy`), mirroring the array element rule; non-Copy element tuples (e.g. `(i32, string)`) are a documented follow-on, as are struct/array destructuring patterns. New diagnostics: `NonCopyTupleElement`, `NotATuple`, `TupleIndexOutOfBounds`. 762 tests pass (+16); new example `examples/types/tuples.nr` (exit 62).

---

## [1.50.1] - 2026-06-28

### Changed
- `docs`: sync the user-facing `docs/` tree with the Phase 1.8 backend pipeline. Rewrote `docs/compiler/compilation.md`, which had drifted to LLVM 18.1.8 / `LLVM_SYS_181_PREFIX` / inkwell 0.6.0 and a four-stage pipeline: it now documents the real **AST → typed HIR → LLVM** flow (the `hir_lowering::lower_program` stage, the `check` vs `compile` split, the actual `compile(&hir, …, source, source_path)` signature), replaces the "tests (planned)" / "known bug" sections with the real 746-test suite, and points toolchain setup at the installation guide instead of duplicating a stale LLVM 18 walkthrough. Updated `docs/README.md` (main pipeline diagram now shows the HIR lowering stage; component index gains HIR Lowering / MLIR Backend) and `docs/compiler/components/llvm-backend.md` (input is the typed HIR, not the AST; corrected entry-point signature, dependency list, and example). Added two new component docs: `docs/compiler/components/hir-lowering.md` and `docs/compiler/components/mlir-backend.md`. Also corrected the lingering LLVM 18.1.8 / `LLVM_SYS_181_PREFIX` references in `docs/guides/cli-usage.md` and `docs/guides/troubleshooting.md` to LLVM 20 / `LLVM_SYS_201_PREFIX`. No compiler code changed.

---

## [1.50.0] - 2026-06-28

### Added
- `infra`: scaffold the `mlir-backend` HIR lowering path (Phase 1.8, final item). The `mlir`-gated slice gains `lower_program(&HirProgram) -> Result<String, MlirError>`: it walks the typed HIR and emits a trivial, verifier-clean MLIR module — one `func.func` *declaration* (empty region, private visibility) per free function and per `impl` method — proving the HIR → `melior` → verified MLIR pipeline end-to-end. HIR types map to their MLIR scalars (`i8`–`i64`, `i1` for `bool`, `i32` for `char`, `f16`/`bf16`/`f32`/`f64`); every aggregate / reference / string type maps to an opaque `!llvm.ptr` until real tensor and struct lowering arrives (Phase 3+). A method receiver lowers to a pointer parameter; `void` is the empty result list in return position and a new `MlirError::UnsupportedType` anywhere else. Function bodies are intentionally **not** lowered yet — that is the Phase 3 linalg/tensor work. The slice gains an `mlir`-gated infrastructure dependency on `neuro-hir` (the typed HIR contract it consumes), keeping the default placeholder build free of both MLIR and `neuro-hir`. The pre-existing HIR-independent `emit_smoke_module` wiring check is retained. CI now runs `cargo test -p mlir-backend --features mlir` on the MLIR-provisioned Linux job, so the scaffold is exercised, not just clippy-checked. Three new unit tests cover free-function lowering, method-with-receiver / `void`-return lowering, and scalar type mapping. **This completes Phase 1.8** (HIR & MLIR backend plumbing); the default `cargo test --workspace` stays at 746 (the `mlir` tests are feature-gated).

---

## [1.49.0] - 2026-06-28

### Changed
- `codegen`: migrate the LLVM backend off the AST onto the typed HIR (Phase 1.8 item 4). The `compile` entry point now takes `&neuro_hir::HirProgram` instead of `&[ast_types::Item]`; `neurc` passes the lowered HIR straight to the backend. Because every HIR node carries its resolved type (`HirExpr::ty`), the backend reads types inline instead of re-deriving them, so the entire codegen-side type-collection pass is **deleted**: `codegen/type_pass.rs` (≈880 lines) and the span-keyed side tables it populated (`expr_types`, `binary_left_types`, `builtin_methods`, `index_object_types`, `fa_struct_names`, `tp_loop_*`, `global_const_types`) are gone. `CodegenContext` keeps a single `type_env` (name → resolved type), now populated as bindings are lowered, solely so the place statements `obj.field = …` and `arr[i] = …` can recover a binding's nominal struct/array type. Value-producing `if`/`loop` and array/index nodes take their result type from the HIR node (a tail `if` used as an implicit return takes the function's return type); builtin-method dispatch resolves from the receiver's `object.ty` at the call site, mirroring the former type pass. The backend gains an infrastructure dependency on `neuro-hir` and a dev-dependency on `hir-lowering` (tests/benches lower before compiling, mirroring the existing `syntax-parsing` dev setup). No language-surface or behavioral change: all 746 workspace tests pass against the HIR-routed backend, and the example binaries are byte-for-byte equivalent. Acceptance for the roadmap item ("full test suite passes against the HIR-routed backend") is met. The remaining Phase 1.8 item is the `mlir-backend` HIR scaffold.

---

## [1.48.0] - 2026-06-27

### Added
- `infra`: implement the AST → HIR lowering strategy (Phase 1.8 item 3). New `compiler/hir-lowering` feature slice with the entry point `lower_program(items: &[Item]) -> Result<HirProgram, LoweringError>`: it consumes the type-checked surface AST and produces the typed HIR (`neuro-hir`) every backend will lower from. Because the HIR's defining property is that **every expression carries its resolved type** and the frontend type checker does not expose those types, the slice **re-derives** each expression's type from the AST rather than importing `semantic_analysis::Type` — importing it would couple two feature slices, which VSA forbids (duplication over coupling). Lowering assumes well-typedness (it computes types, not validates them) and a shape the checker should have rejected surfaces as a `LoweringError` instead of a panic. It faithfully mirrors the checker's contextual rules: literal inference (suffix → expected-type → `i32`/`f64` default), a body's trailing expression typed against the declared return type, builtin-method dispatch (`string`/array `.len()`, `.clone()`, `.slice(range)`, integer `wrapping_*`/`saturating_*`/`.shr`), struct/associated-method signatures via mangled keys, and `loop`-as-value typing via a loop-context stack. Three nodes carry a deliberately-chosen type the source has no first-class form for (a `loop` value-expression takes its `break v` type; a method-name callee carries the call's result type; a `Range` carries `void`), and divergent panic-family calls adopt their context's expected type. `neurc` runs the lowering after type-check in both `check` and `compile`; the LLVM backend still consumes the AST — the backend migration onto HIR is the next roadmap item. New slice `CONTEXT.md`, 11 slice unit tests, and `neurc/tests/hir_lowering.rs` end-to-end coverage; the architecture test enforces the slice's infrastructure-only dependencies.

---

## [1.47.0] - 2026-06-26

### Added
- `infra`: introduce the `neuro-hir` typed HIR infrastructure crate (Phase 1.8 item 2). New `compiler/infrastructure/neuro-hir` defines the typed High-Level IR — the stable, backend-agnostic contract between the frontend (parser + type checker) and all backends (`llvm-backend`, `mlir-backend`). The HIR mirrors the surface AST (`ast-types`) one-to-one with two defining differences: every expression node carries its resolved type (`HirExpr { kind, ty, span }`, with a `HirType` that has **no `Unknown` variant** — reaching the HIR implies the program type-checked), and syntactic noise is normalized away (the `Expr::Paren` grouping node is dropped). `HirType`'s variant set mirrors the resolved types the semantic analyzer produces today; no tensor/generic variants are added ahead of those language features. The crate is pure data (like `ast-types`), depending only on `shared-types` (`Span`, `Literal`) and `ast-types` (the `BinaryOp` / `UnaryOp` operator enums, reused unchanged). The AST → HIR lowering (item 3) and the backend migration onto HIR (item 4) are separate, later pipeline steps and are intentionally not part of this crate.

---

## [1.46.0] - 2026-06-19

### Added
- `infra`: integrate `melior` (Rust MLIR bindings) alongside inkwell (Phase 1.8). New `compiler/mlir-backend` slice hosts `melior 0.25.1` — the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`; melior 0.26+ moved to MLIR 21/22) — behind an off-by-default `mlir` feature. With the feature disabled the crate is an empty placeholder, so the default `cargo build/test --workspace` still works on a stock LLVM 20 install with no MLIR toolchain; enabling `mlir` pulls in melior and exposes `emit_smoke_module`, which builds and verifies `func.func @neuro_smoke(index, index) -> index` (func/arith dialects) as the integration smoke test. `mlir-sys` carries no `llvm-sys` dependency and links its own `MLIR` key, so it coexists with inkwell's `llvm-20` link without a Cargo `links` conflict; pointing `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings share one `libLLVM-20` dylib. CI gains an opt-in MLIR + matching libclang 20 install on Linux (`setup-llvm` `mlir` input) for the `--all-features` lint job; the Windows/macOS test legs build the placeholder. The HIR-consuming lowering entry point and a dedicated CI smoke job remain open (pending typed HIR, Phase 1.8 items 2–5).

---

## [1.45.0] - 2026-06-19

### Added
- `codegen`: fixed-size arrays `[T; N]` (Phase 2A). Array types `[T; N]`, array literals `[e0, e1, ...]` (with element-type inference and length from the element count), index read `arr[i]` and element assignment `arr[i] = v`, the `arr.len()` builtin (compile-time `u64` length), and direct array iteration `for x in arr` / `for x in &arr` (lowered as a counted loop over the storage — no iterator protocol). Element types are restricted to `Copy` scalar primitives (i8–u64, f16/bf16/f32/f64, bool, char), so an array is itself `Copy` and needs no move/Drop tracking. Out-of-bounds index access panics with a located diagnostic in debug builds (`-O0`); release builds omit the check, matching the integer-overflow policy. New diagnostics: `NonCopyArrayElement`, `NotIndexable`, `IndexNotInteger`, `ArrayLengthMismatch`, `CannotInferEmptyArray`. New AST nodes (`Type::Array`, `Expr::ArrayLiteral`, `Expr::Index`, `Stmt::ForEach`, `Stmt::IndexAssignment`), semantic and backend `Type::Array`, `BuiltinMethod::ArrayLen`, `examples/types/arrays.nr`, end-to-end coverage in `neurc/tests/arrays.rs`, and semantic unit tests. Deferred: arrays of non-`Copy` elements (strings, structs), `.enumerate()` indexed iteration (needs tuples), and a compile-time bounds-elision pass.

---

## [1.44.0] - 2026-06-18

### Added
- `codegen`: `Drop` trait + deterministic destruction (Phase 1.7). `impl Drop for T { func drop(&mut self) { ... } }` defines a destructor that runs automatically when an owned binding of `T` leaves its lexical scope, in reverse declaration order, on **normal** exit only — fall-through, `return`, `break`, and `continue`. A panic aborts without running destructors (no stack unwinding, no landing pads). `Drop` is recognized as a compiler-known lang-item (like `Copy`/`Clone`), without the general trait system: the parser accepts `impl TraitName for Type` (`ImplDef::trait_name`); semantic analysis validates the destructor shape (`drop(&mut self)`, no params, no return → `InvalidDropImpl`) and rejects a `Copy` type that implements `Drop` (`DropTypeCannotBeCopy`). The backend (`llvm-backend/src/codegen/drops.rs`) tracks a lexical drop-scope stack and inserts flag-guarded `{struct}__drop` calls at scope exit; each owned binding carries an `i1` drop flag, cleared at every move site, so a value moved out (rebind, return, `break` value, by-value call argument, struct-field store) is dropped exactly once — never double-dropped. All drop machinery is inert for programs that declare no `Drop` types. New `examples/types/drop.nr`, `neurc/tests/drop_destructors.rs`, and semantic/parser unit tests. Known limitations (deferred): reassigning a `Drop` binding does not drop its prior value, and a struct's `Drop`-typed fields are not auto-dropped (no recursive drop glue).

---

## [1.43.0] - 2026-06-18

### Added
- `codegen`: string `.slice(range)` (Phase 1.7). `s.slice(a..b)` (exclusive) and `s.slice(a..=b)` (inclusive) return a borrowed `&string` view into the receiver's UTF-8 data — zero copy, since strings are immutable; the slice is just `(ptr + start, len)`. Indices are byte offsets. Both build modes run a runtime check that panics (abort, no unwinding) on an out-of-bounds range (`start > end`, `end > len`, negative `start`) or a range endpoint that splits a multi-byte UTF-8 code point. A `&string` receiver auto-derefs, and the result is itself a `&string` (so `.slice(...).len()` and `==` work). Range expressions `a..b` / `a..=b` are now a parse node (precedence below `??`), accepted by semantic analysis **only** as a `.slice` argument — used anywhere else they are a `RangeNotAllowed` error; `for`-range loops are unchanged. New `BuiltinMethod::StringSlice`, a `codegen_guard_or_panic` panic-runtime helper, `examples/types/string_slice_method.nr`, and end-to-end coverage in `neurc/tests/string_slice.rs`.

---

## [1.42.1] - 2026-06-18

### Changed
- `docs`: reorganized the private roadmap and synced `CONTRIBUTING.md` so each phase is implementable strictly in order — every open item's prerequisites land in an earlier or the same phase. Resolved `Drop` as a **Phase 1.7 compiler-known lang-item** (recognized specially like `Copy`/`Clone`, reusing impl-blocks + scope/move tracking) rather than gating it on the Phase 2B trait system, making it the keystone next item. Relocated three misplaced items to where their prerequisites live: explicit lifetime annotations `<'a>` → Phase 2B (after Generics, which provides the parse surface); `String.slice(range)` → Phase 1.7 (a string op depending only on `&string` + the panic runtime, both landed); `Layer::require_grad(bool)` → Phase 6 (depends on the Layer trait + `.parameters()`, not the GPU backend). Demoted the duplicate `await`-in-`pool` checkbox in Phase 3 to a forward-dependency note (it is implemented in Phase 7, where `await` exists). No compiler code changed.

---

## [1.42.0] - 2026-06-18

### Added
- `codegen`: string concatenation with `+` (Phase 1.7). `a + b` on two strings allocates a fresh heap buffer via libc `malloc`, copies both operands' bytes in with `memcpy`, and returns a new owned, immutable `string` `{ ptr, len }` (no NUL terminator, consistent with the `len` contract). A `&string` slice may stand in for either operand; the result is always an owned `string`, never a reference. Operands are read, not consumed — like `==`, `+` borrows-to-read and never moves, so both stay usable afterward. Semantic analysis peels a single string reference in the arithmetic arm (`string + string -> string`); any other arithmetic operator on a string, or mixing a string with a non-string, is `InvalidBinaryOperator`. `malloc`/`memcpy` join the existing first-use libc externs (`memcmp`/`write`/`abort`). New `neurc/tests/string_concat.rs` end-to-end coverage and `examples/types/string_concat.nr`.

### Notes
- Concatenated buffers are heap-allocated and **not yet freed** — runtime heap strings leak until `Drop` / deterministic destruction lands (Phase 1.7). The growable-builder half of "Runtime string ops" (`String::new` / `.push_str` / `.clear`) stays open: it needs a mutable growable string type, which contradicts the current immutable-`string` spec and depends on `Drop`.

---

## [1.41.6] - 2026-06-18

### Changed
- `docs`: closed the Phase 1.7 "Remove ARC" roadmap item as an audit. A whole-repo scan (`Arc`/`Rc<`/`Arc::new`/`Rc::new`, the `refcount`/`strong_count`/`retain`/`release_ref` vocabulary, and arc-like crate dependencies) returns zero hits — no reference-counting plumbing was ever introduced. The alpha memory model has always been owned-or-borrowed (move-by-default plus `&T`/`&mut T`), so there is nothing to strip. Marked complete in the roadmap and CONTRIBUTING priorities; the `Drop` half is tracked separately and lands with the trait system (Phase 2B).

---

## [1.41.5] - 2026-06-18

### Changed
- `docs`: documentation sweep for accuracy and concision. Corrected the test count (`677` → `679`) in the README badge/capabilities header and `docs/README.md`. Compacted the README "Current Capabilities" table from ~30 exhaustive rows to a curated mix of core and advanced features ending in a "…and many more" pointer to the changelog/docs. Condensed the CONTRIBUTING Phase 1.7 checklist's landed items to one line each (full behavior stays in the changelog).

### Fixed
- `docs`: corrected the license name in `CODE_OF_CONDUCT.md` ("Neuro Source-Available License" → "Neuro Shared Source License v2.1") and in `docs/README.md` ("GPL v3.0 with Neuro Exceptions" → "Neuro Shared Source License v2.1"). Added a missing **Reporting** section to `CODE_OF_CONDUCT.md` (the enforcement ladder had no reporting channel). Refreshed the stale version/date footer in `docs/README.md` (`v1.31.1`/`2026-06-08` → `v1.41.4`/`2026-06-17`).

---

## [1.41.4] - 2026-06-17

### Fixed
- `ci`: the Scorecard workflow's `github/codeql-action/upload-sarif` step was pinned to SHA `b0c4fd77…`, which does not exist in the `github/codeql-action` repo. Scorecard's webapp verification rejected it as an imposter commit (`http response 400 … imposter commit`), failing the "Scorecard supply-chain security" job on every push to `main`. Repinned to the real v3.30.5 commit `3599b3baa15b485a2e49ef411a7a4bb2452e7f93`.

---

## [1.41.3] - 2026-06-17

### Fixed
- `build`: `compiler/llvm-backend/src/softfloat/builtins.ll` — baked into the build via `include_str!` — was silently excluded from git by the broad `*.ll` ignore rule, so it existed locally but was missing on every CI runner. This broke the build (`couldn't read builtins.ll`), failing Clippy, Architecture Boundaries, the test matrix, and the dependabot PRs. The file is now committed and `.gitignore` carries a negation (`!.../builtins.ll`) so the source IR is tracked while generated `.ll` artifacts stay ignored.

### Changed
- `ci`: dropped dead `develop` branch triggers (only `main` exists) and merged the duplicate `release_smoke` and `build_artifacts` jobs — each rebuilt the release binary on all three OSes — into a single `release` job that builds once, runs smoke tests, then uploads artifacts.

---

## [1.41.2] - 2026-06-17

### Fixed
- `codegen`: `examples/types/half_precision.nr` failed to link on Windows CI — `Failed to execute MSVC cl.exe`. Root cause: LLVM lowers `fpext`/`fptrunc` on `half`/`bfloat` (and f16/bf16 comparisons, which widen to f32 first) to soft-float runtime calls (`__extendhfsf2`, `__truncsfhf2`, `__truncdfhf2`, `__truncsfbf2`, `__truncdfbf2`). Linux/macOS resolve these from libgcc/compiler-rt via the `cc` driver; the Windows linkers (clang → lld-link → MSVC) link no such runtime, so the symbols were undefined and linking fell through to a `cl.exe` that is not on `PATH`. The backend now ships its own definitions: `src/softfloat/` (`builtins.ll`, generated from `reference.c`) is linked into any module that uses `half`/`bfloat`, making the emitted object self-contained on every target. Definitions are `weak_odr` (a platform runtime may still override) and integer-only (they never recursively re-emit these libcalls), and were exhaustively verified against clang's native `_Float16`/`__bf16` — f32↔f16 and f32→bf16 over all 2³² inputs, f16→f32 over all 2¹⁶, and the f64 paths over 200M random inputs, with zero mismatches.

### Security
- `ci`: hardened the GitHub Actions workflows against the OpenSSF Scorecard findings. All third-party actions in `ci.yml` and `scorecard.yml` are now pinned by commit SHA (Pinned-Dependencies); `osv-scanner.yml` drops its workflow-wide `security-events: write` to `permissions: read-all` and grants the write scope per-job (Token-Permissions); and a new `.github/dependabot.yml` keeps the pinned actions and Cargo crates updated weekly (Dependency-Update-Tool).

---

## [1.41.1] - 2026-06-17

### Fixed
- `build`: workspace manifest declared `license = "GPL-3.0"`, which both misidentified the project (it ships under the custom **Neuro Shared Source License v2.1**, per `LICENSE`, README, and `CONTRIBUTING.md`) and used a now-deprecated SPDX form. Switched to `license-file = "LICENSE"` in `[workspace.package]`; all member crates now inherit `license-file.workspace = true`.
- `docs`: corrected the LLVM dependency comment in `Cargo.toml` that claimed LLVM must be built "with MLIR enabled" — the current backend emits LLVM IR via inkwell and needs no MLIR, which only arrives with the Phase 3 tensor dialects.

---

## [1.41.0] - 2026-06-17

### Added
- `semantic` + `codegen`: `&mut self` instance methods — the Phase 1.7 ownership-gated receiver that was previously rejected. A method may now take `&mut self` and assign to `self.field`; the receiver is passed **by pointer** so the write propagates to the caller's value (a `&self` method stays by-value/read-only, and consuming `self` is still rejected pending the by-value struct ABI). Semantic: `register_impl` no longer rejects `&mut self` and records its mangled key in the new `mut_self_methods` set; `check_impl` binds `self` as a *mutable* variable for `&mut self`; the method-call site runs `check_mut_self_receiver`, which enforces the borrow — the receiver must be a `mut` place (or reached through `&mut T`) and must not already be borrowed (`CannotBorrowMutably` / `CannotMutablyBorrowWhileBorrowed`), registering a transient exclusive borrow that clears at statement end. Codegen: `codegen_method` lowers a `&mut self` method's first LLVM parameter as `ptr`, binds `self` directly to it without a copy (recorded type still the struct, so field reads/writes pass through), and seeds `type_env["self"]`; `codegen_method_call` detects a by-pointer callee from its first param type and passes the receiver place's address via `get_struct_ptr_and_type`. Tests: 5 semantic unit (`moves.rs`), 3 `neurc` integration (`methods.rs`), example `structs/mut_self_accumulator.nr`. 677 tests pass.

---

## [1.40.0] - 2026-06-17

### Added
- `semantic`: returned-reference outlives / lifetime elision — the borrow checker now verifies that a function or method whose declared return type is a reference does not return a reference borrowing a place that dies with the call. Under lifetime elision a single input reference lifetime is applied to the output, and the `&self` lifetime is applied to method outputs, so returning one of the reference parameters (or a borrow of `&self`) is sound; borrowing a body-local or a by-value parameter and returning it — directly (`return &local`) or through a local reference binding (`val r = &local; r`) — is rejected with the new `ReturnsReferenceToLocal` diagnostic. New surface: `current_fn_outliving: HashSet<String>` on the type checker (reference-typed parameters plus `self`, rebuilt per function/method and cleared on exit), `SymbolTable::borrow_provenance` (the place a `val r = &x` binding recorded), and `check_returned_reference` (with helpers `tail_expr` / `root_place_name` / `is_local_to_function`) invoked from `Stmt::Return` and both trailing implicit-return sites when the return type is a `Type::Reference`. The walk follows `if`/`else` arms and bare/`unsafe` blocks into their tail expressions. Elision-only — no annotation syntax; ambiguous multi-reference signatures are accepted as long as the borrowee is a parameter (explicit `<'a>` lands with generics, Phase 2B). Conservative: a name absent from the symbol table is treated as non-local, so a valid program is never rejected. Tests: 6 semantic unit, 5 `neurc` integration (`returned_reference.rs`), example `types/returned_reference.nr`. 668 tests pass.

---

## [1.39.0] - 2026-06-16

### Added
- `semantic`: flow-sensitive borrow exclusivity — the aliasing rule split out of the lifetime-inference roadmap item because it needs only borrow-region tracking, not full lifetime inference. The borrow checker now enforces that **at most one `&mut` borrow** of a place may be live at a time and that **no `&` borrow coexists with a live `&mut`**; any number of shared `&` borrows may coexist. Borrow regions are lexical: a borrow held by a binding (`val r = &x`) lives until that binding leaves scope; a borrow passed to a call, used in a condition, or returned ends with the statement that took it. New surface: per-binding persistent/transient borrow counters and a borrow-provenance field on `SymbolInfo`; `SymbolTable` methods `borrow_counts` / `add_transient_borrow` / `attach_borrow` / `release_borrow_of` / `clear_transient_borrows`, with `pop_scope` now releasing a dying reference binding's borrow. The `Expr::Reference` arm checks coexistence and registers each borrow; `check_stmt` clears transient borrows at statement end; `VarDecl` / `Assignment` promote a direct `&place` initializer to a persistent borrow (reassigning a `mut` reference releases its old borrow first). New diagnostics `CannotMutablyBorrowWhileBorrowed` / `CannotBorrowWhileMutablyBorrowed`. Lexical, not NLL: only direct-borrow initializers create tracked persistent borrows, so the analysis never rejects a valid program; read/move-while-borrowed and returned-reference outlives are deferred to lifetime inference. Tests: 8 semantic unit, 5 `neurc` integration (`borrow_exclusivity.rs`), example `types/borrow_exclusivity.nr`. 657 tests pass.

---

## [1.38.0] - 2026-06-16

### Added
- `lexer`/`semantic`/`codegen`: `f16` / `bf16` half-precision scalar primitives — the final Phase 1.5 item. `f16` is the IEEE-754 half float; `bf16` is bfloat16 (`f32`-sized exponent range, fewer mantissa bits). Both are first-class scalar primitives with a deliberately **narrow contract**: binding, move/copy (`Copy`), equality (`==` / `!=`), and `as`-cast to/from any numeric type and to/from each other — but **no scalar arithmetic** (`a + b` on a half operand is a compile error directing you to compute in `f32`: `(a as f32 + b as f32)`) and no ordering. Half-precision literals always carry a suffix (`1.5f16`, `0.02bf16`) — there is no contextual default. Full support as tensor element dtypes is separate and lands with the tensor type system (Phase 3A). New surface: lexer `FloatSuffix::F16`/`BF16` (the two float-suffix regexes now match `(bf16|f16|f32|f64)`, split via `split_float_suffix`), semantic `Type::F16`/`BF16` (`is_half_float()`; new `HalfFloatArithmetic` diagnostic), and backend `Type::F16`/`BF16` lowered to LLVM `half` / `bfloat` (float→float casts and `coerce_if_needed` now pick `fpext`/`fptrunc` by bit width; an `f16`↔`bf16` cast routes through `f32`). Tests: 1 lexer unit, 1 semantic unit, 7 `neurc` integration (`half_precision.rs`), example `types/half_precision.nr`. 644 tests pass. **Phase 1.5 (Syntax & Semantics Stabilization) is now complete.**

---

## [1.37.0] - 2026-06-15

### Added
- `lexer`/`parser`/`semantic`/`codegen`: `char` primitive type. A `char` is a single 32-bit Unicode scalar value. Char literals are written with single quotes and support escapes — `'a'`, `'\n'`, `'\t'`, `'\\'`, `'\''`, `'\0'`, the `\xNN` byte escape, and the `\u{...}` unicode escape (`'\u{1F44D}'`). `char` is `Copy` (binding it does not move the source), has a built-in total order so all six comparison operators work directly (`'a' < 'b'`), and `as`-casts to and from integer types in both directions (`'A' as i32` → 65, `97 as char` → `'a'`); it is **not** castable to/from `float` or `bool` and has **no** arithmetic (`'a' + 1` is a compile error — compute on the integer code point instead). New surface: lexer `TokenKind::Char(char)` (single-scalar regex; `''`/`'ab'`/unterminated `'a` are lex errors via the new `LexError::InvalidCharLiteral`), `Literal::Char(char)`, semantic `Type::Char`, and backend `Type::Char` lowered to LLVM `i32`. Tests: 3 lexer unit, 1 semantic unit, 6 `neurc` integration (`char_type.rs`), example `types/char.nr`. 635 tests pass.

---

## [1.36.0] - 2026-06-15

### Added
- `parser`/`semantic`/`codegen`: `loop` as a value expression — `break value`. A `loop` used in expression position evaluates to the value carried out by its `break`: `val first = loop { ... break v }`. All value-carrying `break`s for one loop must agree on type; `break value` targeting a `while`/`for` is rejected, since those always yield unit (only `loop` is guaranteed to leave solely via a `break`). `break label value` carries a value out of a labeled outer loop, and a labeled loop may itself appear in expression position (`val x = outer: loop { ... }`). New surface: `Expr::Loop { label, body, span }` (distinct from the statement `Stmt::Loop`, whose value is discarded) and a `value: Option<Expr>` field on `Stmt::Break`. The parser disambiguates `break ident` — an identifier is read as a label only when it names an in-scope loop (tracked in a new parser label stack), otherwise it begins the value expression. Semantic analysis replaces the `loop_labels` stack with a `loop_stack` of per-loop contexts that accumulate the agreed value-break type (new `BreakValueInUnitLoop` error; type disagreement reuses `Mismatch`). Codegen allocates a result slot per value loop; a value `break` stores into it before branching and the loop expression loads it at exit. Tests: 2 parser unit, 3 semantic unit, 4 `neurc` integration (`loop_value.rs`), example `control_flow/loop_value.nr`. 626 tests pass.

---

## [1.35.0] - 2026-06-15

### Added
- `parser`/`semantic`/`codegen`: loop labels with `break label` / `continue label`. A `for`, `while`, or `loop` may be prefixed with a label — an identifier followed by a colon (`outer:`) — and a nested `break outer` / `continue outer` then targets the labeled loop rather than the innermost one. This is the construct that lets an inner loop exit or re-enter an enclosing loop directly. New surface: an `Option<Identifier>` `label` field on `Stmt::While` / `ForRange` / `Loop` and on `Stmt::Break` / `Continue` (labels reuse the existing `Identifier` + `Colon` tokens, so the lexer is unchanged). The parser dispatches `ident : <loop-keyword>` to the matching loop parser and reads an optional same-line label after `break` / `continue`. Semantic analysis replaces the `loop_depth` counter with a `loop_labels` stack: an unlabeled `break`/`continue` still requires an enclosing loop, and a labeled one requires a matching active label (else the new `UndefinedLabel` error). Codegen adds a `label` to `LoopTargets` and resolves a labeled jump by scanning the loop-target stack from innermost outward. Tests: 3 parser unit, 3 semantic unit, 4 `neurc` integration (`labeled_breaks.rs`), example `control_flow/labeled_breaks.nr`. 617 tests pass.

---

## [1.34.0] - 2026-06-09

### Added
- `lexer`/`parser`/`semantic`/`codegen`: `loop { ... }` infinite-loop statement. `loop` is the canonical infinite loop — the form the `prefer-loop-over-while-true` lint already recommended in place of `while true { ... }`, but which previously did not exist, so following the compiler's own advice produced broken code. `loop` has no condition: the only exit is `break`, and `continue` re-enters the body from the top. New surface: the `loop` keyword (`TokenKind::Loop`) and the `Stmt::Loop { body, span }` AST node. Semantic analysis increments `loop_depth` for the body (so `break`/`continue` inside are in-loop) and the lint walker recurses into `loop` bodies; codegen lowers it to an unconditional back-edge (`loop.body` branches to itself), mirroring `while` minus the condition block. A `loop` statement evaluates to unit; the value-producing `break value` form is not yet modelled (tracked on the roadmap). Tests: 1 lexer unit, 1 parser unit, 2 semantic integration, 2 `neurc` integration (`control_flow.rs`), example `control_flow/loop_statement.nr`. 607 tests pass.

---

## [1.33.0] - 2026-06-09

### Added
- `parser`/`semantic`/`codegen`: mutable borrows `&mut T` and the dereference operator `*` (Phase 1.7). A `&mut T` reference grants write access to a `mut` binding without taking ownership; values are read and written through the new prefix `*` operator. New surface: the `&mut place` borrow expression, the `&mut T` type annotation (params/returns/locals), the `*expr` dereference expression, and the `*place = value` assignment-through-a-reference statement. Borrow rules enforced in semantic analysis: `&mut` requires a `mut` binding (`CannotBorrowMutably`); `*` applies only to a reference (`CannotDereference`); writing through `*` requires a `&mut` (`CannotAssignThroughRef`). `&mut T` and `&T` are distinct types with no implicit coercion (explicit over implicit). Codegen lowers a borrow to the place's storage pointer (mutability is a compile-time-only distinction) and a deref to a load/store through that pointer. Side fix needed by the canonical example: unit-returning function calls in statement position no longer error (`codegen_call`/`codegen_method_call` now return `Option`, discarded in statement position); and a new line beginning with `*` is parsed as a dereference statement rather than glued to the previous expression as a continued multiplication. **Deferred** (mirrors how `&T` shipped without lifetime checking): flow-sensitive aliasing exclusivity — the "at most one `&mut` at a time, no `&` may coexist" rule — lands with lifetime inference, which shares the same borrow-region analysis. Tests: 4 semantic unit, 1 type-system unit, 8 integration (`neurc/tests/mutable_borrows.rs`), example `showcase/mutable_borrows.nr`. 601 tests pass.

---

## [1.32.0] - 2026-06-09

### Added
- `semantic`/`codegen`: `&string` slice equality (Phase 1.7). `&string` is now usable as a borrowed string slice: the equality operators `==` / `!=` compare the underlying UTF-8 bytes for any combination of an owned `string` and a `&string` slice (`&a == &b`, `&a == "lit"`, `"lit" == &a`). Semantic analysis normalizes a single string reference via `Type::peel_string_ref` in the `Equal`/`NotEqual` arm, so a slice and an owned string are equality-compatible; peeling is limited to `string`, so `i32 == &string` and `&i32 == i32` stay type errors (reading other `&T` through `==` needs the deref operator, which lands with `&mut T`). Codegen handles string equality before the numeric coercion: each operand is normalized to its `{ ptr, len }` fat pointer by the new `load_string_fatptr` helper (a borrow is loaded through its pointer; an owned struct value passes through) and compared with the existing `codegen_string_eq` — the ABI is unchanged, only the borrowed operand is auto-dereferenced. Borrowing for a comparison never moves, so both operands stay usable afterward. Tests: 1 semantic-type unit, 7 integration (`neurc/tests/string_slice.rs`), example `types/string_slice.nr`. 588 tests pass.

---

## [1.31.1] - 2026-06-08

### Changed
- `codegen`: audit cleanup — replace the banned `unimplemented!()` stub on the tensor arm of `Type::from_ast` (`llvm-backend`) with a documented `unreachable!()` invariant. Tensor annotations are rejected by semantic analysis before codegen, so the arm asserts an invariant rather than stubbing a missing feature (AC-004 compliance). No behavioral change; 580 tests pass.

---

## [1.31.0] - 2026-06-08

### Added
- `semantic`/`codegen`: immutable borrows `&T` (Phase 1.7). A new reference type `&T` is accepted in any type-annotation position (parameters, returns, locals), and a prefix `&place` borrow expression takes a non-owning reference to a variable. Borrowing **does not move** the borrowed value — `length(&msg); msg.len()` compiles — and `&T` is itself `Copy`, so a reference can be passed and re-borrowed freely. Method and field access auto-deref through a borrow: `s.len()` / `s.clone()` work on `&string`, and `r.field` / `r.method()` work on `&Struct`. Borrowing a temporary (a literal, a call result) or a `const` (an inlined value, not a place) is a new `CannotBorrowValue` error. References lower to opaque LLVM 20 pointers; a borrow of a place is its alloca pointer, and consuming sites load through the pointer when the receiver is a borrow (value-driven, so owned and borrowed receivers share one path). Integer intrinsics (`wrapping_*`, `.shr`) intentionally still require a value receiver — reading a scalar through `&T` needs the deref operator, which lands with `&mut T`. Implementation spans `ast-types` (`Type::Reference`, `Expr::Reference`), `syntax-parsing` (`&T` type + prefix `&` borrow), `semantic-analysis` (`Type::Reference`, no-move/Copy rules, auto-deref, `CannotBorrowValue`), and `llvm-backend` (`Type::Reference` → `ptr`, borrow + auto-deref codegen). Tests: 1 semantic-type unit, 3 move-checker unit, 6 integration (`neurc/tests/immutable_borrows.rs`), example `types/immutable_borrows.nr`. 580 tests pass.

---

## [1.30.0] - 2026-06-07

### Added
- `semantic`: `Copy` trait + `@derive(Copy, Clone)` for structs (Phase 1.7). A struct that derives `Copy` is duplicated on assignment instead of moved, so `val b = a` leaves `a` usable; a struct that does *not* derive `Copy` is now move-tracked like `string` — binding/assigning/returning/passing it by value moves the source, and reading it afterward is a `UseOfMovedValue` error. Deriving `Copy` requires every field to be `Copy` (primitive scalars are `Copy`, `string` is not, a struct field is `Copy` only when it derives `Copy`); a violation is a new `CopyDeriveNonCopyField` error. `Copy` implies `Clone`. `@derive(Clone)` (or `Copy`) enables `struct.clone()` as a compiler-known builtin deep copy; a user-defined `clone` method shadows it. Unknown derive arguments (e.g. `Debug`) are accepted and ignored for forward compatibility. Move-tracking is now context-aware (`TypeChecker::is_type_move_tracked` / `is_type_copy`) rather than a property of the type alone. `@derive(...)` attributes now attach to struct definitions (parser + `StructDef.attributes`). Implementation spans `ast-types` (`StructDef.attributes`), `syntax-parsing` (struct attribute parsing), `semantic-analysis` (`copy_structs`/`clone_structs` registries, Copy-field validation pass, struct `.clone()` resolution), and `llvm-backend` (`BuiltinMethod::StructClone` — loads the aggregate value, a faithful copy while structs are stack-allocated). Tests: 3 parser unit, 5 semantic unit, 5 integration (`neurc/tests/copy_clone.rs`), example `types/copy_clone.nr`. 570 tests pass.

---

## [1.29.0] - 2026-06-07

### Added
- `semantic`: move semantics by default (Phase 1.7). Non-`Copy` owned values are *moved* out of their source binding when bound (`val s2 = s1`), assigned, returned, passed by value to a call, or stored into a struct field; reading the source afterward is a new `UseOfMovedValue` error pointing at where the move happened. `.clone()` (already a builtin) borrows its receiver and is the canonical opt-out. Move tracking is limited to `string` — the only non-`Copy` type the language can construct today; structs stay freely duplicable until `Copy`/`@derive(Copy)` lands (the next Phase 1.7 item). The checker is conservative: it flags only direct place expressions in a consuming position, and `if`/`while`/`for` bodies and `if`-expression arms snapshot/restore move state so a conditional move never leaks onto a path that did not execute it (no false positives; may miss e.g. second-iteration loop moves). Implementation: new `type_checkers/moves.rs` (`record_move`), `SymbolInfo.moved_at` plus `mark_moved`/`clear_moved`/`snapshot_moves`/`restore_moves` on `SymbolTable`, `Type::is_move_tracked()`. Tests: 6 unit (`semantic-analysis`), 5 integration (`neurc/tests/move_semantics.rs`), example `types/move_semantics.nr`. 557 tests pass.

---

## [1.28.1] - 2026-06-05

### Fixed
- `docs`: corrected a pervasive, user-facing error — the getting-started tutorial, troubleshooting guide, and language reference all claimed semicolons were *required* to terminate statements (`val x: i32 = 10  // Semicolon required`). Neuro has **no semicolons**: statements are newline-terminated and a trailing `;` is an `unexpected token Semicolon` parse error. A beginner following `first-program.md` verbatim would hit an immediate parse failure. Rewrote the statement/implicit-return explanation in `docs/getting-started/first-program.md`, `docs/guides/troubleshooting.md`, `docs/language-reference/expressions.md`, and `docs/language-reference/functions.md` to describe newline termination and positional implicit return.
- `docs`: fixed stale example paths left over from the v1.23.1 `examples/` reorg into topic subdirectories. README, CONTRIBUTING, and the getting-started/CLI/compilation guides told users to run `cargo run -p neurc -- compile examples/hello.nr`, which no longer exists (now `examples/basics/hello.nr`); the README "compiles and runs today" link pointed at `examples/neuron.nr` instead of `examples/structs/neuron.nr`. Historical CHANGELOG entries and illustrative `bad.nr`/`mismatch.nr` error-output paths left untouched.
- `docs`: refreshed `docs/README.md` version/footer metadata — version 1.27.0 → 1.28.0, last-updated date, and inkwell 0.8.0 → 0.9.0 (two spots) to match the v1.26.2 bump.

### Added
- `tests`: 3 parser regression tests (`syntax-parsing/tests/error_tests.rs`) asserting that a trailing semicolon after a binding, an expression, or a `return` is a parse error — locking in the no-semicolon language decision so it cannot silently drift from the docs. 546 tests pass.

---

## [1.28.0] - 2026-06-05

### Added
- `parser`: struct field-init shorthand and functional-update syntax (Phase 2A). `Point { x, y }` desugars each bare field to `x: x` at parse time (a `FieldInit` whose value is `Expr::Identifier(field_name)` — no AST node, so semantic analysis and codegen are unchanged for shorthand; an undefined name surfaces as the ordinary undefined-variable error). `Point { x: 1.0, ..p }` adds a functional-update base: `Expr::StructLiteral` gained `base: Option<Box<Expr>>`, and the parser stops the field scan at a trailing `..expr`. Semantic analysis checks the base against `Type::Struct(name)` (wrong struct → `Mismatch`) and, when a base is present, skips the missing-field scan since `..base` supplies every unlisted field; a base-less literal still requires all fields (`MissingStructField`). Codegen seeds the LLVM aggregate from the base struct value instead of `undef`, then `insert_value` overwrites each explicit field, so unlisted fields keep the base's values with no reallocation. The type-alias rewrite pass recurses into `base`. Tests: 5 parser unit (`syntax-parsing/tests/expression_tests.rs`), 8 integration (`neurc/tests/struct_shorthand_update.rs`), example `structs/struct_update.nr`. 543 tests pass.

---

## [1.27.0] - 2026-06-05

### Added
- `codegen`: `string.clone()` builtin method (Phase 1.7). Returns a fresh `string` equal to its receiver — the canonical opt-out of move-by-default for non-`Copy` types. Resolved on a `string` receiver in both `semantic-analysis` (`resolve_builtin_method` → `Type::String`) and `llvm-backend` (`BuiltinMethod::StringClone`), duplicated per VSA so neither slice depends on the other. Lowering copies the `{ ptr, len }` fat-pointer value: today strings are immutable and `.rodata`-backed (no heap string type yet), so a value copy is observationally a deep copy; when runtime heap strings land this must duplicate the underlying buffer. Takes no arguments (`ArgumentCountMismatch` otherwise); `.clone()` on a non-`string` receiver remains `MethodNotFound` (`Copy` scalars take the assignment path). Also fixed a latent `span.start` collision: chaining two builtin calls (`s.clone().len()`) nests two `Call` nodes sharing `span.start`, so the `builtin_methods` dispatch map is now keyed by the full span `(start, end)` — unique per node — matching the existing `binary_left_types` workaround. Tests: 2 semantic unit, 1 backend unit, 3 integration (`neurc/tests/builtin_methods.rs`), example `types/string_clone.nr` (exit 5). 530 tests pass.

---

## [1.26.2] - 2026-06-04

### Changed
- `build`: dependency maintenance pass. Removed three dead dependencies from `syntax-parsing` — `lalrpop`, `lalrpop-util`, and `chumsky` — which were declared but never imported (the parser is hand-written Pratt; there was no `.lalrpop` grammar or `build.rs`). This drops their transitive build-time tree (`petgraph`, `regex`, `string_cache`, `bit-set`, `term`, …), trimming the dependency surface and build time. Upgraded `inkwell` 0.8.0 → 0.9.0 (still LLVM 20 via `llvm20-1`; 0.9 adds LLVM 21/22 support and ports to Rust edition 2024 — no backend code changes required), `thiserror` 1 → 2, `toml` 0.8 → 1.1, and `criterion` 0.5 → 0.8 (dev/bench only). `cargo update` swept caret-range drift (unicode-segmentation 1.13, unicode-ident 1.0.24, tempfile 3.27, etc.). `logos` deliberately held at 0.14 — the 0.16 engine rewrite trades a slight lexer perf regression for regex correctness we do not currently need. All 526 tests pass; clippy clean.

---

## [1.26.1] - 2026-06-04

### Fixed
- `codegen`: short-circuit `&&` / `||` with a comparison as the **left** operand miscompiled. The type pass stored each binary node's left-operand type at `expr_types[span.start + 1]`, but a binary node and its leftmost descendant share the same `span.start`; the parent's write (e.g. `&&`, left type `Bool`) clobbered the left child comparison's entry (e.g. `i32`). The leftmost comparison was then generated with `left_ty = Bool`, truncating its i32 operands to i1 — so `c >= 48 && c <= 57` with `c = 51` wrongly evaluated to `false` (the i1 `-1 >= 0` is false), while `c == 51 && …` and parenthesized `(c >= 48) && …` happened to work. Left-operand types now live in a dedicated `binary_left_types` map keyed by the full span `(start, end)`, which is unique per node and immune to the `span.start` collision. Regression test: `compiler/neurc/tests/short_circuit_runtime.rs`.

---

## [1.26.0] - 2026-06-04

### Added
- `codegen`: panic runtime — `panic` / `assert` / `unreachable` (Phase 1.7, syntax). The three panic-family builtins now lower end-to-end with the **abort, no unwinding** contract: `panic(msg: string)` and `unreachable()` print a diagnostic to stderr and terminate via libc `abort()` (SIGABRT); `assert(cond: bool)` branches and aborts only when the condition is false. Diagnostics carry the source location (`message at file:line:col`), threaded into `llvm_backend::compile` via two new `source` / `source_path` parameters and rendered through `source_location::SourceFile`. The diagnostic is written with the POSIX `write` syscall to stderr (fd 2) so it reaches the terminal before the process dies; no unwinding landing pads are emitted, so `Drop`/`defer` (future) will fire only on normal scope exit. The builtins are recognized in both `semantic-analysis` (`resolve_panic_builtin`, returning the divergent `Unknown` type so a panic satisfies any return/binding context, e.g. `func f() -> i32 { panic("x") }`) and `llvm-backend` (`is_panic_builtin` + `panic.rs`); a user function of the same name shadows the builtin. Statements after a divergent call are dropped via new terminated-block guards in `codegen_stmt`/`codegen_return`/`codegen_body`. `checked_*`-style value-position panics and rerouting integer-overflow/bounds checks through this runtime remain follow-ups.

---

## [1.25.0] - 2026-06-04

### Added
- `parser`: `unsafe { }` block infrastructure (Phase 1.7 groundwork, syntax). `unsafe` is now a reserved keyword (`TokenKind::Unsafe`) and `unsafe { … }` parses to a dedicated `Expr::Unsafe { stmts, span }` AST node. The block is an ordinary statement block: it introduces a scope and evaluates to its trailing expression, so it works as an implicit return, a `val` initializer, or a void statement. `unsafe` is deliberately **inert** — it carries no special semantics yet and lowers to identical IR as a bare block (`codegen_block_expr`). The distinct node exists so the Phase 5 GPU-kernel aliasing model can later gate raw `KernelOut` index writes behind `unsafe { }` without re-shaping the grammar. Reserving the keyword means it can no longer be used as an identifier.

---

## [1.24.1] - 2026-06-03

### Changed
- `docs`: synced phase status across all Markdown docs to reflect Phase 1.5 (syntax & semantics stabilization) as complete and Phase 1.7 (ownership & borrow checker) as the active phase. Removed the now-complete Phase 1.5 checklist from `CONTRIBUTING.md`; updated status/roadmap lines in `README.md`, `docs/README.md`, `docs/getting-started/quick-start.md`; corrected "not yet implemented" lists (if/block expressions are implemented; ownership → 1.7, string interpolation → Phase 2); retargeted stale `Phase 1.5` references in `docs/language-reference/{operators,control-flow}.md` and `compiler/semantic-analysis/CONTEXT.md`.

---

## [1.24.0] - 2026-06-03

### Added
- `parser`: type aliases. `type Name = TargetType` introduces a transparent alias — the alias and its target are interchangeable and no new nominal type is created. Aliases resolve in every type-annotation position (variable/const annotations, function parameters and return types, struct fields, and `as` cast targets) and collapse through chains (`type A = B; type B = i32`). Resolution happens entirely at parse time by substituting each aliased annotation with its target type (the same desugaring strategy used for compound assignment), so semantic analysis and codegen are unchanged and never observe an alias. New `TokenKind::Type` keyword and `ParseError::{DuplicateTypeAlias, TypeAliasShadowsBuiltin, CyclicTypeAlias}` diagnostics reject duplicate aliases, aliases that shadow a built-in type, and cyclic alias chains; an unknown target type is still reported by the existing semantic `UnknownTypeName` check against the resolved type, with the span anchored at the alias use site. Scope note: alias substitution applies to type positions only — using an alias as a value-position constructor or path name is not part of this feature.

---

## [1.23.4] - 2026-06-03

### Fixed
- `build`: release smoke-test harness (`tools/run_release_smoke_tests.py`) referenced `examples/milestone.nr` and `examples/factorial.nr`, which moved to `examples/basics/` when the examples were reorganized. The hard-coded list now points at `basics/milestone.nr` and `basics/factorial.nr`, so the Windows/Linux/macOS release CI jobs find and compile them again (exit codes 8 and 120 unchanged).
- `docs`: updated stale `examples/milestone.nr` / `examples/factorial.nr` paths across `README.md` and `docs/` to the current `examples/basics/` locations.
- `ci`: the Test Suite matrix installed Rust via `dtolnay/rust-toolchain@stable` while overriding `toolchain: ${{ matrix.rust_version }}`. The `@stable` tag hardcodes the channel in the action ref (input `toolchain` is `required: false, default: stable`), so the dynamic override is unsupported and the `nightly` leg bailed out in the action's parse step with `'toolchain' is a required input`. The matrix step now uses `@master` (where `toolchain` is `required: true`), the documented pattern for channels that vary by matrix value. The six fixed-stable `@stable` uses elsewhere are unchanged.

---

## [1.23.3] - 2026-06-02

### Fixed
- `codegen`: logical `&&` and `||` now short-circuit, as the language spec and `docs/language-reference/operators.md` have always promised. The backend previously evaluated **both** operands eagerly and combined them with a plain `and`/`or` on the two `i1` values, so the right-hand side always ran — meaning a guard like `if x != 0 && 10 / x > 0` still executed the division when `x == 0` (SIGFPE), and any RHS side effect fired unconditionally. `codegen_binary` now intercepts `&&`/`||` before operand evaluation and lowers them through `codegen_short_circuit`, which branches on the LHS and only evaluates the RHS on the deciding edge, merging the two results with a phi node (`&&` → `if lhs { rhs } else { false }`, `||` → `if lhs { true } else { rhs }`). The phi captures the true predecessor blocks after each side is emitted, so a RHS that itself appends blocks (e.g. a nested `if`-expression) is handled correctly.
- `codegen`: a `bool`-typed constant whose initializer is a binary expression — `const FLAG: bool = true && false`, `const OK: bool = (1 < 2) && (3 < 4)`, `const E: bool = true == true`, including function-scope `const` — no longer aborts compilation with an `internal compiler error: type mismatch in const binary expression`. The const folder (`fold_const`) only had arms for two-integer and two-float operands; bool operands fell through to the catch-all error even though semantic analysis accepted the program. Added a `(Bool, Bool)` arm handling `&&`, `||`, `==`, and `!=`.

---

## [1.23.2] - 2026-06-02

### Fixed
- `codegen`: a tail-position `if`/`else` used as a function's or method's implicit return value is now lowered correctly. A statement-position `if` parses to `Stmt::If`, so the backend's implicit-return detection — which only recognised `Stmt::Expr` — fell through to a void `if` statement and emitted `unreachable` for the non-void return, producing no instruction at `-O0` and letting execution run off the end of the function (segfault or garbage). `codegen_body` now treats a trailing `Stmt::If { else_block: Some(..), .. }` as a value-producing if-expression, and the type pass records its result type at the `if` span so the result slot is allocated. Restores the idiomatic `func f() -> T { if c { a } else { b } }` form, including recursion (`gcd`) and `&self` methods. `examples/structs/neuron.nr` reverts to the idiomatic tail if-expr.

---

## [1.23.1] - 2026-06-02

### Changed
- `tests`/`docs`: reorganized the `examples/` directory into topic subdirectories (`basics/`, `types/`, `operators/`, `control_flow/`, `structs/`, `showcase/`) and made the example test harness self-expanding. `compiler/neurc/tests/examples.rs` now discovers every `.nr` file recursively and checks its exit code against a single manifest, `examples/expected.txt`; adding an example is one new file plus one manifest line, with no Rust edits. The harness fails loudly on an unregistered file, a stale manifest entry, or any exit-code mismatch, so `cargo test --workspace` exercises all examples automatically.

### Added
- `tests`: four "showcase" examples that exercise multiple features together — `showcase/perceptron.nr` (structs + methods + `f64` + branches + loop), `showcase/num_algorithms.nr` (recursion + loops + modulo + saturating arithmetic), `showcase/running_stats.nr` (struct state + field mutation + `&self` method + `f64` division), and `showcase/simulation.nr` (bitwise flags + struct state + `.shr` + `break`).

### Fixed
- `examples`: rewrote example functions that relied on a tail-position `if`/`else` *expression* as the implicit return value to use explicit `return` instead, since that codegen path is currently miscompiled (segfault / wrong value when the result is consumed). `structs/neuron.nr` previously masked the issue by discarding the result; it now observes its activation in the exit code. Removed stray compiled binaries from `examples/` (including a tracked one) and tightened the `.gitignore` rules to cover the new subdirectories.

---

## [1.23.0] - 2026-06-02

### Changed
- `codegen`/`docs`: formalized the string literal vs runtime string distinction (Phase 1.5). String literals are emitted to `.rodata` (never heap-allocated); the trailing NUL is now the named `STRING_NULL_TERMINATOR` constant in `literals.rs`, documented as a C-string/FFI convenience that the fat-pointer `len` deliberately excludes. `len` is the authoritative UTF-8 byte count — interior NUL bytes are legal content and are counted, so consumers must not treat string data as NUL-terminated. Behaviour is unchanged (codegen already computed `len` this way); this item formalizes, documents, and tests the guarantee. New end-to-end tests cover multibyte UTF-8 (`"héllo".len() == 6`) and an interior NUL (`"a\0b".len() == 3`). 506 tests passing.

---

## [1.22.1] - 2026-05-31

### Changed
- `build`/`tests`: refactored the three largest source files into focused modules with no behaviour change (504 tests still pass). `llvm-backend/src/codegen/expressions.rs` (1609 lines) split into an `expressions/` submodule — `mod.rs` (dispatch + shared helpers), `literals.rs`, `binary.rs`, `unary.rs`, `methods.rs`, `control_flow.rs` — each adding to the same `impl CodegenContext` block. `semantic-analysis/tests/integration_tests.rs` (980 lines) split into seven feature suites (`semantics_{functions,control_flow,integers,errors,expression_returns,strings,lints}.rs`). The lexer's `lib.rs` test module (≈540 lines) moved to `lexical-analysis/src/tests.rs`, leaving the slice entry point at 137 lines.

---

## [1.22.0] - 2026-05-31

### Added
- `semantic`/`codegen`: integer primitive methods `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and the right-shift method `.shr(n)` (Phase 1.5). Each resolves on any integer receiver, takes one same-typed argument, and returns the receiver's type. Wrapping ops emit plain non-trapping two's-complement arithmetic (they never panic, even in debug builds). `.shr(n)` lowers to `ashr` for signed receivers and `lshr` for unsigned. Saturating add/sub use the `llvm.{s,u}{add,sub}.sat` intrinsics; saturating mul uses `{s,u}mul.with.overflow` and selects the saturation bound (unsigned → MAX; signed → MIN/MAX by product sign). Non-integer receivers report `MethodNotFound`; wrong arity reports `ArgumentCountMismatch`; a mismatched argument type reports a type `Mismatch`. `checked_*` (returns `Option<T>`) stays deferred to Phase 2C.

---

## [1.21.0] - 2026-05-31

### Added
- `semantic`/`codegen`: builtin method dispatch on primitive & string types (Phase 1.5). Method-call syntax `receiver.method(args)` now resolves a fixed, compiler-known set of intrinsic methods when the receiver is a non-struct (primitive or string) type, in addition to user-defined `impl` methods. The first intrinsic is `string.len() -> u64`, which lowers to a single `extractvalue` of the fat pointer's stored byte length (O(1), no scan, excludes the null terminator). Unknown builtin methods still report `MethodNotFound`; wrong arity reports `ArgumentCountMismatch`. This unblocks the integer `wrapping_*` / `saturating_*` / `.shr(n)` methods tracked as a separate roadmap item.

---

## [1.20.1] - 2026-05-30

### Fixed
- `build`: disable inkwell's `target-all` default feature so only the x86 target is compiled in. The previous config additively enabled all 17 LLVM target initializers, which failed to link on Windows CI (whose prebuilt LLVM only ships the x86 target libs) with ~79 unresolved `LLVMInitialize*` symbols.
- `tests`: make the integer-overflow end-to-end tests cross-platform. `llvm.trap` is delivered as a signal on Unix (no exit code) but as a negative NTSTATUS exit code on Windows; wrapped-result exit codes are 8-bit on Unix but full-width on Windows. Trap detection and wrap-value checks now handle both.

---

## [1.20.0] - 2026-05-30

### Added
- `codegen`: integer overflow semantics. Runtime `+`, `-`, and `*` on integer types now trap at runtime in debug builds (`-O0`) via the LLVM `{s,u}{add,sub,mul}.with.overflow` intrinsics + `llvm.trap`, and wrap (two's complement) in release builds (`-O1..-O3`). Division, modulo, bitwise ops, and floats are unaffected; compile-time constant folding continues to wrap.
- `tests`: 2 backend unit tests (valid IR at `-O0` and `-O2`) and 4 end-to-end tests (signed/unsigned overflow traps in debug, wraps in release).
- `docs`: `examples/integer_overflow.nr` plus an "Integer Overflow" section in the type-system reference.

---

## [1.19.3] - 2026-05-29

### Changed
- `docs`: reorganized the roadmap by dependency order. Three Phase 1.5 tail items had forward dependencies on later phases and were relocated: `*Assign` traits → Phase 2B (need the trait system), `&string` slice type → Phase 1.7 (needs the `&T` reference type), and `checked_*` integer methods → Phase 2C (need `Option`). Added an explicit "builtin method dispatch on primitive & string types" prerequisite that gates the integer methods and the `.shr(n)` shift method.
- `docs`: corrected the bitwise-operators note — `.shr(n)` is specified as a method but is **not yet implemented** (it needs builtin-method dispatch); the roadmap and CONTRIBUTING.md previously implied it had shipped.
- `docs`: CONTRIBUTING.md now lists only the active phase (Phase 1.5) and links to the README Quick Roadmap for the full multi-phase plan, instead of duplicating phases 1.7/1.8/2.

---

## [1.19.2] - 2026-05-29

### Added
- `tests`: dedicated coverage for underscore digit separators in numeric literals — 5 lexer unit tests (decimal, hex/binary/octal, float fractional + exponent, suffixed int/float, leading-underscore boundary) and 4 end-to-end compile-and-run integration tests.
- `docs`: `examples/underscore_separators.nr` plus a "Digit Separators" note in the type-system reference.

### Notes
- Lexer support for `_` separators already shipped incidentally with the literal-suffix regexes (every numeric pattern carries `_` in its character class and each parser strips it). This release formally validates, documents, and closes out the Phase 1.5 roadmap item — no production code changed.

---

## [1.19.1] - 2026-05-27

### Fixed
- `build`: Windows CI link failure — 79 unresolved LLVM symbols (`LLVMInitializeARMTarget`, `LLVMInitializeAArch64Target`, etc.). Root cause: `inkwell` was built with `target-all` which compiles init stubs for every LLVM backend, but `vovkos/llvm-package-windows` ships only `X86;NVPTX;AMDGPU` targets. Fix: `target-all` → `target-x86`; Neuro calls only `initialize_native()` so this is sufficient on all CI platforms.

### Changed
- `ci`: `security_audit` job now uses `taiki-e/install-action` (prebuilt `cargo-audit` binary, ~2 min faster) instead of `cargo install cargo-audit` from source.
- `ci`: `coverage` job uses `taiki-e/install-action` for `cargo-tarpaulin` (prebuilt binary) and fixes deprecated `--all` flag → `--workspace`. Updates `codecov/codecov-action` v4 → v5.
- `ci`: `test` job toolchain action pinned to `@stable` (was `@master`).
- `ci`: Removed dead `allow_failure: true` matrix label and empty `exclude: []` array from `test` matrix.
- `ci`: `lint` job — removed redundant `cargo check --workspace` (already covered by clippy).
- `ci`: `build_artifacts` matrix now has `fail-fast: false` so one OS failure doesn't cancel the others.
- `ci`: Windows LLVM install now cached via `actions/cache@v4` keyed on the pinned version string, saving ~5 min per Windows job on cache hit. Install/configure steps separated for clarity.

---

## [1.19.0] - 2026-05-27

### Added
- `semantic`: comparison chain rejection — `a < b < c` is now a compile error with an actionable "use `&&` to combine separate comparisons" suggestion. Covers all six comparison operators (`<`, `>`, `<=`, `>=`, `==`, `!=`). Detection fires in semantic analysis when a comparison operator's LHS is itself a comparison expression.
- `infra`: `BinaryOp::is_comparison()` helper on the AST `BinaryOp` enum.
- `tests`: 5 unit tests in semantic-analysis and 6 integration tests in neurc validating rejection of chained comparisons and acceptance of valid patterns (`a < b`, `a < b && b < c`).

---

## [1.18.2] - 2026-05-25

### Changed
- `docs`: `docs/README.md` "Current Features" section rewritten — now covers all Phase 1.5 and Phase 2 features (const, compound assignment, `as` casts, inclusive range `..=`, bitwise ops, integer/float literal suffixes, if/block expressions, attribute system, `??` operator, string equality, structs, methods); stale "Phase 1 Complete" heading removed; example programs section expanded with a Neuron (struct + method + if-expression) snippet; Last Updated date corrected to 2026-05-25.
- `docs`: `docs/language-reference/operators.md` Common Patterns section updated — removed three stale "if-as-expression not yet implemented" notes from the Clamping, Sign Determination, and Absolute Value examples; each now shows the idiomatic if-expression form (landed in v1.13.0).
- `docs`: `examples/README.md` updated — added entries for `structs.nr`, `methods.nr`, `neuron.nr`, and `compound_assignment.nr`; fixed Windows `.exe` paths to Unix paths; updated Known Limitations (borrow checker phase 1.7, `&mut self` deferred); Exit Codes table extended with all missing examples.

---

## [1.18.1] - 2026-05-25

### Changed
- `docs`: `CONTRIBUTING.md` now carries the detailed Phase 1.5 — Syntax & Semantics Stabilization checklist (Parser & Syntax Fixes, Language Semantics, String Memory Model) so contributors can see at a glance which items have landed and which are open. Replaces the brief three-bullet Phase 1.5 summary.
- `docs`: `docs/README.md` roadmap table removed; replaced with a pointer to `README.md#quick-roadmap` (public quick view) and `CONTRIBUTING.md` (detailed checklists). Roadmap content now lives in exactly three places: `README.md`, `CONTRIBUTING.md`, `.idea/roadmap.md`.

---

## [1.18.0] - 2026-05-25

### Added
- `lexer`: float literal type suffixes `f32` / `f64` — `1.5f32`, `2.0f64`, `1e10f32`, `1.5e-5f64` now tokenize to a dedicated `TokenKind::FloatSuffix(FloatSuffixToken { value, suffix })` token, mirroring the existing integer-suffix encoding. Two new `priority = 3` regexes (fractional and exponent-only forms) sit above the bare-float patterns so logos longest-match always picks the suffixed token.
- `parser`: `parse_prefix` handles `TokenKind::FloatSuffix(tok)` → `Literal::Float(tok.value, Some(tok.suffix))`; plain `TokenKind::Float(f)` now produces `Literal::Float(f, None)`.
- `semantic`: `infer_suffixed_float_type` short-circuits contextual inference when a suffix is present and pins the literal to `Type::F32` / `Type::F64` via `float_suffix_to_type`. Annotation mismatches (e.g. `val x: f32 = 1.5f64`) surface through the existing assignment type-check path.
- `codegen`: `codegen_literal` and the type pass route `Literal::Float(_, Some(F32))` to `f32_type().const_float(_)` and the `None`/`Some(F64)` paths to `f64_type().const_float(_)`.
- `infra`: new `FloatSuffix` enum in `shared-types`; `Literal::Float(f64)` → `Literal::Float(f64, Option<FloatSuffix>)` to carry the explicit type suffix from the lexer through to codegen.
- `tests`: new lexer, parser, and `neurc` integration tests covering `f32` / `f64` suffixes, the exponent form, annotation consistency, the f32/f64 mismatch error path, and the unchanged default-`f64` behavior for unsuffixed literals.
- `docs`: language reference updated to document float literal type suffixes; new `examples/float_suffixes.nr` runnable example.

---

## [1.17.8] - 2026-05-24

### Changed
- `docs`: README "Current Memory Model" warning rewritten for accuracy — verified that the compiler currently emits zero heap allocations (string literals land in `.rodata` via `build_global_string_ptr`; no `malloc`/`build_malloc`/`build_free` call sites anywhere in `compiler/`), so the previous "every string value is currently leaked" wording overstated the present-day risk. The new wording explains that no leak exists today because no heap ops exist, but every future heap value will leak until ownership semantics (Phase 1.7) ship.
- `docs`: README roadmap table rebalanced to match the new `.idea/roadmap.md` structure — Phase 1.5 narrowed to syntax/semantics stabilization; new Phase 1.7 (ownership & borrow checker) and Phase 1.8 (HIR + `melior` plumbing) extracted from the old Phase 1.5 mega-bucket; async runtime split out of Phase 6 into Phase 7; Python FFI / advanced syntax bundled as Phase 8; developer experience (LSP, formatter) promoted to its own Phase 9; package manager + opt passes as Phase 10.

### Changed (private — not in git)
- `.idea/roadmap.md` rewritten and rebalanced against `.idea/syntax.md` v4.5
  - All previously-checked items preserved verbatim (parser fixes, `const`, compound assignment, `as` casts, `..=`, bitwise ops, integer suffixes, if/block expressions, IEEE-754, integer magnitude rule, `while true` lint, `??` associativity, string fat pointers, string equality, LLVM 20 upgrade)
  - Phase 1.5 scope reduced to frontend / type-checker / scalar-codegen work; added missing items (float literal suffixes, comparison chain rejection, digit separators, integer overflow semantics, `&string` slice type)
  - Phase 1.7 (ownership + borrow checker) extracted as its own multi-month milestone — move semantics, `Copy`, `.clone()`, `&T` / `&mut T`, lifetimes, `Drop`, ARC removal, `unsafe { }` infra, runtime string ops
  - Phase 1.8 (HIR + `melior` plumbing) extracted — `neuro-hir` crate, `mlir-backend` scaffold, HIR-routed lowering
  - Phase 2 subdivided into 2A (arrays, tuples, enums, pattern matching, type aliases, newtypes, struct shorthand/update), 2B (generics, traits, operator traits, closures), 2C (Option/Result, `??`, `?`, `val-else`, modules, `import`/`export`, prelude), 2D (string interpolation, triple-quoted strings, nested comments, named arguments)
  - Phase 3 subdivided into 3A (tensor core + ownership + DLPack + reductions + sort/argsort/topk), 3B (MLIR linalg lowering + matmul), 3C (pool allocator, `PoolAware`, LIFO, await-in-pool diagnostic), 3D (pipeline `|>`, composition `>>`, einsum, functional ops)
  - Phase 4 picked up higher-order derivatives and `@no_grad` / `@detach` — previously missing
  - Phase 5 picked up device management primitives
  - Phase 7 created from the async/concurrency cluster previously buried in Phase 6 — `async func`, `Future<T>`, `spawn`, `JoinHandle`, `join`/`race`, executor
  - Phase 8 created — Python FFI + DLPack + spread + advanced pattern matching + custom attributes + `defer`, all previously absent
  - Phase 9 created — LSP + diagnostic polish + `neuro-fmt`
  - Phase 10 — `neurpm` + optimization passes
  - Cross-cutting tracks documented at top: diagnostics, tests/benchmarks, docs

---

## [1.17.7] - 2026-05-24

### Fixed
- `docs`: README license references updated from NSSL v2.0 to v2.1 (badge, License section heading, and license link) — actual `LICENSE` file is v2.1 since 1.17.5 but README was not updated at the time
- `docs`: README Current Capabilities table — added missing rows for compound assignment operators (`+=`, `-=`, `*=`, `/=`, `%=`, implemented in v1.11.7) and the attribute/lint system (`@allow(...)` + `while true` lint, implemented in v1.17.0)

---

## [1.17.6] - 2026-05-24

### Added
- Demo GIF (`assets/demo.gif`) showing `neurc compile examples/neuron.nr` and instant execution of the resulting native binary, embedded near the top of `README.md`

---

## [1.17.5] - 2026-05-24

### Changed (License v2.0 → v2.1)
- Removed `Non-Public Proprietary Elements` concept (§ 1.7, § 9.3, § 13.1(g), checklist line) and the dependency on a non-existent `PROPRIETARY.md` file — license scope is now fully self-contained and no longer expandable via an external mutable file
- Tightened § 12.3 contributor relicensing: dropped GPL v3 from the enumerated future-license list (semantic mismatch with a source-available project); kept future NSSL versions, Apache 2.0, and mutually agreed licenses; any other relicensing now requires explicit per-contributor written consent
- Added explicit acceptance mechanism for § 12.3 via DCO sign-off — contributors must use `git commit -s`, and unsigned contributions are not accepted
- Added § 12.5 **Patent Grant**: Apache-2.0-style perpetual, worldwide, royalty-free patent license from each Contributor, with defensive patent-retaliation termination — closes a material gap as the project matures
- Added § 1.12 `Patent Claims` definition to support § 12.5
- Softened § 4.3(c) alpha-notice exemption: distributors now assume liability only for their own certification statement and user-facing warranties, not for the upstream Software (which remains governed by §§ 14–15), making the exemption practically usable
- Added § 16.5 mandatory-law / consumer-protection carveout: forum, choice-of-law, and arbitration provisions of § 16 do not override non-waivable mandatory rules in a natural-person Recipient's habitual residence
- § 16.2 arbitration default is now fully remote (written submission + video conference) unless a party demonstrates a specific need for in-person proceedings
- Rewrote § 17.3 severability fallback to use breach-of-confidence / unfair-competition framing instead of trade-secret + proprietary-elements

### Changed (CONTRIBUTING.md)
- New **Developer Certificate of Origin** section documenting the DCO text, the `git commit -s` workflow, the relationship to § 12.3 relicensing acceptance, and the CI enforcement rule
- Pre-submission checklist now requires `Signed-off-by:` on every commit
- License section rewritten to reference NSSL v2.1, § 12.3 acceptance, and the § 12.5 patent grant

---

## [1.17.4] - 2026-05-24

### Changed
- `docs`: replaced factorial Quick Example in README with a compilable `Neuron` perceptron example (`examples/neuron.nr`) that demonstrates structs, `impl` blocks, associated functions, instance methods, if-expressions, and implicit returns — verified to compile and run
- `docs`: elevated memory leak warning from buried table paragraph to a prominent blockquote with contributor call-to-action
- `docs`: rewrote License section with plain-language summary, explicit "what you can do" / "what requires a license" breakdown, and Apache 2.0 transition commitment
- `docs`: updated license badge to reflect "Neuro Shared Source → Apache 2.0" framing

### Changed (License)
- Renamed license from "Neuro Source-Available License v1.0" to "Neuro Shared Source License v2.0"
- Added unconditional **Compiled Program Exemption** (§ 2): programs compiled by Neuro are fully exempt from the license and may be used/sold under any terms
- Added **OSI Transition Commitment**: all code merged before Phase 2 milestone will be concurrently published under Apache 2.0 upon that milestone's announcement
- Added **Tooling and Integration** exemption (§ 3.5): tools, plugins, and editor extensions that invoke the compiler are not subject to the Commercial Distribution restriction
- Contributors explicitly consent to the Apache 2.0 transition via § 12.3

---

## [1.17.3] - 2026-05-24

### Fixed
- `ci`: Windows LLVM setup — replaced the official LLVM NSIS installer (which omits `llvm-config.exe`, headers, and static `.lib` files, making it unusable for `llvm-sys`) with the full MSVC dev build from `vovkos/llvm-package-windows` 20.1.8 (`msvcrt`/`/MD` variant matching Rust's default CRT linkage); fixes "llvm-config.exe not found at C:\\LLVM\\bin\\llvm-config.exe" on `windows-latest` runners

---

## [1.17.2] - 2026-05-20

### Fixed
- `ci`: Windows LLVM setup — detect existing LLVM 20.x dev install before attempting installation; fall back to official NSIS installer (20.1.8) instead of Chocolatey, which fails when a newer runtime-only version is already present on the runner
- `docs`: updated Windows installation guide to LLVM 20.1.8 and clarified install path constraint

---

## [1.17.1] - 2026-05-20

### Added
- `docs`: `SECURITY.md` — vulnerability reporting via GitHub private advisory, response timeline, security surface definition
- `docs`: `CODE_OF_CONDUCT.md` — Contributor Covenant v2.1
- `docs`: `DESIGN.md` — language design principles, non-goals, and AI-first rationale
- `docs`: `DESIGN.md` linked from `README.md` ToC and `CONTRIBUTING.md` codebase reading list

### Fixed
- `docs`: license name in `CONTRIBUTING.md` corrected from "GNU GPL v3.0 with Neuro Exceptions" to "Neuro Source-Available License"

---

## [1.17.0] - 2026-05-20

### Added
- `infra`: `Attribute { name, args, span }` AST node; `FunctionDef` and `MethodDef` now carry `attributes: Vec<Attribute>` (Phase 1.5)
- `parser`: `parse_attributes` consumes `@name` / `@name(arg, ...)` prefixes before any `func` definition, including methods in `impl` blocks
- `semantic`: lint infrastructure — `Warning` / `WarningCode` public types; `type_check` now returns `Result<Vec<Warning>, Vec<TypeError>>`
- `semantic`: `prefer-loop-over-while-true` lint — fires on bare `while true { ... }`; suppressed by `@allow(prefer_loop_over_while_true)` on the enclosing function/method
- `codegen`: lint warnings forwarded to stderr by `neurc check` and `neurc compile`; never block compilation
- `tests`: attribute parsing coverage (free functions, methods, multi-arg, bare, struct-rejection) and lint emission/suppression coverage in semantic-analysis and neurc integration tests
- `docs`: lint section in `docs/language-reference/control-flow.md`; `examples/while_true_lint.nr` runnable demo

---

## [1.16.0] - 2026-05-18

### Added
- `lexer`: `??` token (`TokenKind::QuestionQuestion`) for null/error coalescing
- `parser`: `BinaryOp::NullCoalesce` with R-to-L associativity per Appendix B row 14 (Phase 1.5)
- `tests`: parser tests pinning `a ?? b ?? c` to `a ?? (b ?? c)` and `a ?? b || c` to `a ?? (b || c)`

### Changed
- `semantic`: `??` rejected via new `OperatorNotYetSupported` diagnostic until Option/Result land in Phase 2

---

## [1.15.0] - 2026-05-13

### Changed
- `semantic`: restrict inequality operators `<, >, <=, >=` to numeric types (ints and floats)
- `tests`: add IEEE-754 testing for native float comparisons testing NaN semantics

---

## [1.14.0] - 2026-05-13

### Changed
- `semantic`: unannotated integer literals out of i32 range yield error rather than promote to i64 (Phase 1.5)
- VSA_4_3.xml changed to VSA.md, saving around 2500 tokens in size

---

## [1.13.0] - 2026-04-28

### Added
- **If-expressions and block expressions as values** (Phase 1.5)
  - `if`/`else` chains now produce values when used in expression position:
    `val abs = if x >= 0 { x } else { 0 - x }`
  - `else if` chains work in expression position:
    `val s = if n < 0 { -1 } else if n == 0 { 0 } else { 1 }`
  - Bare block expressions produce the value of their trailing expression:
    `val r = { val a = 3; val b = 4; a + b }`
  - Both forms are fully type-checked: all arms must produce the same type;
    if-without-else has type `Void` and cannot be used as a value.
  - All four compiler stages updated: AST (`Expr::If`, `Expr::Block`),
    parser, type checker, LLVM backend (alloca-based lowering; mem2reg promotes
    to SSA registers in optimised builds).

### Internal
- `codegen_expr` and all callee helpers upgraded from `&self` to `&mut self`
  (required because `Expr::If` codegen appends basic blocks and calls
  `codegen_stmt`).

### Tests
- Added `compiler/neurc/tests/if_block_expressions.rs` — 7 integration tests.
- Added `examples/if_block_expressions.nr` example; wired into `examples.rs` test suite.
  Total test count raised from 428 to 436.

---

## [1.12.3] - 2026-04-27

### Fixed
- **Codegen bug:** `const` declarations with non-i32 types (e.g. `i64`, `u8`) were
  silently emitted as i32 LLVM constants, truncating values > 2 147 483 647.
  Both module-level (`const X: i64 = …`) and function-body (`const X: i64 = …`)
  constants are now emitted at the correct declared bit-width.

### Tests
- Added `compiler/neurc/tests/examples.rs` — 22 integration tests that compile and
  execute every `.nr` file in `examples/` and assert the expected exit code.
  Total test count raised from 406 to 428.

### Documentation
- `docs/getting-started/first-program.md`: fixed f32/f64 code sample (`pi: f32 * 2.0`
  was a type error); updated stale note about float literal inference.
- `docs/language-reference/operators.md`: marked clamping / sign / abs examples that
  use `if`-as-expression as not yet implemented (Phase 1.5); added working workarounds.

---

## [1.12.2] - 2026-04-18

### Documentation
- Added complete Windows 10/11 installation guide to README (MSVC Build Tools,
  rustup, LLVM 20 installer, env var setup, PATH, troubleshooting).
- Aligned Linux/macOS installation sections: added Rust install step and
  `cargo install` step to Ubuntu/Debian and macOS sections.
- Replaced hardcoded Linux-only `LLVM_SYS_201_PREFIX=…` prefix in the
  Development section with a platform-neutral note and a Windows PowerShell
  snippet.

---

## [1.12.1] - 2026-04-18

### Fixed
- Windows CI: NSIS installer called with single-string `/S /D=C:\LLVM` argument
  (array form caused silent misparse, leaving llvm-config in the default path).
- Windows CI: upgraded pinned LLVM from 20.1.2 → 20.1.4; switched download
  from `Invoke-WebRequest` to `curl.exe` for reliability on large binaries.
- Windows CI: added fallback path search (`C:\Program Files\LLVM`) and PATH
  scan so the step self-heals if the custom install dir is ignored.
- Verify step: replaced `"$LLVM_SYS_201_PREFIX/bin/llvm-config"` with plain
  `llvm-config` (from PATH) to avoid Git Bash backslash path failures on
  Windows runners.

---

## [1.12.0] - 2026-04-18

### Added

- **lexer**: Integer literal type suffixes — `42i64`, `255u8`, `0xFFu8`, `0b1010i32`
  - All eight suffix variants: `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`
  - New `TokenKind::IntegerSuffix(IntegerSuffixToken)` emitted by four new regexes (decimal,
    binary, octal, hex) at `priority = 2`; logos maximal munch picks `42i64` as one token
  - `IntegerSuffixToken { value: i64, suffix: IntSuffix }` exported from `lexical-analysis`
  - `IntSuffix` enum added to `shared-types`; `Literal::Integer` now carries `Option<IntSuffix>`
- **parser**: `parse_prefix` maps `IntegerSuffix` tokens to `Literal::Integer(v, Some(s))`
- **semantic**: Suffix present → `infer_suffixed_integer_type` bypasses contextual inference,
  range-checks value, returns the suffix type; `300u8` is a compile error
- **codegen**: `codegen_literal` emits correct LLVM integer width (i8/i16/i32/i64) for suffix;
  `type_pass` infers correct expression type for binary ops on suffixed literals
- **tests**: 6 new integration tests; total 406 passing (was 397)

---

## [1.11.9] - 2026-04-18

### Fixed

- **codegen**: Integer and float variable declarations with explicit type annotations
  now emit the correct LLVM alloca width and coerce the initialiser to match
  - Previously `val x: i64 = 255` emitted an `i32` alloca; passing two such
    variables to an `i64`-typed function caused an LLVM verifier type mismatch
  - Fix: `codegen_var_decl` uses the declared annotation type for the alloca;
    `coerce_if_needed` handles `sext`/`zext`/`trunc`/`fpext`/`fptrunc` as needed
  - Binary expressions (`a + literal`) also coerce the right-hand literal to
    match the left-hand operand's semantic type, eliminating IR type mismatches
  - `type_pass.rs` `type_env` now records the declared annotation type (not the
    literal's default type) so downstream codegen lookups agree
  - Three regression tests added in `type_inference::codegen_regressions`:
    `regression_i64_annotation_creates_i64_alloca`,
    `regression_f32_annotation_truncates_f64_literal`,
    `regression_i64_literal_in_binary_expression`

---

## [1.11.8] - 2026-04-18

### Fixed

- **codegen**: `if`/`else` where all branches return explicitly now compiles correctly
  - Previously emitted a false "missing return" error because the dead merge block
    after all-returning branches had no LLVM terminator
  - Fix: emit `unreachable` for dead merge blocks; LLVM eliminates them during
    optimisation. Applies to both free functions and `impl` methods.
  - Two regression tests added: `regression_if_else_all_branches_return_no_missing_return_error`
    and `regression_else_if_all_branches_return`

---

## [1.11.7] - 2026-04-18

### Added

- **lexer/parser/semantic/codegen**: Bitwise operators `&`, `|`, `^`, `~`, `<<`
  - New tokens: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`); `Amp` wired as binary op
  - New AST variants: `BinaryOp::{BitAnd, BitOr, BitXor, Shl}`, `UnaryOp::BitNot`
  - New precedence levels (Appendix B): `Shift` (7), `BitwiseAnd` (8), `BitwiseXor` (9), `BitwiseOr` (10)
  - Type checker enforces integer-only operands; floats and bools are rejected
  - LLVM codegen: `build_and`/`build_or`/`build_xor`/`build_left_shift`/`build_not`; const folding included
  - 10 integration tests covering all operators, precedence, i64, and type-error rejection

- **lexer/parser/semantic/codegen**: `const` declarations
  - `const NAME: Type = expr` at module scope and inside function bodies
  - Module-level consts emitted as `@NAME = internal constant` LLVM globals; visible
    to all functions via a pre-registration pass (forward references work regardless
    of source order)
  - Function-body consts folded in Rust (`FoldedConst`) and stored as compile-time
    values — no `alloca` emitted
  - RHS must be a constant expression (literals, arithmetic/unary/cast on literals,
    or identifiers of previously declared consts); function calls and runtime values
    are rejected with `InvalidConstExpr`
  - Duplicate const names are rejected at both module and function scope
  - 9 integration tests covering: module const, multiple consts, body const,
    arithmetic folding, forward references, and rejection of non-const/duplicate RHS

- **semantic**: add support for explicit numeric type casts
  - `as` type cast expressions now parsed via Pratt precedence `Cast` level
  - Supports numeric width conversions, signed/unsigned matching, float to int, and bool to int casts
  - Lowered natively into LLVM type conversions (`zext`, `sext`, `trunc`, `fpext`, `fptrunc`, `fptosi`, `fptoui`, `sitofp`, `uitofp`, etc)

- **ast-types/parser/codegen**: Inclusive range `..=` in `for` loops
  - `ForRange` AST node now handles an `inclusive: bool` flag.
  - `for i in 0..=10` emits an inclusive upper bound (`<=`) instead of exclusive (`<`).
- **lexer/parser**: Compound assignment operators `+=`, `-=`, `*=`, `/=`, `%=`
  - Five new token variants in `lexical-analysis`; logos longest-match ensures `+=` etc. are consumed as single tokens
  - `parse_compound_assignment_stmt` desugars `target OP= rhs` → `Stmt::Assignment { value: Expr::Binary }` at parse time
  - No new AST nodes; semantic analysis and codegen unchanged
  - 8 integration tests covering all operators, loop accumulator patterns, and desugar equivalence

### Fixed

- **parser**: `else if` condition — `no_struct_lit` guard missing
  - Setting `no_struct_lit = true` around each `else if` condition prevents a bare
    identifier (e.g. `else if isValid {`) from having its block-opening `{` consumed
    as a struct literal opener, corrupting the parse tree
- **codegen**: `else if … else` chain — final `else` body executed unconditionally
  - Replaced the flat loop over `else_if_blocks` with a recursive `split_first` call
    that passes the remaining arms and the `else_block` down, keeping each arm
    mutually exclusive with all subsequent arms

---

## [0.1.1] - 2026-03-28

### Added

- **parser/semantic/codegen**: `impl` blocks — methods and associated functions on structs
  - `impl TypeName { func method(&self) ... func assoc(args) ... }` parsed as `Item::Impl`
  - `&self` instance methods lowered to LLVM free functions under mangled names `StructName__methodName`; struct passed by value as first parameter
  - Associated functions (no `self`) called via `TypeName::func(args)` path syntax; `Expr::Path` AST node added
  - Method calls `instance.method(args)` recognised in semantic analysis and codegen via `Call { func: FieldAccess }` shape
  - `&mut self` and consuming `self` rejected at semantic analysis with actionable error until ownership semantics land
  - Three-pass `check_program`: struct pre-registration → impl signature registration → body checking
  - `Amp` (`&`) token added to lexer; logos longest-match keeps `&&` as `AmpAmp`
  - 8 new integration tests covering all acceptance criteria

- **parser/semantic/codegen**: Struct types — definition, instantiation, field access, and field mutation
  - `struct Name { field: Type, ... }` declarations parsed as `Item::Struct`
  - Struct literal expressions `Name { field: value, ... }` with full type checking
  - Field read via `.field` infix (`Expr::FieldAccess`), codegen via LLVM `build_struct_gep` + `build_load`
  - Field write `obj.field = value` on `mut` bindings (`Stmt::FieldAssignment`), codegen via `build_struct_gep` + `build_store`
  - Two-pass semantic analysis: structs pre-registered before function bodies are checked, so definition order doesn't matter
  - Nominal typing: `Type::Struct(name)` matched by name; struct-literal fields validated for presence, uniqueness, and type
  - Immutability enforced: mutating a field of a `val` binding is a compile error (`AssignToImmutableField`)
  - `no_struct_lit` parser flag prevents `{ }` after bare identifiers from being parsed as struct literals inside `if`/`while`/`for` conditions
  - 10 integration tests in `compiler/neurc/tests/structs.rs` covering all acceptance criteria
  - `examples/structs.nr` runnable example

- **codegen**: String equality operators `==` and `!=`
  - Lowered to length check (fast path) + `memcmp` call via external libc symbol
  - `select` keeps `memcmp` safe when lengths differ (passes `n=0`)
  - 4 integration tests added to `neurc/tests/string_type.rs`
  - `docs/language-reference/operators.md` and `README.md` updated

- **control-flow**: Exclusive range `for` loops (`for i in 0..n`) end-to-end
  - `Stmt::ForRange` AST node in `ast-types`
  - Parser support for `for <ident> in <expr>..<expr> { ... }`
  - Semantic validation for integer range bounds and loop-scoped iterator binding
  - LLVM codegen with dedicated step block so `continue` advances the iterator correctly
  - Parser, semantic, and neurc integration tests

- **control-flow**: `break` and `continue` for `while` loops
  - `Stmt::Break` and `Stmt::Continue` AST nodes in `ast-types`
  - Semantic validation: `BreakOutsideLoop` / `ContinueOutsideLoop` errors
  - LLVM codegen loop-target stack for `break`/`continue` control transfer

- **control-flow**: `while` loops end-to-end
  - `Stmt::While` AST node; `while <condition> { ... }` syntax
  - Boolean loop condition enforcement in type checker
  - LLVM IR: `while.cond` / `while.body` / `while.exit` basic blocks

- **neurc**: `neurc compile <file.nr>` produces executables on Linux, macOS, Windows
  - Multi-stage linker fallback: clang → lld-link → MSVC/cc
  - Platform-specific object file handling
  - 16 end-to-end integration tests

- **neurc**: CLI contract integration test suite (`tests/cli_contract.rs`)
  - `neurc check` success path writes to stdout, empty stderr
  - `neurc check` type errors return non-zero exit and print diagnostics to stderr
  - `neurc compile` type errors return non-zero exit with failure diagnostics

- **semantic-analysis**: Contextual type inference for numeric literals
  - Integers and floats infer type from declaration/parameter/return context
  - Range validation: `300` cannot be assigned to `i8`
  - Defaults: integers → `i32`, floats → `f64`; large integers auto-promote to `i64`
  - `IntegerLiteralOutOfRange` error type

- **semantic-analysis**: Full type checking
  - Types: i32, i64, f32, f64, bool
  - Function signature validation and lexical scoping with variable shadowing
  - Multiple-error collection (fail-slow strategy)
  - 24 tests

- **semantic-analysis / lexical-analysis / llvm-backend**: String type (Phase 1)
  - `Type::String` in the type system; string literal checking and propagation
  - LLVM IR: string literals as global constants, opaque pointer mapping (LLVM 15+ style)
  - C-style null-terminated implementation (fat-pointer refactor planned for Phase 1.5)
  - Full escape sequence support: `\n`, `\r`, `\t`, `\\`, `\"`, `\0`, `\xNN`, `\u{NNNN}`

- **syntax-parsing / semantic-analysis / llvm-backend**: Mutable variable reassignment
  - `Stmt::Assign` AST node; `mut x: i32 = 0; x = 10` syntax
  - `SymbolInfo` tracks mutability; `AssignToImmutable` error for `val` targets
  - LLVM `store` instruction for assignment codegen

- **semantic-analysis / llvm-backend**: Extended primitive types
  - Signed: i8, i16 (complementing i32, i64)
  - Unsigned: u8, u16, u32, u64
  - Signedness-aware codegen: `sdiv`/`udiv`, `srem`/`urem`, `icmp s*/u*`

- **llvm-backend**: Complete LLVM code generation
  - Function codegen with parameters and return values
  - Expression codegen (arithmetic, comparison, logical)
  - Statement codegen (variables, return, if/else)
  - Object code emission for the native target
  - 4 tests

- **syntax-parsing**: Statement and function parsing
  - Variable declarations, return statements, expression statements
  - Function definitions with parameters and return types
  - If/else with multiple else-if clauses
  - 39 additional tests (65 total)

- **syntax-parsing**: Comprehensive test suite (117 tests)
  - `expression_tests.rs` (34), `statement_tests.rs` (20), `function_tests.rs` (16),
    `error_tests.rs` (31), `integration_tests.rs` (16)

### Fixed

- **codegen**: `codegen_if` branch check used the stale `then_bb`/`else_bb` reference
  after nested control flow moved the builder to an inner merge block — replaced with
  `builder.get_insert_block()` check (mirrors the existing pattern in `codegen_while`)
- **codegen**: `codegen_binary` read `span.start` (result type, e.g. `Bool`) instead of
  `span.start + 1` (left-operand type) — this silently broke float comparisons and
  prevented string equality dispatch

### Changed

- **codegen**: String type now uses fat pointer `{ ptr, i64 }` ABI instead of bare `ptr`
  - `type_mapping.rs`: `Type::String` maps to anonymous LLVM struct `{ ptr, i64 }`
  - `codegen.rs`: string literals built via `insertvalue` instructions; field 0 = pointer to
    null-terminated UTF-8 bytes in `.rodata`, field 1 = byte count excluding null terminator
  - `lib.rs`: target machine relocation model changed from `RelocMode::Default` to
    `RelocMode::PIC` so emitted object files are linkable into PIE executables on modern Linux
  - `llvm-backend/CONTEXT.md`: String ABI section added documenting the fat pointer layout
- **infra**: `OptimizationLevel` default impl replaced with `#[derive(Default)]` + `#[default]`
  on `O0` (clippy `derivable_impls`)
- **llvm-backend**: Upgraded inkwell `0.6.0` (LLVM 18) → `0.8.0` (LLVM 20)
  - Updated `[workspace.dependencies]` inkwell feature flag to `llvm20-1`
  - Raised minimum Rust version (`rust-version`) from `1.70` to `1.85`
  - `LLVM_SYS_201_PREFIX` is now the required build-time env var (e.g. `/usr/lib/llvm20`)
  - Fixed `codegen.rs`: `try_as_basic_value().left()` → `.basic()` (inkwell 0.8 `ValueKind` API)
  - Updated `compiler/llvm-backend/CONTEXT.md` with LLVM 20 reference and future MLIR plan
  - Updated `.idea/roadmap.md` (v4.1) and `.idea/idea.md` with accurate backend stack,
    MLIR lowering strategy, Enzyme MLIR dialect plan, and GPU dialect paths
- **architecture-tests**: Renamed `test_all_slices_have_readme` → `test_all_slices_have_context_md`
  — README.md files replaced by CONTEXT.md across all slices; required sections updated to
  `Purpose`, `Entry Point`, `Data Ownership`, `Shared Kernel`
- **workspace**: Repository and homepage URLs updated to `github.com/PanzerPeter/Neuro`
- **workspace**: `Cargo.lock` format upgraded to version 4 (Cargo 1.85+)
- **neurc**: Improved linker detection with detailed error messages

### Fixed

- **neurc**: Object file linking race condition with tempfile cleanup
- **llvm-backend**: Type inference for identifiers and function calls
- **lexical-analysis**: `InvalidEscape` and `UnterminatedString` no longer masked as `UnexpectedChar`
- **syntax-parsing**: Hardcoded `Span::new(0, 0)` in `token_to_binary_op` error path replaced with the token's actual span
- **syntax-parsing**: Added maximum expression nesting depth (256) to prevent stack overflow
- **syntax-parsing**: Duplicate parameter names in function definitions now produce a compile error
- **semantic-analysis**: Symbol table correctly tracks mutability for all declaration forms

### Architecture

- Extracted AST types from `syntax-parsing` into new `compiler/infrastructure/ast-types` crate
  - `semantic-analysis` and `llvm-backend` now depend on `ast-types`, not `syntax-parsing`
  - VSA cross-slice dependency eliminated; `syntax-parsing` maintains backward-compatible re-exports
- Replaced per-slice `README.md` with `CONTEXT.md` (AI-contract files) across all feature slices
  - Sections: Purpose, Entry Point, Data Ownership, Shared Kernel, Notes
  - Architecture test `test_all_slices_have_context_md` enforces compliance
- Removed direct `llvm-backend` → `semantic-analysis` production dependency
  - `neurc` remains the single orchestration boundary for parse → type-check → codegen
  - `llvm-backend` uses a backend-local type model for codegen decisions

### Infrastructure

- CI: dedicated `Architecture` gate runs `cargo test -p neurc --test architecture_tests`
- CI: docs-consistency gate (`tools/check_docs_consistency.py`) on every push/PR
- CI: benchmark regression gate (`tools/check_benchmark_regression.py`) for `llvm-backend`
- CI: cross-platform release smoke gate — builds `neurc` on Linux, macOS, Windows
  and executes representative examples via `tools/run_release_smoke_tests.py`

---

## [0.1.0] - 2025-01-21

### Initial Release — Lexer and Expression Parser

### Added

- **lexical-analysis**: Complete tokenizer
  - Phase 1 keywords, number literals (binary/octal/hex/decimal/float), string literals,
    line and block comments, source span tracking
  - 28 tests

- **syntax-parsing**: Expression parser with Pratt precedence climbing
  - Literals, identifiers, function calls, binary and unary operators, parenthesized expressions
  - 26 tests

- **infrastructure**: Workspace setup with Vertical Slice Architecture (VSA)
  - inkwell 0.6.0 (LLVM 18 bindings) — replaced by LLVM 20 in Unreleased

### Fixed

- String error reporting (unterminated strings, invalid escapes)
- Redundant closure warnings in Unicode validation
