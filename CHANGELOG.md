# Changelog

All notable changes to the Neuro programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [1.46.0] - 2026-06-19

### Added
- `infra`: integrate `melior` (Rust MLIR bindings) alongside inkwell (Phase 1.8). New `compiler/mlir-backend` slice hosts `melior 0.25.1` — the newest release targeting MLIR 20 (via `mlir-sys 0.5.0`; melior 0.26+ moved to MLIR 21/22) — behind an off-by-default `mlir` feature. With the feature disabled the crate is an empty placeholder, so the default `cargo build/test --workspace` still works on a stock LLVM 20 install with no MLIR toolchain; enabling `mlir` pulls in melior and exposes `emit_smoke_module`, which builds and verifies `func.func @neuro_smoke(index, index) -> index` (func/arith dialects) as the integration smoke test. `mlir-sys` carries no `llvm-sys` dependency and links its own `MLIR` key, so it coexists with inkwell's `llvm-20` link without a Cargo `links` conflict; pointing `MLIR_SYS_200_PREFIX` / `TABLEGEN_200_PREFIX` at the same LLVM 20 build as `LLVM_SYS_201_PREFIX` makes both bindings share one `libLLVM-20` dylib. CI gains an opt-in MLIR + matching libclang 20 install on Linux (`setup-llvm` `mlir` input) for the `--all-features` lint job; the Windows/macOS test legs build the placeholder. The HIR-consuming lowering entry point and a dedicated CI smoke job remain open (pending typed HIR, Phase 1.8 items 2–5).

---

## [1.45.0] - 2026-06-19

### Added
- `codegen`: fixed-size arrays `[T; N]` (Phase 2A, §3.1). Array types `[T; N]`, array literals `[e0, e1, ...]` (with element-type inference and length from the element count), index read `arr[i]` and element assignment `arr[i] = v`, the `arr.len()` builtin (compile-time `u64` length), and direct array iteration `for x in arr` / `for x in &arr` (lowered as a counted loop over the storage — no iterator protocol). Element types are restricted to `Copy` scalar primitives (i8–u64, f16/bf16/f32/f64, bool, char), so an array is itself `Copy` and needs no move/Drop tracking. Out-of-bounds index access panics with a located diagnostic in debug builds (`-O0`); release builds omit the check, matching the integer-overflow policy (§1.2). New diagnostics: `NonCopyArrayElement`, `NotIndexable`, `IndexNotInteger`, `ArrayLengthMismatch`, `CannotInferEmptyArray`. New AST nodes (`Type::Array`, `Expr::ArrayLiteral`, `Expr::Index`, `Stmt::ForEach`, `Stmt::IndexAssignment`), semantic and backend `Type::Array`, `BuiltinMethod::ArrayLen`, `examples/types/arrays.nr`, end-to-end coverage in `neurc/tests/arrays.rs`, and semantic unit tests. Deferred: arrays of non-`Copy` elements (strings, structs), `.enumerate()` indexed iteration (needs tuples, §3.2), and a compile-time bounds-elision pass.

---

## [1.44.0] - 2026-06-18

### Added
- `codegen`: `Drop` trait + deterministic destruction (Phase 1.7, §2.1). `impl Drop for T { func drop(&mut self) { ... } }` defines a destructor that runs automatically when an owned binding of `T` leaves its lexical scope, in reverse declaration order, on **normal** exit only — fall-through, `return`, `break`, and `continue`. A panic aborts without running destructors (§1.2: no stack unwinding, no landing pads). `Drop` is recognized as a compiler-known lang-item (like `Copy`/`Clone`), without the general trait system: the parser accepts `impl TraitName for Type` (`ImplDef::trait_name`); semantic analysis validates the destructor shape (`drop(&mut self)`, no params, no return → `InvalidDropImpl`) and rejects a `Copy` type that implements `Drop` (`DropTypeCannotBeCopy`). The backend (`llvm-backend/src/codegen/drops.rs`) tracks a lexical drop-scope stack and inserts flag-guarded `{struct}__drop` calls at scope exit; each owned binding carries an `i1` drop flag, cleared at every move site, so a value moved out (rebind, return, `break` value, by-value call argument, struct-field store) is dropped exactly once — never double-dropped (§2.2). All drop machinery is inert for programs that declare no `Drop` types. New `examples/types/drop.nr`, `neurc/tests/drop_destructors.rs`, and semantic/parser unit tests. Known limitations (deferred): reassigning a `Drop` binding does not drop its prior value, and a struct's `Drop`-typed fields are not auto-dropped (no recursive drop glue).

---

## [1.43.0] - 2026-06-18

### Added
- `codegen`: string `.slice(range)` (Phase 1.7, §2.7). `s.slice(a..b)` (exclusive) and `s.slice(a..=b)` (inclusive) return a borrowed `&string` view into the receiver's UTF-8 data — zero copy, since strings are immutable; the slice is just `(ptr + start, len)`. Indices are byte offsets. Both build modes run a runtime check that panics (abort, no unwinding) on an out-of-bounds range (`start > end`, `end > len`, negative `start`) or a range endpoint that splits a multi-byte UTF-8 code point. A `&string` receiver auto-derefs (§2.4), and the result is itself a `&string` (so `.slice(...).len()` and `==` work). Range expressions `a..b` / `a..=b` are now a parse node (precedence below `??`), accepted by semantic analysis **only** as a `.slice` argument — used anywhere else they are a `RangeNotAllowed` error; `for`-range loops are unchanged. New `BuiltinMethod::StringSlice`, a `codegen_guard_or_panic` panic-runtime helper, `examples/types/string_slice_method.nr`, and end-to-end coverage in `neurc/tests/string_slice.rs`.

---

## [1.42.1] - 2026-06-18

### Changed
- `docs`: reorganized the private roadmap and synced `CONTRIBUTING.md` so each phase is implementable strictly in order — every open item's prerequisites land in an earlier or the same phase. Resolved `Drop` as a **Phase 1.7 compiler-known lang-item** (recognized specially like `Copy`/`Clone`, reusing impl-blocks + scope/move tracking) rather than gating it on the Phase 2B trait system, making it the keystone next item. Relocated three misplaced items to where their prerequisites live: explicit lifetime annotations `<'a>` → Phase 2B (after Generics, which provides the parse surface); `String.slice(range)` → Phase 1.7 (a string op depending only on `&string` + the panic runtime, both landed); `Layer::require_grad(bool)` → Phase 6 (depends on the Layer trait + `.parameters()`, not the GPU backend). Demoted the duplicate `await`-in-`pool` checkbox in Phase 3 to a forward-dependency note (it is implemented in Phase 7, where `await` exists). No compiler code changed.

---

## [1.42.0] - 2026-06-18

### Added
- `codegen`: string concatenation with `+` (Phase 1.7, §2.7). `a + b` on two strings allocates a fresh heap buffer via libc `malloc`, copies both operands' bytes in with `memcpy`, and returns a new owned, immutable `string` `{ ptr, len }` (no NUL terminator, consistent with the `len` contract). A `&string` slice may stand in for either operand; the result is always an owned `string`, never a reference. Operands are read, not consumed — like `==`, `+` borrows-to-read and never moves, so both stay usable afterward. Semantic analysis peels a single string reference in the arithmetic arm (`string + string -> string`); any other arithmetic operator on a string, or mixing a string with a non-string, is `InvalidBinaryOperator`. `malloc`/`memcpy` join the existing first-use libc externs (`memcmp`/`write`/`abort`). New `neurc/tests/string_concat.rs` end-to-end coverage and `examples/types/string_concat.nr`.

### Notes
- Concatenated buffers are heap-allocated and **not yet freed** — runtime heap strings leak until `Drop` / deterministic destruction lands (Phase 1.7). The growable-builder half of "Runtime string ops" (`String::new` / `.push_str` / `.clear`) stays open: it needs a mutable growable string type, which contradicts the current immutable-`string` spec (§2.7) and depends on `Drop`.

---

## [1.41.6] - 2026-06-18

### Changed
- `docs`: closed the Phase 1.7 "Remove ARC" roadmap item as an audit. A whole-repo scan (`Arc`/`Rc<`/`Arc::new`/`Rc::new`, the `refcount`/`strong_count`/`retain`/`release_ref` vocabulary, and arc-like crate dependencies) returns zero hits — no reference-counting plumbing was ever introduced. The alpha memory model has always been owned-or-borrowed (move-by-default plus `&T`/`&mut T`), so there is nothing to strip. Marked complete in the roadmap and CONTRIBUTING priorities; the `Drop` half is tracked separately and lands with the trait system (Phase 2B §3.9).

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
- `semantic` + `codegen`: `&mut self` instance methods (§2.5) — the Phase 1.7 ownership-gated receiver that was previously rejected. A method may now take `&mut self` and assign to `self.field`; the receiver is passed **by pointer** so the write propagates to the caller's value (a `&self` method stays by-value/read-only, and consuming `self` is still rejected pending the by-value struct ABI). Semantic: `register_impl` no longer rejects `&mut self` and records its mangled key in the new `mut_self_methods` set; `check_impl` binds `self` as a *mutable* variable for `&mut self`; the method-call site runs `check_mut_self_receiver`, which enforces the §2.5 borrow — the receiver must be a `mut` place (or reached through `&mut T`) and must not already be borrowed (`CannotBorrowMutably` / `CannotMutablyBorrowWhileBorrowed`), registering a transient exclusive borrow that clears at statement end. Codegen: `codegen_method` lowers a `&mut self` method's first LLVM parameter as `ptr`, binds `self` directly to it without a copy (recorded type still the struct, so field reads/writes pass through), and seeds `type_env["self"]`; `codegen_method_call` detects a by-pointer callee from its first param type and passes the receiver place's address via `get_struct_ptr_and_type`. Tests: 5 semantic unit (`moves.rs`), 3 `neurc` integration (`methods.rs`), example `structs/mut_self_accumulator.nr`. 677 tests pass.

---

## [1.40.0] - 2026-06-17

### Added
- `semantic`: returned-reference outlives / lifetime elision (§2.6) — the borrow checker now verifies that a function or method whose declared return type is a reference does not return a reference borrowing a place that dies with the call. Under lifetime elision a single input reference lifetime is applied to the output, and the `&self` lifetime is applied to method outputs, so returning one of the reference parameters (or a borrow of `&self`) is sound; borrowing a body-local or a by-value parameter and returning it — directly (`return &local`) or through a local reference binding (`val r = &local; r`) — is rejected with the new `ReturnsReferenceToLocal` diagnostic. New surface: `current_fn_outliving: HashSet<String>` on the type checker (reference-typed parameters plus `self`, rebuilt per function/method and cleared on exit), `SymbolTable::borrow_provenance` (the place a `val r = &x` binding recorded), and `check_returned_reference` (with helpers `tail_expr` / `root_place_name` / `is_local_to_function`) invoked from `Stmt::Return` and both trailing implicit-return sites when the return type is a `Type::Reference`. The walk follows `if`/`else` arms and bare/`unsafe` blocks into their tail expressions. Elision-only — no annotation syntax; ambiguous multi-reference signatures are accepted as long as the borrowee is a parameter (explicit `<'a>` lands with generics, Phase 2B). Conservative: a name absent from the symbol table is treated as non-local, so a valid program is never rejected. Tests: 6 semantic unit, 5 `neurc` integration (`returned_reference.rs`), example `types/returned_reference.nr`. 668 tests pass.

---

## [1.39.0] - 2026-06-16

### Added
- `semantic`: flow-sensitive borrow exclusivity (§2.4, §2.5) — the aliasing rule split out of the lifetime-inference roadmap item because it needs only borrow-region tracking, not full lifetime inference. The borrow checker now enforces that **at most one `&mut` borrow** of a place may be live at a time and that **no `&` borrow coexists with a live `&mut`**; any number of shared `&` borrows may coexist. Borrow regions are lexical: a borrow held by a binding (`val r = &x`) lives until that binding leaves scope; a borrow passed to a call, used in a condition, or returned ends with the statement that took it. New surface: per-binding persistent/transient borrow counters and a borrow-provenance field on `SymbolInfo`; `SymbolTable` methods `borrow_counts` / `add_transient_borrow` / `attach_borrow` / `release_borrow_of` / `clear_transient_borrows`, with `pop_scope` now releasing a dying reference binding's borrow. The `Expr::Reference` arm checks coexistence and registers each borrow; `check_stmt` clears transient borrows at statement end; `VarDecl` / `Assignment` promote a direct `&place` initializer to a persistent borrow (reassigning a `mut` reference releases its old borrow first). New diagnostics `CannotMutablyBorrowWhileBorrowed` / `CannotBorrowWhileMutablyBorrowed`. Lexical, not NLL: only direct-borrow initializers create tracked persistent borrows, so the analysis never rejects a valid program; read/move-while-borrowed and returned-reference outlives are deferred to lifetime inference. Tests: 8 semantic unit, 5 `neurc` integration (`borrow_exclusivity.rs`), example `types/borrow_exclusivity.nr`. 657 tests pass.

---

## [1.38.0] - 2026-06-16

### Added
- `lexer`/`semantic`/`codegen`: `f16` / `bf16` half-precision scalar primitives (§1.2) — the final Phase 1.5 item. `f16` is the IEEE-754 half float; `bf16` is bfloat16 (`f32`-sized exponent range, fewer mantissa bits). Both are first-class scalar primitives with a deliberately **narrow contract**: binding, move/copy (`Copy`), equality (`==` / `!=`), and `as`-cast to/from any numeric type and to/from each other — but **no scalar arithmetic** (`a + b` on a half operand is a compile error directing you to compute in `f32`: `(a as f32 + b as f32)`) and no ordering. Half-precision literals always carry a suffix (`1.5f16`, `0.02bf16`) — there is no contextual default. Full support as tensor element dtypes is separate and lands with the tensor type system (Phase 3A). New surface: lexer `FloatSuffix::F16`/`BF16` (the two float-suffix regexes now match `(bf16|f16|f32|f64)`, split via `split_float_suffix`), semantic `Type::F16`/`BF16` (`is_half_float()`; new `HalfFloatArithmetic` diagnostic), and backend `Type::F16`/`BF16` lowered to LLVM `half` / `bfloat` (float→float casts and `coerce_if_needed` now pick `fpext`/`fptrunc` by bit width; an `f16`↔`bf16` cast routes through `f32`). Tests: 1 lexer unit, 1 semantic unit, 7 `neurc` integration (`half_precision.rs`), example `types/half_precision.nr`. 644 tests pass. **Phase 1.5 (Syntax & Semantics Stabilization) is now complete.**

---

## [1.37.0] - 2026-06-15

### Added
- `lexer`/`parser`/`semantic`/`codegen`: `char` primitive type (§1.2). A `char` is a single 32-bit Unicode scalar value. Char literals are written with single quotes and support escapes — `'a'`, `'\n'`, `'\t'`, `'\\'`, `'\''`, `'\0'`, the `\xNN` byte escape, and the `\u{...}` unicode escape (`'\u{1F44D}'`). `char` is `Copy` (binding it does not move the source), has a built-in total order so all six comparison operators work directly (`'a' < 'b'`), and `as`-casts to and from integer types in both directions (`'A' as i32` → 65, `97 as char` → `'a'`); it is **not** castable to/from `float` or `bool` and has **no** arithmetic (`'a' + 1` is a compile error — compute on the integer code point instead). New surface: lexer `TokenKind::Char(char)` (single-scalar regex; `''`/`'ab'`/unterminated `'a` are lex errors via the new `LexError::InvalidCharLiteral`), `Literal::Char(char)`, semantic `Type::Char`, and backend `Type::Char` lowered to LLVM `i32`. Tests: 3 lexer unit, 1 semantic unit, 6 `neurc` integration (`char_type.rs`), example `types/char.nr`. 635 tests pass.

---

## [1.36.0] - 2026-06-15

### Added
- `parser`/`semantic`/`codegen`: `loop` as a value expression — `break value` (§3.7). A `loop` used in expression position evaluates to the value carried out by its `break`: `val first = loop { ... break v }`. All value-carrying `break`s for one loop must agree on type; `break value` targeting a `while`/`for` is rejected, since those always yield unit (only `loop` is guaranteed to leave solely via a `break`). `break label value` carries a value out of a labeled outer loop, and a labeled loop may itself appear in expression position (`val x = outer: loop { ... }`). New surface: `Expr::Loop { label, body, span }` (distinct from the statement `Stmt::Loop`, whose value is discarded) and a `value: Option<Expr>` field on `Stmt::Break`. The parser disambiguates `break ident` — an identifier is read as a label only when it names an in-scope loop (tracked in a new parser label stack), otherwise it begins the value expression. Semantic analysis replaces the `loop_labels` stack with a `loop_stack` of per-loop contexts that accumulate the agreed value-break type (new `BreakValueInUnitLoop` error; type disagreement reuses `Mismatch`). Codegen allocates a result slot per value loop; a value `break` stores into it before branching and the loop expression loads it at exit. Tests: 2 parser unit, 3 semantic unit, 4 `neurc` integration (`loop_value.rs`), example `control_flow/loop_value.nr`. 626 tests pass.

---

## [1.35.0] - 2026-06-15

### Added
- `parser`/`semantic`/`codegen`: loop labels with `break label` / `continue label` (§3.7). A `for`, `while`, or `loop` may be prefixed with a label — an identifier followed by a colon (`outer:`) — and a nested `break outer` / `continue outer` then targets the labeled loop rather than the innermost one. This is the construct that lets an inner loop exit or re-enter an enclosing loop directly. New surface: an `Option<Identifier>` `label` field on `Stmt::While` / `ForRange` / `Loop` and on `Stmt::Break` / `Continue` (labels reuse the existing `Identifier` + `Colon` tokens, so the lexer is unchanged). The parser dispatches `ident : <loop-keyword>` to the matching loop parser and reads an optional same-line label after `break` / `continue`. Semantic analysis replaces the `loop_depth` counter with a `loop_labels` stack: an unlabeled `break`/`continue` still requires an enclosing loop, and a labeled one requires a matching active label (else the new `UndefinedLabel` error). Codegen adds a `label` to `LoopTargets` and resolves a labeled jump by scanning the loop-target stack from innermost outward. Tests: 3 parser unit, 3 semantic unit, 4 `neurc` integration (`labeled_breaks.rs`), example `control_flow/labeled_breaks.nr`. 617 tests pass.

---

## [1.34.0] - 2026-06-09

### Added
- `lexer`/`parser`/`semantic`/`codegen`: `loop { ... }` infinite-loop statement (§3.7). `loop` is the canonical infinite loop — the form the `prefer-loop-over-while-true` lint already recommended in place of `while true { ... }`, but which previously did not exist, so following the compiler's own advice produced broken code. `loop` has no condition: the only exit is `break`, and `continue` re-enters the body from the top. New surface: the `loop` keyword (`TokenKind::Loop`) and the `Stmt::Loop { body, span }` AST node. Semantic analysis increments `loop_depth` for the body (so `break`/`continue` inside are in-loop) and the lint walker recurses into `loop` bodies; codegen lowers it to an unconditional back-edge (`loop.body` branches to itself), mirroring `while` minus the condition block. A `loop` statement evaluates to unit; the value-producing `break value` form is not yet modelled (tracked on the roadmap). Tests: 1 lexer unit, 1 parser unit, 2 semantic integration, 2 `neurc` integration (`control_flow.rs`), example `control_flow/loop_statement.nr`. 607 tests pass.

---

## [1.33.0] - 2026-06-09

### Added
- `parser`/`semantic`/`codegen`: mutable borrows `&mut T` and the dereference operator `*` (§2.5, Phase 1.7). A `&mut T` reference grants write access to a `mut` binding without taking ownership; values are read and written through the new prefix `*` operator. New surface: the `&mut place` borrow expression, the `&mut T` type annotation (params/returns/locals), the `*expr` dereference expression, and the `*place = value` assignment-through-a-reference statement. Borrow rules enforced in semantic analysis: `&mut` requires a `mut` binding (`CannotBorrowMutably`); `*` applies only to a reference (`CannotDereference`); writing through `*` requires a `&mut` (`CannotAssignThroughRef`). `&mut T` and `&T` are distinct types with no implicit coercion (explicit over implicit). Codegen lowers a borrow to the place's storage pointer (mutability is a compile-time-only distinction) and a deref to a load/store through that pointer. Side fix needed by the canonical example: unit-returning function calls in statement position no longer error (`codegen_call`/`codegen_method_call` now return `Option`, discarded in statement position); and a new line beginning with `*` is parsed as a dereference statement rather than glued to the previous expression as a continued multiplication. **Deferred** (mirrors how `&T` shipped without lifetime checking): flow-sensitive aliasing exclusivity — the "at most one `&mut` at a time, no `&` may coexist" rule — lands with lifetime inference, which shares the same borrow-region analysis. Tests: 4 semantic unit, 1 type-system unit, 8 integration (`neurc/tests/mutable_borrows.rs`), example `showcase/mutable_borrows.nr`. 601 tests pass.

---

## [1.32.0] - 2026-06-09

### Added
- `semantic`/`codegen`: `&string` slice equality (§2.7, Phase 1.7). `&string` is now usable as a borrowed string slice: the equality operators `==` / `!=` compare the underlying UTF-8 bytes for any combination of an owned `string` and a `&string` slice (`&a == &b`, `&a == "lit"`, `"lit" == &a`). Semantic analysis normalizes a single string reference via `Type::peel_string_ref` in the `Equal`/`NotEqual` arm, so a slice and an owned string are equality-compatible; peeling is limited to `string`, so `i32 == &string` and `&i32 == i32` stay type errors (reading other `&T` through `==` needs the deref operator, which lands with `&mut T`). Codegen handles string equality before the numeric coercion: each operand is normalized to its `{ ptr, len }` fat pointer by the new `load_string_fatptr` helper (a borrow is loaded through its pointer; an owned struct value passes through) and compared with the existing `codegen_string_eq` — the ABI is unchanged, only the borrowed operand is auto-dereferenced. Borrowing for a comparison never moves, so both operands stay usable afterward. Tests: 1 semantic-type unit, 7 integration (`neurc/tests/string_slice.rs`), example `types/string_slice.nr`. 588 tests pass.

---

## [1.31.1] - 2026-06-08

### Changed
- `codegen`: audit cleanup — replace the banned `unimplemented!()` stub on the tensor arm of `Type::from_ast` (`llvm-backend`) with a documented `unreachable!()` invariant. Tensor annotations are rejected by semantic analysis before codegen, so the arm asserts an invariant rather than stubbing a missing feature (AC-004 compliance). No behavioral change; 580 tests pass.

---

## [1.31.0] - 2026-06-08

### Added
- `semantic`/`codegen`: immutable borrows `&T` (§2.4, Phase 1.7). A new reference type `&T` is accepted in any type-annotation position (parameters, returns, locals), and a prefix `&place` borrow expression takes a non-owning reference to a variable. Borrowing **does not move** the borrowed value — `length(&msg); msg.len()` compiles — and `&T` is itself `Copy`, so a reference can be passed and re-borrowed freely. Method and field access auto-deref through a borrow: `s.len()` / `s.clone()` work on `&string`, and `r.field` / `r.method()` work on `&Struct`. Borrowing a temporary (a literal, a call result) or a `const` (an inlined value, not a place) is a new `CannotBorrowValue` error. References lower to opaque LLVM 20 pointers; a borrow of a place is its alloca pointer, and consuming sites load through the pointer when the receiver is a borrow (value-driven, so owned and borrowed receivers share one path). Integer intrinsics (`wrapping_*`, `.shr`) intentionally still require a value receiver — reading a scalar through `&T` needs the deref operator, which lands with `&mut T`. Implementation spans `ast-types` (`Type::Reference`, `Expr::Reference`), `syntax-parsing` (`&T` type + prefix `&` borrow), `semantic-analysis` (`Type::Reference`, no-move/Copy rules, auto-deref, `CannotBorrowValue`), and `llvm-backend` (`Type::Reference` → `ptr`, borrow + auto-deref codegen). Tests: 1 semantic-type unit, 3 move-checker unit, 6 integration (`neurc/tests/immutable_borrows.rs`), example `types/immutable_borrows.nr`. 580 tests pass.

---

## [1.30.0] - 2026-06-07

### Added
- `semantic`: `Copy` trait + `@derive(Copy, Clone)` for structs (§2.3, Phase 1.7). A struct that derives `Copy` is duplicated on assignment instead of moved, so `val b = a` leaves `a` usable; a struct that does *not* derive `Copy` is now move-tracked like `string` — binding/assigning/returning/passing it by value moves the source, and reading it afterward is a `UseOfMovedValue` error. Deriving `Copy` requires every field to be `Copy` (primitive scalars are `Copy`, `string` is not, a struct field is `Copy` only when it derives `Copy`); a violation is a new `CopyDeriveNonCopyField` error. `Copy` implies `Clone`. `@derive(Clone)` (or `Copy`) enables `struct.clone()` as a compiler-known builtin deep copy; a user-defined `clone` method shadows it. Unknown derive arguments (e.g. `Debug`) are accepted and ignored for forward compatibility. Move-tracking is now context-aware (`TypeChecker::is_type_move_tracked` / `is_type_copy`) rather than a property of the type alone. `@derive(...)` attributes now attach to struct definitions (parser + `StructDef.attributes`). Implementation spans `ast-types` (`StructDef.attributes`), `syntax-parsing` (struct attribute parsing), `semantic-analysis` (`copy_structs`/`clone_structs` registries, Copy-field validation pass, struct `.clone()` resolution), and `llvm-backend` (`BuiltinMethod::StructClone` — loads the aggregate value, a faithful copy while structs are stack-allocated). Tests: 3 parser unit, 5 semantic unit, 5 integration (`neurc/tests/copy_clone.rs`), example `types/copy_clone.nr`. 570 tests pass.

---

## [1.29.0] - 2026-06-07

### Added
- `semantic`: move semantics by default (§2.2, Phase 1.7). Non-`Copy` owned values are *moved* out of their source binding when bound (`val s2 = s1`), assigned, returned, passed by value to a call, or stored into a struct field; reading the source afterward is a new `UseOfMovedValue` error pointing at where the move happened. `.clone()` (already a builtin) borrows its receiver and is the canonical opt-out. Move tracking is limited to `string` — the only non-`Copy` type the language can construct today; structs stay freely duplicable until `Copy`/`@derive(Copy)` lands (the next Phase 1.7 item). The checker is conservative: it flags only direct place expressions in a consuming position, and `if`/`while`/`for` bodies and `if`-expression arms snapshot/restore move state so a conditional move never leaks onto a path that did not execute it (no false positives; may miss e.g. second-iteration loop moves). Implementation: new `type_checkers/moves.rs` (`record_move`), `SymbolInfo.moved_at` plus `mark_moved`/`clear_moved`/`snapshot_moves`/`restore_moves` on `SymbolTable`, `Type::is_move_tracked()`. Tests: 6 unit (`semantic-analysis`), 5 integration (`neurc/tests/move_semantics.rs`), example `types/move_semantics.nr`. 557 tests pass.

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
- `parser`: struct field-init shorthand and functional-update syntax (§3.3, Phase 2A). `Point { x, y }` desugars each bare field to `x: x` at parse time (a `FieldInit` whose value is `Expr::Identifier(field_name)` — no AST node, so semantic analysis and codegen are unchanged for shorthand; an undefined name surfaces as the ordinary undefined-variable error). `Point { x: 1.0, ..p }` adds a functional-update base: `Expr::StructLiteral` gained `base: Option<Box<Expr>>`, and the parser stops the field scan at a trailing `..expr`. Semantic analysis checks the base against `Type::Struct(name)` (wrong struct → `Mismatch`) and, when a base is present, skips the missing-field scan since `..base` supplies every unlisted field; a base-less literal still requires all fields (`MissingStructField`). Codegen seeds the LLVM aggregate from the base struct value instead of `undef`, then `insert_value` overwrites each explicit field, so unlisted fields keep the base's values with no reallocation. The type-alias rewrite pass recurses into `base`. Tests: 5 parser unit (`syntax-parsing/tests/expression_tests.rs`), 8 integration (`neurc/tests/struct_shorthand_update.rs`), example `structs/struct_update.nr`. 543 tests pass.

---

## [1.27.0] - 2026-06-05

### Added
- `codegen`: `string.clone()` builtin method (§2.7, Phase 1.7). Returns a fresh `string` equal to its receiver — the canonical opt-out of move-by-default for non-`Copy` types. Resolved on a `string` receiver in both `semantic-analysis` (`resolve_builtin_method` → `Type::String`) and `llvm-backend` (`BuiltinMethod::StringClone`), duplicated per VSA so neither slice depends on the other. Lowering copies the `{ ptr, len }` fat-pointer value: today strings are immutable and `.rodata`-backed (no heap string type yet), so a value copy is observationally a deep copy; when runtime heap strings land this must duplicate the underlying buffer. Takes no arguments (`ArgumentCountMismatch` otherwise); `.clone()` on a non-`string` receiver remains `MethodNotFound` (`Copy` scalars take the assignment path). Also fixed a latent `span.start` collision: chaining two builtin calls (`s.clone().len()`) nests two `Call` nodes sharing `span.start`, so the `builtin_methods` dispatch map is now keyed by the full span `(start, end)` — unique per node — matching the existing `binary_left_types` workaround. Tests: 2 semantic unit, 1 backend unit, 3 integration (`neurc/tests/builtin_methods.rs`), example `types/string_clone.nr` (exit 5). 530 tests pass.

---

## [1.26.2] - 2026-06-04

### Changed
- `build`: dependency maintenance pass. Removed three dead dependencies from `syntax-parsing` — `lalrpop`, `lalrpop-util`, and `chumsky` — which were declared but never imported (the parser is hand-written Pratt; there was no `.lalrpop` grammar or `build.rs`). This drops their transitive build-time tree (`petgraph`, `regex`, `string_cache`, `bit-set`, `term`, …), trimming the dependency surface and build time. Upgraded `inkwell` 0.8.0 → 0.9.0 (still LLVM 20 via `llvm20-1`; 0.9 adds LLVM 21/22 support and ports to Rust edition 2024 — no backend code changes required), `thiserror` 1 → 2, `toml` 0.8 → 1.1, and `criterion` 0.5 → 0.8 (dev/bench only). `cargo update` swept caret-range drift (unicode-segmentation 1.13, unicode-ident 1.0.24, tempfile 3.27, etc.). `logos` deliberately held at 0.14 — the 0.16 engine rewrite trades a slight lexer perf regression for regex correctness we do not currently need. All 526 tests pass; clippy clean.

---

## [1.26.1] - 2026-06-04

### Fixed
- `codegen`: short-circuit `&&` / `||` with a comparison as the **left** operand miscompiled (§1.4). The type pass stored each binary node's left-operand type at `expr_types[span.start + 1]`, but a binary node and its leftmost descendant share the same `span.start`; the parent's write (e.g. `&&`, left type `Bool`) clobbered the left child comparison's entry (e.g. `i32`). The leftmost comparison was then generated with `left_ty = Bool`, truncating its i32 operands to i1 — so `c >= 48 && c <= 57` with `c = 51` wrongly evaluated to `false` (the i1 `-1 >= 0` is false), while `c == 51 && …` and parenthesized `(c >= 48) && …` happened to work. Left-operand types now live in a dedicated `binary_left_types` map keyed by the full span `(start, end)`, which is unique per node and immune to the `span.start` collision. Regression test: `compiler/neurc/tests/short_circuit_runtime.rs`.

---

## [1.26.0] - 2026-06-04

### Added
- `codegen`: panic runtime — `panic` / `assert` / `unreachable` (Phase 1.7, syntax §1.2). The three panic-family builtins now lower end-to-end with the **abort, no unwinding** contract: `panic(msg: string)` and `unreachable()` print a diagnostic to stderr and terminate via libc `abort()` (SIGABRT); `assert(cond: bool)` branches and aborts only when the condition is false. Diagnostics carry the source location (`message at file:line:col`), threaded into `llvm_backend::compile` via two new `source` / `source_path` parameters and rendered through `source_location::SourceFile`. The diagnostic is written with the POSIX `write` syscall to stderr (fd 2) so it reaches the terminal before the process dies; no unwinding landing pads are emitted, so `Drop`/`defer` (future) will fire only on normal scope exit. The builtins are recognized in both `semantic-analysis` (`resolve_panic_builtin`, returning the divergent `Unknown` type so a panic satisfies any return/binding context, e.g. `func f() -> i32 { panic("x") }`) and `llvm-backend` (`is_panic_builtin` + `panic.rs`); a user function of the same name shadows the builtin. Statements after a divergent call are dropped via new terminated-block guards in `codegen_stmt`/`codegen_return`/`codegen_body`. `checked_*`-style value-position panics and rerouting integer-overflow/bounds checks through this runtime remain follow-ups.

---

## [1.25.0] - 2026-06-04

### Added
- `parser`: `unsafe { }` block infrastructure (Phase 1.7 groundwork, syntax §3 / §6.3). `unsafe` is now a reserved keyword (`TokenKind::Unsafe`) and `unsafe { … }` parses to a dedicated `Expr::Unsafe { stmts, span }` AST node. The block is an ordinary statement block: it introduces a scope and evaluates to its trailing expression, so it works as an implicit return, a `val` initializer, or a void statement. `unsafe` is deliberately **inert** — it carries no special semantics yet and lowers to identical IR as a bare block (`codegen_block_expr`). The distinct node exists so the Phase 5 GPU-kernel aliasing model can later gate raw `KernelOut` index writes behind `unsafe { }` without re-shaping the grammar. Reserving the keyword means it can no longer be used as an identifier.

---

## [1.24.1] - 2026-06-03

### Changed
- `docs`: synced phase status across all Markdown docs to reflect Phase 1.5 (syntax & semantics stabilization) as complete and Phase 1.7 (ownership & borrow checker) as the active phase. Removed the now-complete Phase 1.5 checklist from `CONTRIBUTING.md`; updated status/roadmap lines in `README.md`, `docs/README.md`, `docs/getting-started/quick-start.md`; corrected "not yet implemented" lists (if/block expressions are implemented; ownership → 1.7, string interpolation → Phase 2); retargeted stale `Phase 1.5` references in `docs/language-reference/{operators,control-flow}.md` and `compiler/semantic-analysis/CONTEXT.md`.

---

## [1.24.0] - 2026-06-03

### Added
- `parser`: type aliases (§3.14). `type Name = TargetType` introduces a transparent alias — the alias and its target are interchangeable and no new nominal type is created. Aliases resolve in every type-annotation position (variable/const annotations, function parameters and return types, struct fields, and `as` cast targets) and collapse through chains (`type A = B; type B = i32`). Resolution happens entirely at parse time by substituting each aliased annotation with its target type (the same desugaring strategy used for compound assignment), so semantic analysis and codegen are unchanged and never observe an alias. New `TokenKind::Type` keyword and `ParseError::{DuplicateTypeAlias, TypeAliasShadowsBuiltin, CyclicTypeAlias}` diagnostics reject duplicate aliases, aliases that shadow a built-in type, and cyclic alias chains; an unknown target type is still reported by the existing semantic `UnknownTypeName` check against the resolved type, with the span anchored at the alias use site. Scope note: alias substitution applies to type positions only — using an alias as a value-position constructor or path name is not part of this feature.

---

## [1.23.4] - 2026-06-03

### Fixed
- `build`: release smoke-test harness (`tools/run_release_smoke_tests.py`) referenced `examples/milestone.nr` and `examples/factorial.nr`, which moved to `examples/basics/` when the examples were reorganized. The hard-coded list now points at `basics/milestone.nr` and `basics/factorial.nr`, so the Windows/Linux/macOS release CI jobs find and compile them again (exit codes 8 and 120 unchanged).
- `docs`: updated stale `examples/milestone.nr` / `examples/factorial.nr` paths across `README.md` and `docs/` to the current `examples/basics/` locations.
- `ci`: the Test Suite matrix installed Rust via `dtolnay/rust-toolchain@stable` while overriding `toolchain: ${{ matrix.rust_version }}`. The `@stable` tag hardcodes the channel in the action ref (input `toolchain` is `required: false, default: stable`), so the dynamic override is unsupported and the `nightly` leg bailed out in the action's parse step with `'toolchain' is a required input`. The matrix step now uses `@master` (where `toolchain` is `required: true`), the documented pattern for channels that vary by matrix value. The six fixed-stable `@stable` uses elsewhere are unchanged.

---

## [1.23.3] - 2026-06-02

### Fixed
- `codegen`: logical `&&` and `||` now short-circuit (§1.4), as the language spec and `docs/language-reference/operators.md` have always promised. The backend previously evaluated **both** operands eagerly and combined them with a plain `and`/`or` on the two `i1` values, so the right-hand side always ran — meaning a guard like `if x != 0 && 10 / x > 0` still executed the division when `x == 0` (SIGFPE), and any RHS side effect fired unconditionally. `codegen_binary` now intercepts `&&`/`||` before operand evaluation and lowers them through `codegen_short_circuit`, which branches on the LHS and only evaluates the RHS on the deciding edge, merging the two results with a phi node (`&&` → `if lhs { rhs } else { false }`, `||` → `if lhs { true } else { rhs }`). The phi captures the true predecessor blocks after each side is emitted, so a RHS that itself appends blocks (e.g. a nested `if`-expression) is handled correctly.
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
- `codegen`/`docs`: formalized the string literal vs runtime string distinction (Phase 1.5 §2.7). String literals are emitted to `.rodata` (never heap-allocated); the trailing NUL is now the named `STRING_NULL_TERMINATOR` constant in `literals.rs`, documented as a C-string/FFI convenience that the fat-pointer `len` deliberately excludes. `len` is the authoritative UTF-8 byte count — interior NUL bytes are legal content and are counted, so consumers must not treat string data as NUL-terminated. Behaviour is unchanged (codegen already computed `len` this way); this item formalizes, documents, and tests the guarantee. New end-to-end tests cover multibyte UTF-8 (`"héllo".len() == 6`) and an interior NUL (`"a\0b".len() == 3`). 506 tests passing.

---

## [1.22.1] - 2026-05-31

### Changed
- `build`/`tests`: refactored the three largest source files into focused modules with no behaviour change (504 tests still pass). `llvm-backend/src/codegen/expressions.rs` (1609 lines) split into an `expressions/` submodule — `mod.rs` (dispatch + shared helpers), `literals.rs`, `binary.rs`, `unary.rs`, `methods.rs`, `control_flow.rs` — each adding to the same `impl CodegenContext` block. `semantic-analysis/tests/integration_tests.rs` (980 lines) split into seven feature suites (`semantics_{functions,control_flow,integers,errors,expression_returns,strings,lints}.rs`). The lexer's `lib.rs` test module (≈540 lines) moved to `lexical-analysis/src/tests.rs`, leaving the slice entry point at 137 lines.

---

## [1.22.0] - 2026-05-31

### Added
- `semantic`/`codegen`: integer primitive methods `wrapping_{add,sub,mul}`, `saturating_{add,sub,mul}`, and the right-shift method `.shr(n)` (Phase 1.5 §1.2, §1.4). Each resolves on any integer receiver, takes one same-typed argument, and returns the receiver's type. Wrapping ops emit plain non-trapping two's-complement arithmetic (they never panic, even in debug builds). `.shr(n)` lowers to `ashr` for signed receivers and `lshr` for unsigned. Saturating add/sub use the `llvm.{s,u}{add,sub}.sat` intrinsics; saturating mul uses `{s,u}mul.with.overflow` and selects the saturation bound (unsigned → MAX; signed → MIN/MAX by product sign). Non-integer receivers report `MethodNotFound`; wrong arity reports `ArgumentCountMismatch`; a mismatched argument type reports a type `Mismatch`. `checked_*` (returns `Option<T>`) stays deferred to Phase 2C.

---

## [1.21.0] - 2026-05-31

### Added
- `semantic`/`codegen`: builtin method dispatch on primitive & string types (Phase 1.5 §2). Method-call syntax `receiver.method(args)` now resolves a fixed, compiler-known set of intrinsic methods when the receiver is a non-struct (primitive or string) type, in addition to user-defined `impl` methods. The first intrinsic is `string.len() -> u64` (§2.7), which lowers to a single `extractvalue` of the fat pointer's stored byte length (O(1), no scan, excludes the null terminator). Unknown builtin methods still report `MethodNotFound`; wrong arity reports `ArgumentCountMismatch`. This unblocks the integer `wrapping_*` / `saturating_*` / `.shr(n)` methods tracked as a separate roadmap item.

---

## [1.20.1] - 2026-05-30

### Fixed
- `build`: disable inkwell's `target-all` default feature so only the x86 target is compiled in. The previous config additively enabled all 17 LLVM target initializers, which failed to link on Windows CI (whose prebuilt LLVM only ships the x86 target libs) with ~79 unresolved `LLVMInitialize*` symbols.
- `tests`: make the integer-overflow end-to-end tests cross-platform. `llvm.trap` is delivered as a signal on Unix (no exit code) but as a negative NTSTATUS exit code on Windows; wrapped-result exit codes are 8-bit on Unix but full-width on Windows. Trap detection and wrap-value checks now handle both.

---

## [1.20.0] - 2026-05-30

### Added
- `codegen`: integer overflow semantics (§1.2). Runtime `+`, `-`, and `*` on integer types now trap at runtime in debug builds (`-O0`) via the LLVM `{s,u}{add,sub,mul}.with.overflow` intrinsics + `llvm.trap`, and wrap (two's complement) in release builds (`-O1..-O3`). Division, modulo, bitwise ops, and floats are unaffected; compile-time constant folding continues to wrap.
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
- `tests`: dedicated coverage for underscore digit separators in numeric literals (§1.2) — 5 lexer unit tests (decimal, hex/binary/octal, float fractional + exponent, suffixed int/float, leading-underscore boundary) and 4 end-to-end compile-and-run integration tests.
- `docs`: `examples/underscore_separators.nr` plus a "Digit Separators" note in the type-system reference.

### Notes
- Lexer support for `_` separators already shipped incidentally with the literal-suffix regexes (every numeric pattern carries `_` in its character class and each parser strips it). This release formally validates, documents, and closes out the Phase 1.5 §1 roadmap item — no production code changed.

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
- `semantic`: comparison chain rejection (§1.4) — `a < b < c` is now a compile error with an actionable "use `&&` to combine separate comparisons" suggestion. Covers all six comparison operators (`<`, `>`, `<=`, `>=`, `==`, `!=`). Detection fires in semantic analysis when a comparison operator's LHS is itself a comparison expression.
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
- `lexer`: float literal type suffixes `f32` / `f64` (§1.2, §1.4) — `1.5f32`, `2.0f64`, `1e10f32`, `1.5e-5f64` now tokenize to a dedicated `TokenKind::FloatSuffix(FloatSuffixToken { value, suffix })` token, mirroring the existing integer-suffix encoding. Two new `priority = 3` regexes (fractional and exponent-only forms) sit above the bare-float patterns so logos longest-match always picks the suffixed token.
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
  - Phase 4 picked up higher-order derivatives (§5.3) and `@no_grad` / `@detach` (§5.4) — previously missing
  - Phase 5 picked up device management primitives (§6.2)
  - Phase 7 created from the async/concurrency cluster previously buried in Phase 6 — `async func`, `Future<T>`, `spawn`, `JoinHandle`, `join`/`race`, executor
  - Phase 8 created — Python FFI + DLPack + spread (§8.1) + advanced pattern matching (§8.2) + custom attributes (§8.4) + `defer` (§8.5), all previously absent
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
- `semantic`: `prefer-loop-over-while-true` lint §3.7 — fires on bare `while true { ... }`; suppressed by `@allow(prefer_loop_over_while_true)` on the enclosing function/method
- `codegen`: lint warnings forwarded to stderr by `neurc check` and `neurc compile`; never block compilation
- `tests`: attribute parsing coverage (free functions, methods, multi-arg, bare, struct-rejection) and lint emission/suppression coverage in semantic-analysis and neurc integration tests
- `docs`: §3.7 lint section in `docs/language-reference/control-flow.md`; `examples/while_true_lint.nr` runnable demo

---

## [1.16.0] - 2026-05-18

### Added
- `lexer`: `??` token (`TokenKind::QuestionQuestion`) for null/error coalescing
- `parser`: `BinaryOp::NullCoalesce` with R-to-L associativity per Appendix B row 14 (Phase 1.5 §3.11)
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
- **If-expressions and block expressions as values** (Phase 1.5 §1.8)
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

- **lexer**: Integer literal type suffixes §1.4 — `42i64`, `255u8`, `0xFFu8`, `0b1010i32`
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

- **lexer/parser/semantic/codegen**: Bitwise operators `&`, `|`, `^`, `~`, `<<` (§1.4)
  - New tokens: `Pipe` (`|`), `Caret` (`^`), `Tilde` (`~`), `LeftShift` (`<<`); `Amp` wired as binary op
  - New AST variants: `BinaryOp::{BitAnd, BitOr, BitXor, Shl}`, `UnaryOp::BitNot`
  - New precedence levels (Appendix B): `Shift` (7), `BitwiseAnd` (8), `BitwiseXor` (9), `BitwiseOr` (10)
  - Type checker enforces integer-only operands; floats and bools are rejected
  - LLVM codegen: `build_and`/`build_or`/`build_xor`/`build_left_shift`/`build_not`; const folding included
  - 10 integration tests covering all operators, precedence, i64, and type-error rejection

- **lexer/parser/semantic/codegen**: `const` declarations (§1.3)
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

- **ast-types/parser/codegen**: Inclusive range `..=` in `for` loops (§1.6)
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
