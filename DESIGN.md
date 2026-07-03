# Neuro Language Design

This document captures the core design principles, non-goals, and AI-first rationale behind Neuro. It is intended for contributors, language designers, and technically curious users — people who want to understand the *why*, not just the *what*.

---

## Language Design Principles

### 1. Compile-time correctness over runtime discovery

Neuro's central premise is that bugs caught at compile time are cheaper than bugs caught during a GPU training run that took six hours to reach the failure. Every design decision that moves an error earlier in the pipeline is a win.

This motivates:
- Static typing with full type inference — you get type safety without the ceremony.
- Mandatory explicit mutability (`val` / `mut`) — mutation is visible at the declaration site, not buried in an assignment.
- Tensor shape types planned as compile-time parameters — shape mismatches rejected before a single weight is allocated.

### 2. Explicitness where ambiguity costs

Neuro is not terse at the expense of clarity. Implicit numeric type coercions are rejected. Casting between numeric types requires an explicit `as`. Mutable borrowing requires an explicit `mut`. These rules feel strict at first and save debugging time later.

Inference is permitted where it is unambiguous and adds no semantic risk: local variable types, return types of single-expression functions, literal types in context. Inference is not permitted where it could silently produce a wrong type.

### 3. Ownership without a GC

Neuro will eventually adopt an ownership and borrow-checker model similar to Rust's. Garbage collection is not on the roadmap. This is a deliberate trade-off:

- AI workloads are memory-intensive and latency-sensitive. GC pauses interact badly with tight training loops and large tensor allocations.
- A GC would make it harder to reason about memory layout, which matters for MLIR lowering and GPU memory management.
- Ownership makes memory behavior auditable at the type level — critical for a language that is supposed to give you confidence in what your model is doing.

The ownership system is being layered on progressively as the type system matures: move-by-default, borrows (`&T` / `&mut T`), and deterministic `Drop` (scope-exit destructors) have landed. The remaining alpha gap is broader heap support — until the growable-string builder and owning collections land, `+` string concatenation still leaks its heap buffer (see README).

### 4. Zero-cost abstractions

Structs, methods, and (eventually) generics should produce the same code as hand-written equivalents. Neuro does not pay for abstraction at runtime. There is no virtual dispatch by default, no boxing, no reference counting unless explicitly opted into.

### 5. Native code, always

Neuro compiles directly to native binaries via LLVM. There is no interpreter, no JIT, and no planned REPL for the production compiler. The debugging experience is improved through better diagnostics and a `check` mode (type-check without codegen), not through a slower execution model.

### 6. Errors as values

Neuro will use `Result<T, E>` and `Option<T>` for error handling — not exceptions. Exceptions make control flow non-local and difficult to reason about in a system without a GC. `?` propagation and pattern matching provide ergonomic error handling without hidden jumps.

---

## Non-Goals

### Not a general-purpose systems language

Rust exists and does this extremely well. Neuro's differentiation is in the AI domain. Neuro is not trying to compete with Rust for operating system kernels, network servers, or embedded systems.

### Not a scripting language

Neuro does not have an interpreter and does not plan to have one. It is not suitable for one-off scripts, shell glue, or rapid iteration at the REPL. Python remains better for those workflows. Neuro targets the domain where Python's performance and type system are the bottleneck.

### Not Python-compatible

Neuro does not aim to run Python code or embed a Python interpreter. DLPack interoperability (Phase 6) will allow tensor data to be exchanged with Python-based ML frameworks at runtime, but Neuro code and Python code are distinct programs.

### Not a research language for type theory

Neuro's type system is designed to be learnable and practical, not to explore the boundaries of dependent types, linear types, or algebraic effects. If a type system feature does not have a clear user benefit in the AI domain, it is not added. Complexity is a cost.

### No runtime reflection

There is no `reflect` package, no `typeof`, and no dynamic dispatch on arbitrary types. Neuro's compilation model depends on knowing all types at compile time. Runtime type introspection is fundamentally at odds with static shape verification and ahead-of-time MLIR lowering.

### No implicit numeric coercions

`i32` does not silently become `f64`. `u8` does not silently become `i32`. Every numeric type change requires an explicit `as` cast. This is verbose in small examples and prevents silent precision loss or overflow in real programs.

### No backwards compatibility promise during alpha

Until Neuro reaches v1.0 (Phase 2 complete), syntax and semantics may change between minor versions. Contributors should not build critical infrastructure on pre-1.0 Neuro. The CHANGELOG documents all breaking changes.

---

## AI-First Design Rationale

### Why not Python?

Python is the dominant language for AI today. It has an enormous ecosystem, excellent tooling, and a huge community. Neuro is not competing with Python on these dimensions — it is addressing Python's structural limitations as a performance language:

- **Dynamic typing** — shape errors in tensor operations surface at runtime, often deep into a training run. A `(784, 128)` weight matrix multiplied by a `(128, 784)` input does not fail until the operation executes.
- **Interpreted execution** — the CPython interpreter adds overhead in tight loops. Frameworks like NumPy and PyTorch work around this by dropping into C extensions, but the Python layer between operations still costs.
- **The GIL** — CPython's global interpreter lock makes true parallelism on the CPU difficult without subprocess gymnastics.
- **Memory opacity** — Python abstracts memory layout. Writing a CUDA kernel that operates on a Python object requires navigating framework-specific buffer protocols.

Neuro addresses these at the language level: types are static, tensor shapes are compile-time, execution is native, and memory layout is explicit.

### Why LLVM 20 and MLIR?

LLVM is the industry standard for optimizing native code generation. The inkwell bindings give Neuro a mature, well-tested code generation foundation without reinventing register allocation, instruction selection, or platform ABI handling.

MLIR (Multi-Level Intermediate Representation) is how Phase 3+ tensor operations will be lowered. MLIR's type system natively represents tensor shapes as type parameters. The `linalg`, `tensor`, and `arith` dialects provide a high-level representation that MLIR can lower to both CPU vector code and GPU kernels using the `nvgpu`, `rocdl`, and Triton dialects. This means Neuro's compiler can target CPU, NVIDIA GPU, and AMD GPU without maintaining separate backends.

### Why automatic differentiation via Enzyme?

Frameworks like PyTorch use operator-overloading AD: every tensor operation records itself into a computation graph at runtime, which is then traversed in reverse. This has high overhead and requires the framework to know about every operation.

Enzyme is an LLVM/MLIR pass that differentiates native code at the IR level. It does not need to know about "tensor operations" specifically — it differentiates the LLVM IR produced by the compiler. This means:

- Differentiation is transparent to the language — `@grad(f)` is a compiler annotation, not a runtime mode switch.
- Custom operations written in Neuro (not from a library) are automatically differentiable.
- The differentiated code is optimized by the same LLVM passes as the forward pass.

### Why tensor shapes as type parameters?

A `Tensor<f32, [784, 128]>` has its shape encoded in its type. This means:

- Matrix multiply `[A, B] × [B, C] → [A, C]` is type-checked: passing `[784, 128]` and `[256, 10]` is a compile error.
- Batch dimension mismatches between model layers are caught before the model runs.
- The optimizer can use shape information to select loop tile sizes and memory access patterns.

This requires MLIR (Phase 3) because the LLVM IR type system does not natively represent n-dimensional arrays with static shape constraints. MLIR's parametric type system does.

---

## Design Influences

| Language | What Neuro borrows |
|---|---|
| **Rust** | Ownership model, `Result`/`Option`, `pub(crate)` visibility, no GC |
| **Swift** | Ergonomic syntax, implicit return, struct/method design |
| **Python** | Whitespace-tolerant style, AI ecosystem mindset |
| **Mojo** | AI-first language goals, MLIR lowering strategy |
| **Zig** | Explicit allocators, no hidden control flow |

Neuro does not copy any of these languages. It borrows *ideas* where they solve real problems in the AI domain and discards the rest.
