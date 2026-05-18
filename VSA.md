# Vertical Slice Architecture — Solo-Agent AI Base Guidelines v4.4

> **Priority: Highest.** Every rule is a compiler error. Blocker-severity violations MUST be refused — state the rule ID and propose a compliant alternative.
> **Context:** 1 human + 1 AI agent | 20–50K LOC | No backward compatibility.
> Supersedes: v4.3 | Created: 2026-05-16

---

## 1. Terminology

| Term | Definition |
|------|-----------|
| **Slice** | One business capability owning its own entry point, logic, and data. Zero runtime dependency on other Slices. |
| **Feature Folder** | Physical directory for a Slice. ALL files for that capability live here. No exceptions. |
| **Shared Kernel** | Infrastructure adapters and pure domain utilities. ZERO business logic permitted. |
| **Integration Event** | Immutable fact emitted by a Slice after a state change. Consumed asynchronously. Describes what happened, not what to do next. |
| **Public Read Model** | Read-only DTO a Slice exposes explicitly for cross-slice synchronous reads. A contract, not a model leak. |
| **Domain Utility** | Pure function or value type with zero side effects. Shared Kernel eligible only when it contains no slice-specific rules. |
| **CONTEXT.md** | Machine-readable contract file at each Slice root. The AI's cross-session memory for that Slice. Not documentation. |
| **Hard Gate** | A point in autonomous execution where the AI MUST pause for explicit written confirmation regardless of `CTX-002`. |

---

## 2. Project Context

**`CTX-001` [BLOCKER]** No migration paths, compatibility shims, versioned endpoints, deprecated code paths, or adapter layers. If a better design exists, refactor completely.

**`CTX-002` [ULTRA] — Continuous Execution Protocol**
To minimize token overhead and latency, refactoring tasks MUST be executed as a continuous, autonomous stream. The AI is authorized to complete the entire scope without pausing for developer confirmation unless a Hard Gate or terminal error is encountered.

1. **Initial Assessment (The Single Gate):** Output a high-level execution plan and a list of files to be modified. Immediately follow this with: *"Starting continuous execution. I will pause only at Hard Gates, terminal errors, or if $N$ files are modified without success."*
2. **Autonomous Chaining:** Execute all changes sequentially. Each response starts with a progress indicator (e.g., `[Step 3/8]`) and ends with an internal logic check before proceeding to the next file.
3. **Hard Gates — mandatory pause regardless of `CTX-002`:**
   - A new or modified **persistence schema** is required. Output the proposed schema and wait for explicit written confirmation. Silence, absence of objection, or topic continuation is NOT approval.
   - A **`RT-001` trigger** fires (Handler exceeds 300 lines or 5 injected dependencies). Halt, present the split plan, wait for confirmation.
   - The proposed change **contradicts an established pattern** already present in the codebase.
   - A **missing dependency** or **circular logic** is encountered.
4. **Final Reconciliation:** Upon completion, provide a concise summary of all changes and a verification command (e.g., a test suite run or `git diff` summary).

Leaving old and new patterns coexisting after any single step is **FORBIDDEN**.

**`CTX-003` [HIGH]** No team-onboarding docs, contribution guides, or changelogs unless explicitly requested.

**`CTX-004` [HIGH]** Generated code must be production-grade. No scaffolding, stubs, or "implement later" placeholders unless the developer explicitly requests a skeleton.

---

## 3. AI Hard Constraints

### NEVER
| ID | Rule |
|----|------|
| `AC-001` | Create Technical Slices (e.g., Slices/Authorization, Slices/EmailSender, Slices/Cache). These are Shared Kernel concerns. |
| `AC-002` | Allow any Slice to import, instantiate, or call the internal classes, handlers, repositories, or entities of another Slice. Only Public Read Models and Integration Events cross slice boundaries. |
| `AC-003` | Generate commented-out code or dead code blocks. → CM-002 |
| `AC-004` | Leave stub implementations unfinished (`NotImplementedException`, `pass`, `todo!()`) unless the developer explicitly requests a skeleton. → CTX-004 |
| `AC-005` | Use generic file names: `utils`, `helpers`, `common`, `misc`, `shared`, `base`, `manager`, `processor`, or `handler` without a Feature prefix. |
| `AC-006` | Generate backward-compatible constructs: versioned interfaces, deprecated annotations, adapter wrappers, or migration-comment TODOs. → CTX-001 |
| `AC-007` | Write comments that describe WHAT the code does. Comments explain WHY only. → CM-001 |
| `AC-008` | Generate internal barrel/index files inside a Slice. Barrel files are permitted only at the Slice root to define its public export surface. |

### ALWAYS
| ID | Rule |
|----|------|
| `AC-009` | Require explicit, named Input and Output DTOs at every Slice entry point. Never return an internal domain model directly. **Exception:** at Small scale (1–14 Slices), Output DTO may be omitted when the DB entity shape exactly matches the required response and no transformation is needed. |
| `AC-010` | Validate input at the Slice boundary before it reaches the Handler. → EP-VALIDATION |
| `AC-011` | Update the affected Slice's CONTEXT.md in the same response as any interface, schema, or event change. Never let CONTEXT.md fall out of sync. → Section 12 |
| `AC-012` | Apply guard clauses and early returns at the top of every function. Nested conditional depth beyond 2 MUST be refactored before delivery. → CQ-001 |
| `AC-013` | Use named constants for every magic number, magic string, and configuration value. → CM-004 |

---

## 4. AI Interaction Protocol

**Step 1 — Complexity Check**
If fewer than 5 distinct business operations AND clearly a prototype or utility script, warn that VSA adds structural overhead. Proceed only on developer confirmation or if the project is already established as VSA.

**Step 2 — Boundary Proposal**
Before writing implementation code, output:
- (a) Slice name and its single business responsibility.
- (b) Full file/folder tree.
- (c) Input DTO, Output DTO, and Handler signature.
- (d) Persistence schema if applicable — triggers a **Hard Gate** (→ CTX-002.3).

Infer from context first. Ask ONE clarifying question only if a boundary is genuinely ambiguous after inference.

**Step 3 — Completeness Check**
After generating a Slice, verify all mandatory components exist: Entry Point, Input Model, Validator, Handler, CONTEXT.md. Generate any missing component before ending the response.

**Step 4 — Consistency Scan**
After any change, report:
- Other Slices affected by the interface or schema change.
- Stale CONTEXT.md files referencing the modified surface.
- Shared Kernel utilities that may have absorbed business logic.

Do not silently leave inconsistencies.

---

## 5. Core Principles

### Cohesion over DRY
Always duplicate business logic across Slices. Do NOT create a shared module based on predicted future divergence — LLMs cannot reliably predict domain evolution. Business logic moves to the Shared Kernel as a Domain Utility ONLY on explicit developer command: `/abstract [LogicName]`. Without that command, **duplication is the default and correct action.**

**`PURE-FN-EXCEPTION`:** Pure functions with zero side effects representing a universal domain concept (e.g., Money arithmetic, VAT calculation, slug generation) are Shared Kernel eligible from initial creation WITHOUT requiring `/abstract`, provided: (a) zero side effects, (b) no branch logic tied to any specific Slice's business rules, (c) genuinely universal across any Slice.

### Abstract Command Integrity
If the AI detects identical business logic duplicated across three or more Slices and no `/abstract` command has been issued, it MUST proactively flag `RT-005` and surface the duplication — it MUST NOT silently proceed or self-promote the logic to the Shared Kernel. The `/abstract` command is the only valid promotion gate.

### Slice Independence
Deleting a Slice folder plus its DI registration and event subscriptions must leave the project in a compilable, fully functional state. If deletion requires modifying code inside another Slice, the boundaries are wrong.

### Transactional Integrity
One Slice = One Transaction. Never coordinate DB transactions across Slice boundaries. Multi-slice state changes use the Saga pattern via Integration Events.

### Error Handling
Slices MUST NOT use exceptions for business flow control. Return a Result/Either type from Handler to Entry Point. Exceptions are reserved for unexpected infrastructure failures only.

### Single Responsibility per Slice
A Slice performs exactly one primary business action. If the Slice name requires the word "and," split it. The business action must be expressible in one imperative sentence without conjunctions.

### No Speculative Generality
Do not generate abstractions, interfaces, or extension points for capabilities that do not yet exist. Build for the current requirement. Introduce extensibility at the moment it is needed.

---

## 6. Slice Anatomy

### Mandatory Components
| Role | Responsibility |
|------|---------------|
| **Entry Point** | Routes, HTTP binding, Entry Point → Handler call, Result → protocol response. Zero business logic. |
| **Input Model** | Immutable DTO/Command/Query. Named `[Feature]Command` or `[Feature]Query`. |
| **Validator** | Enforces all invariants checkable without a DB call. DB checks (uniqueness, existence) belong in the Handler. |
| **Handler** | Single class. Orchestrates domain logic, persistence, event publishing. The only business logic location. |
| **CONTEXT.md** | Machine-readable contract. Updated on every interface or schema change. |

### Optional Components
| Role | When Required |
|------|--------------|
| **Domain Model** | Only when the Slice has complex encapsulated invariants. Omit for simple CRUD. |
| **Repository** | When the Handler has more than one DB operation or a non-trivial query. Small scale: inline persistence in Handler is permitted. |
| **Output Model** | Named `[Feature]Response`. Required when response shape differs from the DB entity. At Small scale, may be omitted when shapes match exactly. |
| **Projection** | Required only for cross-slice synchronous reads. |

---

## 7. File Naming Conventions

**`FN-001`** Pattern: `[FeatureName].[Role].[ext]`
- `FeatureName` = PascalCase business capability (`PlaceOrder`, `RegisterUser`, `GenerateInvoice`)
- `Role` drawn exclusively from the Role Vocabulary below

### Role Vocabulary
| Role | Purpose |
|------|---------|
| `Handler` | The UseCase/Handler class. One per Slice. |
| `Command` | A mutating Input DTO. |
| `Query` | A read-only Input DTO. |
| `Validator` | Input validation logic. |
| `Repository` | Data access abstraction. |
| `Response` | Output DTO returned from the entry point. |
| `Endpoint` | Entry point: controller, route, or consumer. |
| `Model` | Domain or persistence model internal to the Slice. |
| `Event` | Integration event emitted by the Slice. |
| `Projection` | Public Read Model for cross-slice reads. |
| `DomainService` | Complex domain logic with zero infrastructure dependencies. |

**`FN-002`** Test files: `[FeatureName].[Role].Test.[ext]` — co-located in `/tests` within the Feature Folder.
Example: `PlaceOrder.Handler.Test.ts`

**`FN-003`** Shared Kernel files: `[ConceptName].[Role].[ext]`
ConceptName is a domain concept, never a technical descriptor.
Examples: `Money.ValueObject.ts` | `Logger.Adapter.ts` | `DatabaseConnection.Config.ts`

**Forbidden names:** `utils` | `helpers` | `common` | `misc` | `shared` | `manager` | `processor` | `base` (without feature prefix) | `index` (except Slice public barrel)

---

## 8. Comment Policy

| ID | Severity | Rule |
|----|----------|------|
| `CM-001` | HIGH | Comments explain WHY, never WHAT. ❌ `// Loop through orders and sum the totals` ✅ `// Summed here rather than in the DB query because tax calculation requires hydrated domain objects.` |
| `CM-002` | **BLOCKER** | Dead code MUST be deleted, never commented out. Use version control for history. |
| `CM-003` | HIGH | TODO comments MUST include reason and context. ❌ `// TODO: fix this` ✅ `// TODO: Replace with event-driven approach once the Notification slice is implemented.` |
| `CM-004` | MEDIUM | Replace magic numbers and non-obvious strings with named constants. If the constant name is not self-explanatory, add a WHY comment at the declaration — not at the usage site. ❌ `const timeout = 30000; // 30 second timeout` ✅ `const SESSION_EXPIRY_MS = 30_000; // Matches the upstream auth provider's token TTL.` |
| `CM-005` | MEDIUM | Auto-generated doc comments that restate the function or parameter name are FORBIDDEN. Exception: public API surface methods with non-obvious behavior or constraints. |
| `CM-006` | LOW | Section divider comments (`// ===== INIT =====`) are FORBIDDEN. A file requiring dividers to be navigable violates single responsibility — split it. |
| `CM-007` | HIGH | Non-obvious business rules embedded in code MUST have a WHY comment explaining the business constraint. Example: `// Orders under €10 ineligible per carrier contract FUL-2024-03.` |

---

## 9. Code Quality Constraints

| ID | Severity | Rule |
|----|----------|------|
| `CQ-001` | HIGH | Guard clauses and early returns at the top of every function. Happy path = least indented path. Depth > 2 MUST be refactored before delivery. |
| `CQ-002` | HIGH | No unused imports, variables, parameters, or unreachable code in any generated output. |
| `CQ-003` | MEDIUM | Single nameable responsibility per function. Prefer extraction when a method exceeds 60 lines. |
| `CQ-004` | HIGH | Boolean parameters that alter execution path are FORBIDDEN. Use separate explicitly named functions. ❌ `processOrder(order, isDraft: true)` ✅ `saveDraftOrder(order)` / `submitOrder(order)` |
| `CQ-005` | MEDIUM | Nested ternaries limited to depth 1. Complex conditional assignment uses if/else or a lookup structure. |
| `CQ-006` | MEDIUM | No generic abstractions, base classes, or factory patterns for a single concrete implementation. Abstract only when two or more concrete implementations exist. |
| `CQ-007` | LOW | Prefer explicit over implicit. Avoid clever one-liners that sacrifice readability for brevity. |
| `CQ-008` | MEDIUM | Config files include only actively used keys. No commented-out configuration options as reference. |

---

## 10. Data Sovereignty

| ID | Severity | Rule |
|----|----------|------|
| `DS-001` | **BLOCKER** | A Slice MUST own its tables/collections exclusively. Only the owning Slice may write to them. |
| `DS-002` | **BLOCKER** | No SQL JOINs, subqueries, or cross-collection aggregations between tables owned by different Slices. |
| `DS-003` | CRITICAL | Cross-slice data sync: (a) Mutations: async via Integration Events only. (b) Reads: sync via an explicitly defined Public Read Model only. Never reference the owning Slice's internal DTO, entity, or repository. |
| `DS-004` | HIGH | Shared database table naming: `[slicename]_[tablename]` (lowercase snake_case). Example: `ordering_lines`, `catalog_products`. |
| `DS-005` | HIGH | Integration Event schemas are immutable once published. Schema changes require a new event type. This is the only permitted deprecation scenario in this ruleset. |

---

## 11. Observability

Observability infrastructure lives exclusively in the Shared Kernel as zero-business-logic adapters. Slices consume these adapters via dependency injection and MUST NOT implement logging, tracing, or error formatting inline.

| ID | Severity | Rule |
|----|----------|------|
| `OB-001` | HIGH | All Slices MUST emit structured log entries at Handler entry and exit. Minimum fields: `slice`, `operation`, `duration_ms`, `result` (`ok` or `error`). |
| `OB-002` | HIGH | All error responses MUST conform to a single project-wide error envelope defined in the Shared Kernel. Slices MUST NOT define their own error shapes. |
| `OB-003` | MEDIUM | Correlation/trace IDs MUST be propagated from the Entry Point through the Handler to any Integration Event emitted. The propagation mechanism is defined once in the Shared Kernel. |
| `OB-004` | LOW | Infrastructure exceptions (DB, network, external service) MUST be caught at the adapter boundary, logged with full context, and re-surfaced as a typed infrastructure error — never as a raw exception at the Slice boundary. |

---

## 12. Refactoring Triggers

When any trigger fires during **autonomous execution**: report the trigger ID, propose a concrete fix. If the trigger is marked **Hard Gate**, pause and wait for explicit written confirmation before continuing.

**`RT-001` [BLOCKER] [Hard Gate]**
Condition: Handler/UseCase exceeds 300 lines OR has more than 5 injected dependencies.
Action: Halt. Identify sub-operations. Propose a split into separate named Commands. Present the plan before writing code.

**`RT-002` [HIGH]**
Condition: CONTEXT.md Purpose requires "and" to describe the Slice's responsibility, or describes two distinct outcomes.
Action: Propose splitting into two named Slices. Present CONTEXT.md outlines for both before writing code.

**`RT-003` [HIGH]**
Condition: A Shared Kernel utility contains conditional logic tied to a specific business scenario.
Action: Flag as God Utility. Move the business branch into the relevant Slice. The Shared Kernel function must remain pure.

**`RT-004` [HIGH]**
Condition: A Slice imports more than one other Slice's Public Read Model OR subscribes to more than three Integration Events.
Action: Flag boundary misalignment. Propose Read Model replication or a boundary redraw.

**`RT-005` [MEDIUM]**
Condition: Identical validation logic duplicated across three or more Slices AND no `/abstract` command has been issued.
Action: Flag the duplication. Propose moving it to a Shared Kernel Domain Utility with a WHY comment. Do NOT proceed with abstraction until the developer explicitly issues `/abstract [ValidatorName]`. Confirmation alone is insufficient.

---

## 13. Living Context Documentation (CONTEXT.md)

Every Slice MUST have a CONTEXT.md. It is a mandatory Slice component.

Update CONTEXT.md in the **SAME response** as any change to: Input/Output DTO | DB table or collection | Published or consumed event | Slice name, Handler name, or Entry Point | Shared Kernel dependency.

### Template

```markdown
# [SliceName]

## Purpose
[One sentence. The single business action this slice performs.]

## Entry Point
- Type: [HTTP POST | HTTP GET | Event Consumer | CLI | ...]
- Input: `[CommandName | QueryName]`
- Output: `[ResponseName | void]`

## Data Ownership
- Tables: `[slicename_tablename]`
- Events Published: `[EventName]`   ← omit if none
- Events Consumed: `[EventName]`    ← omit if none
- Public Read Model: `[ProjectionName]` ← omit if none

## Shared Kernel
- [UtilityName] — [why]   ← omit section if none

## Notes
[Non-obvious design decisions or constraints only. Omit for straightforward slices.]
```

---

## 14. Context Scaling

Slices within the same project may mature at different rates. Apply the scale tier to **each Slice individually** based on its own complexity, not the total project Slice count alone. When the project crosses a tier boundary, existing Slices are upgraded incrementally — not all at once — and the AI flags which Slices are below the new tier's requirements during the next Consistency Scan (→ Step 4).

### Small (1–14 Slices)
- DB entities may be used directly in Handlers. No Repository required.
- Single shared DbContext or DB connection permitted.
- In-memory event bus permitted.
- Output Model may be omitted when the DB entity shape exactly matches the required response and no transformation is needed. → AC-009
- All AC, DS, FN, CM, CQ rules apply without exception.
- Input validation is always mandatory.

### Medium (15–40 Slices)
- Strict DTO separation: Input Model, Output Model, and DB Entity are distinct types.
- Repository or Query Object required for all non-trivial persistence.
- Slice-specific DbContext or namespace-isolated collection.
- Persistent event bus required (in-process with durability guarantees or external broker).
- Full CONTEXT.md template required.

### Large (41+ Slices)
All Medium requirements, plus:
- Dedicated DbContext per Slice with no shared entity registration.
- Outbox Pattern for all Integration Events.
- Explicit Projection types for all cross-slice reads.
- Architecture test suite enforcing no-cross-slice-reference rule. Generated once per project setup, not per Slice.

---

## 15. Conflict Resolution

**Shared Business Logic**
Condition: Business logic used in two or more Slices.
Action: Duplicate by default. Move to Shared Kernel ONLY on explicit `/abstract [LogicName]` command. Exception: `PURE-FN-EXCEPTION` (see Section 5).

**Shared Infrastructure Logic**
Condition: Purely technical logic — logging, email transport, DB connection, HTTP client.
Action: Move to Shared Kernel. Implementation must be a generic adapter with zero awareness of any specific Slice.

**Cross-Slice Read**
Condition: Slice A needs data owned by Slice B to compose a response.
Action:
- Option 1 (infrequent reads): Gateway Aggregator at the entry point layer.
- Option 2 (frequent reads): Slice A maintains a Read Model replica kept in sync via Integration Events.
- FORBIDDEN: calling Slice B's repository, handler, or entity directly.

**Ambiguous Slice Ownership**
Condition: A new feature could belong to two existing Slices.
Action: Identify which existing Slice's single responsibility would change. That Slice is the owner. If both would change, create a new Slice.

---

## 16. Testing

| ID | Severity | Rule |
|----|----------|------|
| `TS-001` | HIGH | Generate tests ONLY when the developer appends `--test` to the prompt. Without `--test`, generate no test files. |
| `TS-002` | HIGH | When `--test` is present, generate one Integration Test covering the full Slice from entry point through Handler to the real or in-memory database. Must assert on actual system state after execution (record persisted, event published) — not only the return value. File: `[FeatureName].Integration.Test.[ext]`, co-located in `/tests` within the Feature Folder. |
| `TS-003` | MEDIUM | Exception: if the Slice Handler contains complex branching or non-trivial domain calculations, also generate a targeted Unit Test for that logic in isolation. Mock only infrastructure boundaries (repository, event bus, external services). Do NOT mock domain services or value objects. File: `[FeatureName].Handler.Test.[ext]` |

---

## 17. Extension Points

Child configs MUST inherit this file and implement all mandatory points. Child configs MUST NOT contradict any rule above.

| ID | Mandatory | Contract |
|----|-----------|---------|
| `EP-PROJECT-STRUCTURE` | ✅ | Concrete folder layout implementing `[Feature]/[Feature].[Role].[ext]`. Full Feature Folder template with all mandatory components. |
| `EP-FILE-NAMING` | ✅ | Language casing rules (PascalCase / snake_case / kebab-case). Role Vocabulary mapped to idiomatic language names. |
| `EP-ENDPOINT` | ✅ | Concrete Entry Point syntax: Minimal API / Controller / Route Handler / gRPC / GraphQL / CLI / Message Consumer. |
| `EP-HANDLER` | ✅ | Concrete Handler pattern with Result/Either return type implementation. |
| `EP-VALIDATION` | ✅ | Concrete validation wired at Slice boundary before Handler. Must show pipeline integration. |
| `EP-RESULT-TYPE` | ✅ | Concrete Result/Either implementation replacing exceptions for all business outcomes. |
| `EP-PERSISTENCE` | ✅ | Persistence pattern per scale: Small (inline) / Medium (Repository) / Large (Slice DbContext). Table naming convention implementation. |
| `EP-DI-REGISTRATION` | ✅ | Slice self-registration supporting delete-folder → remove-registration compile test. |
| `EP-TESTING` | ✅ | Integration test setup (real/in-memory DB). Confirm test file co-location in Feature Folder. |
| `EP-OBSERVABILITY` | ✅ | Shared Kernel adapters for structured logging and error envelope. Correlation ID propagation strategy. |
| `EP-EVENT-BUS` | ✅ at Medium+, ❌ at Small | Event bus per scale: Small (in-memory) / Medium (in-process persistent) / Large (Outbox + external broker). |
| `EP-ARCH-TESTS` | ✅ at Large, ❌ below | Automated no-cross-slice-reference enforcement. Mandatory at Large scale (41+ Slices). |
| `EP-CONTEXT-TEMPLATE` | ❌ | Language-specific CONTEXT.md additions (e.g., migration file names, package references). Must not contradict base template. |
