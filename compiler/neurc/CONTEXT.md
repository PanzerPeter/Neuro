# neurc

## Purpose
Orchestrate the full Neuro compiler pipeline and expose it as a CLI tool.

## Entry Point
- Type: CLI
- Input: `neurc check <file.nr>` | `neurc compile <file.nr> [-O<0-3>] [-o <output>]`
- Output: an executable binary on success; diagnostics and non-fatal lint warnings to stderr

## Data Ownership
- Tables / Events Published / Events Consumed / Public Read Model: none

## Shared Kernel
- diagnostics — pipeline error formatting
- ast-types — the parsed item list handed between the resolution, binding, and checking steps
- neuro-hir — the lowered program handed to the backend

## Notes
neurc is the only crate permitted to depend on every feature slice. It holds **no business
logic of its own**: every decision is delegated to the owning slice. What it owns is the order.

### Pipeline order, and why it is that order
Both `check_file` and `compile_file` run the same front half, so neither can skip a step:

1. `load_program` — module resolution plus the prelude. It hands `syntax_parsing::parse` to
   `module_resolution::resolve_program` and gets back the merged item list of every module the
   root reaches through a qualified path. The parser is **passed in rather than depended on**
   because `module-resolution` may not import a feature slice; neurc is the single place the
   two meet.
2. `argument_binding::bind_arguments` — after the merge and the prelude, before type checking.
   A call names a callee that may be declared in any file, so the table cannot be built until
   every module is merged; and the arguments must already sit in declaration order when the
   type checker pairs them with parameter types.
3. `semantic_analysis::type_check`.
4. `hir_lowering::lower_program` — the typed HIR. `check` reports the lowered item count;
   `compile` hands the HIR to `llvm_backend::compile`, which lowers native object code from it
   (the backend does not consume the AST).

`compile_file` then checks the lowered HIR for a function named `main` **before** writing an
object file. Without that check the pipeline ran to completion and handed a `main`-less object
to the system linker, so the user saw `undefined reference to 'main'` naming the C runtime's
startup object rather than their own program. `check` is unaffected — type-checking a module
with no `main` is legitimate.

### The prelude
`prelude::load()` parses `prelude.nr` once into a `Prelude` value that answers two questions:
`variants()` — every variant of every enum it declares, handed to
`module_resolution::resolve_program` so each module may write `Some` / `Ok` bare — and
`prepend()`, the declarations themselves. Reading the variant list **off the parsed prelude** is
what keeps `prelude.nr` the single place the prelude's contents are stated; module resolution is
*told* them for the same reason it is handed the parser.

A prelude item whose name the program already declares is dropped, so a local declaration
shadows it. The items are otherwise ordinary declarations: nothing downstream special-cases
`Option` or `Result`.

Dropping one item takes with it every prelude declaration written against it. `Chars::next`
returns `Option<char>`, so a program declaring its own `Option` would leave the prelude's own
body compiled against a type that is no longer there — `PRELUDE_DEPENDENCIES` records that
edge (`Chars` needs `Option` and `Iterator`), and `dropped_declarations` closes over it.
`is_dropped` also drops an `impl` block extending a displaced type: those methods belong to the
prelude's type, not to whatever the program put in its place.

`prepend` stamps every prelude declaration with `PRELUDE_MODULE` (`ModuleId::MAX`), an id no
loaded file can hold. The prelude is prepended *after* module resolution has numbered the
program's files, so leaving it at 0 would make its internals private to whichever file happens
to be the root rather than to the prelude itself. `prelude.nr` marks its own surface with
`export` for the same reason it now has to.

`load_program` also decides whether to prepend at all: `ResolvedProgram.no_prelude` reports the
root file's `@no_prelude`, and the merged namespace is flat, so the prelude's declarations are
either in the whole program or absent from all of it.

`prelude.nr` currently declares `Option<T>`, `Result<T, E>`, the `OrderedF32` / `OrderedF64`
validating wrappers, the `Iterator` / `IntoIterator` protocol traits, and `Chars`, the codepoint
iterator `string.chars()` hands out. The wrappers exist so
an ordered map can be keyed on a float: IEEE-754 `<` is a partial order, so a raw float key
could be inserted and never found again — hence `@derive(Copy, Clone)`, a `new` constructor
that panics on NaN, and `PartialEq` + `Comparable` impls. They deliberately do **not**
implement `Hashable`.

The two protocol traits are what `for` desugars against: `Iterator` declares
`type Item` and `next(&mut self) -> Option<Self::Item>`; `IntoIterator` declares `type Item`,
`type Iter`, and `into_iter(self) -> Self::Iter`. They are ordinary trait declarations —
nothing in the checker or the lowerer treats them as lang items, and a program declaring its
own `Iterator` shadows them like any other prelude name. `type Iter` carries no `: Iterator`
bound because an associated-type *declaration* has no bound syntax yet; the requirement is
enforced where the loop is built, on the type `into_iter` actually returns.

`Chars` holds a `&string` view and a `u64` byte cursor, and its `impl Iterator` is written in
ordinary Neuro — the one thing it cannot say in source is the decode itself, which it takes from
the prelude-private `__char_at(offset)` intrinsic (`in_prelude()` in the type checker gates it on
`PRELUDE_MODULE`, the constant `ast_types` now owns so both this crate and the checker read the
same one). The step width follows from the decoded scalar's own magnitude, so a step reads the
text once. `.chars()` fills the fields in: the lowering builds the struct literal, and the fields
stay private so nothing else can.

### Remaining pipeline facts
Lint warnings from `type_check` are forwarded to stderr by `print_warnings` in both entry
points. Warnings never cause a non-zero exit — they are informational and may be silenced with
`@allow(...)` on the enclosing function.

The backend is handed the *root* file's source for panic-location rendering. Merged modules
share one span space, the same approximation the prepended prelude has always had, so a panic
diagnostic from a non-root module reports a position in the root file's coordinates.

The two-step linker strategy (clang on Unix; lld-link / cl.exe on Windows) is required because
LLVM object files need a platform linker driver to attach the C runtime startup code — neurc
cannot ship its own linker.
