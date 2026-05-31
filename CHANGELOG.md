# Changelog

All notable changes to the NEURO programming language compiler will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

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
