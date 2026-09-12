# Vertical Slice Architecture: Neuro Compiler Rules v5.0

> **Priority: highest.** Every rule here is a build rule. Violations of a BLOCKER rule
> must be refused: state the rule ID and propose a compliant alternative.
> **Context:** 1 human + 1 AI agent. Rust compiler workspace. No backward compatibility.
> **Scope:** this file owns architecture only. Versioning, roadmap, and execution
> protocol live elsewhere and are not restated here.
> Supersedes: the generic v4.4 base ruleset.

---

## 1. Terminology

| Term | Definition |
|------|-----------|
| **Slice** | One compiler capability owning its own entry point, internals, and tests. A crate under `compiler/`. Zero runtime dependency on other Slices. |
| **Shared Kernel** | The crates under `compiler/infrastructure/`. Contract types and adapters only: no compilation logic. |
| **Driver** | `neurc`. The only crate permitted to depend on every Slice. Owns pipeline orchestration. |
| **Contract Type** | A type crossing a Slice boundary: AST nodes, HIR nodes, `Span`, diagnostics. Lives in the Shared Kernel, never in a Slice. |
| **CONTEXT.md** | Contract file at each crate root. The agent's cross-session memory for that Slice. Not documentation. |
| **Hard Gate** | A point where the agent must pause for explicit written confirmation. |

A Slice here is a **pipeline stage**, not a business transaction. Stages are ordered and
each consumes the previous stage's output type. Independence means no stage imports
another; it does not mean stages are unordered.

---

## 2. Project Context

**`CTX-001` [BLOCKER]** No migration paths, compatibility shims, versioned APIs,
deprecated code paths, or adapter layers. If a better design exists, refactor completely.
Leaving old and new patterns coexisting after any single step is forbidden.

**`CTX-002` [BLOCKER] Hard Gates.** Refactoring runs as a continuous stream without
per-step confirmation. Pause and wait for explicit written confirmation only when:

- A **Shared Kernel public type changes** (an AST node, an HIR node, a diagnostic shape).
  Every downstream Slice is affected. Present the proposed type and wait.
- A **new workspace member** is proposed. Present the Slice boundary and entry point first.
- A change would require a **new cross-slice dependency**. There is exactly one
  allowlisted exception (Section 4) and adding a second is a Hard Gate.
- A **refactoring trigger** fires (Section 11).
- A **circular dependency** or a **missing dependency** is encountered.

Silence, absence of objection, or topic continuation is not approval.

**`CTX-003` [HIGH]** Generated code is production-grade. No scaffolding, stubs, or
"implement later" placeholders unless a skeleton is explicitly requested.

---

## 3. Hard Constraints

### NEVER
| ID | Rule |
|----|------|
| `AC-001` | Create a Slice for a technical layer (`compiler/caching`, `compiler/logging`, `compiler/traversal`). Slices are named for compiler capabilities. Technical concerns belong in the Shared Kernel or inside the one Slice that needs them. |
| `AC-002` | **[BLOCKER]** Import, call, or re-export another Slice's items. Slices depend on Shared Kernel crates only. Enforced by `test_no_cross_slice_dependencies`. |
| `AC-003` | Commit commented-out code or dead code blocks. → CM-002 |
| `AC-004` | Leave `todo!()`, `unimplemented!()`, or a function returning an empty result as a placeholder. A stub Slice is worse than a missing one. → CTX-003 |
| `AC-005` | Name a crate or module `utils`, `helpers`, `common`, `misc`, `manager`, `processor`, or `base`. |
| `AC-006` | Generate backward-compatible constructs: versioned traits, deprecation attributes, wrapper shims, migration TODOs. → CTX-001 |
| `AC-007` | Write comments describing WHAT the code does. Comments explain WHY. → CM-001 |
| `AC-008` | Put logic in `lib.rs`. A Slice's `lib.rs` declares its public surface and nothing else. Internal `mod.rs` files are required by Rust and are exempt. |
| `AC-009` | Put compilation logic in a Shared Kernel crate. Shared Kernel crates define types and pure operations on them. |

### ALWAYS
| ID | Rule |
|----|------|
| `AC-010` | A Slice exposes exactly one public entry function. Its input and output types are named and owned by the Shared Kernel or by the Slice itself, never borrowed from a sibling Slice. |
| `AC-011` | Return `Result<T, E>` from the entry point. Errors are values. Panics are reserved for compiler bugs that cannot be expressed as a diagnostic. |
| `AC-012` | **[BLOCKER]** Update the affected Slice's CONTEXT.md in the same commit as any change to its entry point, public surface, or Shared Kernel dependency. → Section 12 |
| `AC-013` | Guard clauses and early returns at the top of every function. Nested conditional depth beyond 2 must be refactored before delivery. → CQ-001 |
| `AC-014` | Named constants for every magic number and magic string. → CM-004 |

---

## 4. Slice Boundaries

**Pipeline order.** Source, then lexical analysis, syntax parsing, module resolution,
semantic analysis, HIR lowering, backend codegen, link. `neurc` calls each in turn and
carries the value from one to the next. A Slice never calls the next stage itself.

**The one allowlisted exception.** `syntax-parsing` depends on `lexical-analysis`, so that
`parse()` calls `tokenize()` internally rather than making the Driver orchestrate a pair
that is never useful apart. It is named in `compiler/syntax-parsing/CONTEXT.md` and
allowlisted by name in `compiler/neurc/tests/architecture_tests.rs`. A second exception
is a Hard Gate and must pass that test's review, not merely be added to it.

**Dev-dependencies are unrestricted.** A Slice may depend on any other Slice in
`[dev-dependencies]` to drive its tests through real source. The architecture test reads
`[dependencies]` only, by design: test coupling costs nothing at runtime.

**`SB-001` [BLOCKER] Contract ownership.** A type crossing a Slice boundary lives in the
Shared Kernel. A Slice must not define a type that a sibling Slice needs to name. If two
Slices need the same type, it is a contract type and moves to `infrastructure/`; that move
is a Hard Gate because it changes every downstream consumer.

**`SB-002` [HIGH] Promotion gate.** Moving anything into the Shared Kernel requires either
(a) it is a contract type per SB-001, or (b) an explicit `/abstract [Name]` command from
the developer. Detecting duplication is not authorisation to promote. → RT-004

**`SB-003` [HIGH] Deletion test.** Deleting a Slice folder plus its workspace member entry
and its call site in the Driver must leave the workspace compiling. If deletion requires
editing another Slice, the boundary is wrong.

---

## 5. Core Principles

**Cohesion over DRY.** Duplicate logic across Slices by default. Do not build a shared
module on predicted future divergence.

*Compiler carve-out:* traversal and type reasoning over a contract type is not duplication
to be shared. Each stage interprets the shared AST or HIR for its own purpose, and those
interpretations legitimately resemble each other. Never move a type-checking rule into a
Shared Kernel crate to avoid rewriting a match arm. A shared match arm couples the stages
the architecture exists to separate.

**Fail-slow diagnostics.** A stage collects every error it can in one pass and returns them
together, so one compile shows the full error set. A stage that returns on the first error
is a bug unless the error makes further analysis meaningless.

**Single responsibility per Slice.** A Slice performs one pipeline transformation. If its
purpose needs "and" to state, split it. → RT-002

**No speculative generality.** No traits with one implementor, no extension points for
capabilities that do not exist, no generic parameter with one instantiation. Abstract at
the moment the second case appears, not before.

---

## 6. Slice Anatomy

| Component | Required | Responsibility |
|-----------|----------|----------------|
| `lib.rs` | yes | Public surface: the entry function, its error type, `pub use` of nothing else. No logic. → AC-008 |
| Entry function | yes | One per Slice. Named for the transformation (`tokenize`, `parse`, `check_program`, `lower_program`). |
| Error type | yes | One `thiserror` enum, owned by the Slice, whose variants carry a `Span`. → Section 10 |
| Internal modules | yes | `pub(crate)` by default. One module per sub-concern, not per file-size limit. |
| `tests/` | yes | Integration tests driving the entry function. → Section 13 |
| `CONTEXT.md` | yes | → Section 12 |

---

## 7. Naming

**`FN-001`** Crates are kebab-case and named for the transformation: `lexical-analysis`,
`hir-lowering`. Modules and files are snake_case. Types are PascalCase. This is idiomatic
Rust; no role suffix scheme is imposed on top of it.

**`FN-002`** Test files: `tests/<subject>_tests.rs` for integration tests;
`src/**/tests/` or `src/tests.rs` for unit tests co-located with what they cover.

**`FN-003`** Forbidden crate and module names: `utils`, `helpers`, `common`, `misc`,
`manager`, `processor`, `base`. `shared-types` is exempt by name: it is the cross-slice
contract crate, its contents are enumerated in its CONTEXT.md, and adding to it is
governed by SB-001 rather than by convenience.

---

## 8. Comment Policy

| ID | Severity | Rule |
|----|----------|------|
| `CM-001` | HIGH | Comments explain WHY, never WHAT. Bad: `// loop over the items`. Good: `// Checked before mangling because a user name carrying its own separator would collide with a generated method symbol.` |
| `CM-002` | **BLOCKER** | Dead code is deleted, never commented out. Version control is the history. Exception: the commented workspace members in the root `Cargo.toml` are a roadmap marker, not dead code. |
| `CM-003` | HIGH | A TODO states the reason and the unblocking condition. Bad: `// TODO: fix`. Good: `// TODO: lower the body here once 2C lands the Linalg path.` |
| `CM-004` | MEDIUM | Magic numbers and non-obvious strings become named constants. The WHY comment goes at the declaration, not the use site. |
| `CM-005` | MEDIUM | Doc comments that restate the item name are forbidden. Exception: a public entry point with non-obvious constraints or ordering requirements. |
| `CM-006` | LOW | Section divider comments are forbidden. A file needing dividers to navigate wants splitting. |
| `CM-007` | HIGH | A non-obvious language rule encoded in the compiler carries a WHY comment naming the rule. |
| `CM-008` | **BLOCKER** | Every `unsafe` block carries a safety rationale stating the invariant the caller must uphold. |

---

## 9. Code Quality

| ID | Severity | Rule |
|----|----------|------|
| `CQ-001` | HIGH | Guard clauses first. Happy path is the least indented path. Depth beyond 2 is refactored before delivery. |
| `CQ-002` | HIGH | No unused imports, variables, parameters, or unreachable code. `cargo clippy --workspace --all-targets -- -D warnings` is the gate. |
| `CQ-003` | MEDIUM | One nameable responsibility per function. Extract when a function exceeds 60 lines. |
| `CQ-004` | HIGH | A `bool` parameter that selects between behaviours is forbidden. Use two named functions or an enum. |
| `CQ-005` | MEDIUM | Nested `if let` / ternary-equivalent chains limited to depth 1. Prefer `match` or a lookup table. |
| `CQ-006` | MEDIUM | No trait, generic parameter, or builder for a single concrete case. → No speculative generality |
| `CQ-007` | LOW | Explicit over clever. A one-liner that needs decoding at 3am is not shorter. |
| `CQ-008` | MEDIUM | Config and manifest files carry only active keys. |

---

## 10. Diagnostics

There is no logging layer and none is to be added. A compiler's observable output is its
diagnostics.

| ID | Severity | Rule |
|----|----------|------|
| `DG-001` | **BLOCKER** | A Slice's errors are one `thiserror` enum on its own entry point, returned to the driver. A Slice must not print a user-facing diagnostic to stderr itself: rendering belongs to `neurc`, which is the only crate that knows whether it is running `check` or `compile`. |
| `DG-002` | HIGH | Every diagnostic carries a `Span` locating it in source. A diagnostic without a span is a compiler bug, not a user error. |
| `DG-003` | HIGH | Infrastructure failures (file IO, linker invocation, LLVM initialisation) are caught at the adapter boundary and surfaced as a typed error, never as a panic at a Slice boundary. |
| `DG-004` | MEDIUM | A diagnostic states what was found and what was expected. "Invalid syntax" is not a diagnostic. |

---

## 11. Refactoring Triggers

When a trigger fires: report the ID and propose a concrete fix. Hard Gate triggers pause
for confirmation before any code is written.

**`RT-001` [HIGH]** A single `.rs` file exceeds 1000 lines.
Action: identify the sub-concerns and propose a module split. Report the proposal; do not
split a file as a side effect of unrelated work.

**`RT-002` [HIGH] [Hard Gate]** A Slice's CONTEXT.md Purpose needs "and", or the Slice
grows a second public entry function.
Action: halt. Propose two named Slices with CONTEXT.md outlines for both.

**`RT-003` [HIGH]** A Shared Kernel crate gains a function containing a compilation rule
rather than a type operation.
Action: flag as a leak. Move the rule into the Slice that owns that stage. → AC-009

**`RT-004` [MEDIUM]** Identical logic appears in three or more Slices and no `/abstract`
command has been issued.
Action: surface the duplication and stop. Do not self-promote to the Shared Kernel.
`/abstract [Name]` is the only promotion gate. → SB-002

**`RT-005` [HIGH] [Hard Gate]** A Slice needs a type owned by a sibling Slice.
Action: halt. Either the type is a contract type and moves to the Shared Kernel (SB-001),
or the boundary is drawn wrong. Never add the cross-slice dependency.

---

## 12. CONTEXT.md

Every crate under `compiler/` has one, infrastructure crates included. Presence and
required sections are asserted by `test_all_slices_have_context_md`.

Update it in the **same commit** as any change to: the entry function signature, the
public surface, a Shared Kernel dependency, or the Slice's name.

```markdown
# [crate-name]

## Purpose
[One sentence. The single transformation this crate performs.]

## Entry Point
- Type: [Library function | CLI | Type definitions only]
- Input: `[type]`
- Output: `[type]`

## Shared Kernel
- [crate]: [what is used and why]
[Note any dev-dependency on a sibling Slice here, stating that it is test-only.]

## Notes
[Non-obvious design decisions, ordering constraints, known limitations. Omit if none.]
```

---

## 13. Testing

| ID | Severity | Rule |
|----|----------|------|
| `TS-001` | **BLOCKER** | Every behaviour change ships with tests in the same commit. Tests are not opt-in and are not deferred. |
| `TS-002` | HIGH | Each Slice has integration tests driving its entry function over real input, asserting on the produced value, not on internals. |
| `TS-003` | HIGH | Every fixed bug gets a regression test naming the bug in the test name. |
| `TS-004` | HIGH | Error paths are tested. A Slice that only has happy-path tests is untested, because a compiler's job is largely rejection. |
| `TS-005` | MEDIUM | A test asserting on a diagnostic asserts on its kind and span, not on its rendered message text. |
| `TS-006` | LOW | Never write a test count into prose. The CI badge is the only honest source, and `tools/check_docs_hygiene.py` fails the build on a written-down total. |

---

## 14. Enforcement

These rules are machine-checked. A rule with a test is not a guideline.

| Check | Enforces |
|-------|----------|
| `compiler/neurc/tests/architecture_tests.rs` | AC-002 (no cross-slice deps, one allowlisted exception), AC-009 (infrastructure depends on no Slice), AC-012 (CONTEXT.md presence and sections) |
| `cargo clippy --workspace --all-targets -- -D warnings` | CQ-002, and most of CQ-001 and CQ-005 |
| `cargo fmt --all` | Formatting. Not negotiable and not discussed in review. |
| `tools/check_docs_hygiene.py` | TS-006, plus the ban on written-down versions and private paths in tracked files |
| `compiler/lexical-analysis/tests/tmlanguage_sync.rs` | The editor grammar tracks the lexer's token set |

Everything else in this file is enforced by the agent reading it. That is the reason it is
short: a rule nobody can check and nobody rereads is not a rule.
