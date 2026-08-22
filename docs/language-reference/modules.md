# Modules

Neuro programs may span several files. Every `.nr` file is a module, and a directory
holding a `mod.nr` is a module with children.

```
examples/modules/
  main.nr          <- the root module (the file you pass to neurc)
  geometry.nr      <- module `geometry`
  shapes/
    mod.nr         <- module `shapes`
    area.nr        <- module `shapes::area`
```

You compile the **root**; every module it reaches is pulled in with it.

```bash
neurc compile examples/modules/main.nr
```

## Reaching into a module

An `import`, or a qualified path written without one, is what pulls a module into the
build. A path is `module::item`, and it may descend through directory modules.

```neuro
func main() -> i32 {
    val corner = geometry::Point { x: 3, y: 4 }             // a struct from a module
    val start: geometry::Point = geometry::Point::new(1, 1) // its associated function

    val span = corner.manhattan() - start.manhattan()       // methods need no qualifier

    val rect = shapes::area::rectangle(span, 4)             // a grandchild module
    val edge = shapes::perimeter(span, 4)

    rect + edge + geometry::ORIGIN_SHIFT                    // a module constant
}
```

Functions, constants, structs, enums, newtypes, and traits are all reachable this way, in
value position and in type annotations alike. A **method** call needs no qualifier: it
resolves on the value's type, wherever that type was declared.

Because only referenced modules load, a directory full of unrelated single-file programs
still compiles one program at a time.

## Importing

An `import` does two things: it pulls the module into the build even when no qualified
path reaches into it, and it binds names locally so the rest of the file can drop the
qualifier.

```neuro
import math                          // the module, still written `math::sqrt` below
import ./utils                       // the same, written explicitly relative
import math::{sqrt, sin}             // two names, usable bare
import math::{sin as sine}           // one of them under a different name
import math::matrix as mat           // a child module under a shorter qualifier
import math::sqrt as root            // a single item under a different name
import Option::{Some, None}          // enum variants, usable without `Option::`
```

Every path is resolved relative to the importing file, whether or not it is written
`./`-first. A brace entry may name a **child module** as easily as an item —
`import ./utils::{io}` binds the module `utils::io`, so `io::read(x)` resolves.

Imported variants read as themselves in value and pattern position alike:

```neuro
import Option::{Some, None}

func halve(n: i32) -> Option<i32> {
    if n % 2 == 0 { return Some(n / 2) }
    None
}

func main() -> i32 {
    match halve(20) {
        Some(half) => half,
        None       => 0
    }
}
```

A variant written without its enum is only readable when an import accounts for it —
otherwise write `Option::Some`. Imports bind per file, so two modules may bind the same
name to different things, but one module may not bind one name twice.

## How a path is resolved

Each segment is looked up beside the file that wrote the path:

1. `<dir>/<segment>/mod.nr` — a directory module, whose own children are the files beside
   its `mod.nr`.
2. `<dir>/<segment>.nr` — a leaf module. A leaf has **no** children: `math.nr` is a file,
   so `math::io::read` cannot reach an `io.nr` sitting next to it.

A segment that names no module ends the lookup, and what is left is an ordinary type path.
That is how `Point::new` and `Option::Some` keep their meaning. A locally declared type
also wins over a same-named file, so adding `Point.nr` beside your code can never silently
re-point an existing `Point::new`.

## Visibility

A declaration is **private to its file** unless it carries `export`. Nothing changes for a
single-file program — one file is one module — but the moment a second file reads your code,
you choose what it may see.

```neuro
// config.nr
export struct Config {
    export host: i32,   // part of the surface
    timeout: i32        // internal: readable only inside config.nr
}

impl Config {
    func new(host: i32) -> Config { Config { host: host, timeout: 30 } }

    // Methods carry the type's visibility — there is no `export` on a method.
    func timeout(&self) -> i32 { self.timeout }
}

export func make(host: i32) -> Config { Config::new(host) }

// No `export`: nothing outside config.nr can name it, so it stays free to change.
func validate(host: i32) -> bool { host > 0 }
```

`export` goes between any `@derive(...)` attributes and the item keyword, the position `pub`
takes in Rust. It applies to `func`, `struct`, `enum`, `trait`, `const`, and `newtype`
declarations, and to each struct field independently — an exported struct may still keep a
field to itself, which is what makes an invariant hold across a module boundary.

Two rules follow from a private field, and both are enforced:

- another module cannot **read or write** it (`c.timeout`, `c.timeout = 5`), and
- another module cannot **construct** the struct at all, whether by listing the field or by
  reaching it through `..base` — the update form supplies every field you did not list.

There is no `export` on an `impl` block, a `type` alias, or an `import`. An `impl` declares no
name of its own; an alias is expanded at parse time, so nothing of it survives to be reached;
and `export import` re-export is not implemented yet. Each is rejected with a message saying so.

Item visibility is settled while modules are resolved, so it is reported before type checking.
Field visibility needs the receiver's type and is reported by the type checker.

## Diagnostics

| Situation | What you get |
|---|---|
| `math::cbrt` where `math.nr` has no `cbrt` | ``module `math` declares no item named `cbrt` `` |
| `utils::read` where `utils/` has no `mod.nr` | ``utils` is a directory with no `mod.nr` … add `utils/mod.nr`` |
| `missing::inner::value()` naming no module at all | ``missing::inner::value` does not name a module … expected `missing.nr` or `missing/mod.nr` beside it` |
| The same name declared by two loaded modules | ``shared` is declared in both `main.nr` and `helper.nr` … rename one of them` |
| `import math::{cbrt}` where `math.nr` has no `cbrt` | ``module `math` declares no item named `cbrt` `` |
| Two imports binding one name | ``sqrt` is imported twice … rename one of them with `as`` |
| `Some(n)` in a pattern with no import behind it | ``variant `Some` is used without its enum … write `Enum::Some` or add `import Enum::{Some}`` |
| Reaching a declaration with no `export` | ``internal` is private to module `lib` … write `export` before its declaration` |
| Reaching a field with no `export` | ``field 'timeout' of struct 'Config' is private to the module that declares it`` |
| `export` on an `impl`, a `type` alias, or an `import` | ``export` cannot be applied to …` |

## What has not landed yet

Modules still share **one flat namespace**: qualifiers are checked against the module that
owns the name and then erased, so the rest of the compiler sees a single-file program. Two
consequences:

- **Two modules cannot declare the same name**, even when both keep it private. The collision
  is reported, naming both files, rather than one declaration silently winning; renaming at
  the import site does not help, because the clash is between the declarations rather than the
  uses. Visibility says who may *reach* a name — lifting this needs the merge itself to stop
  being flat.
- **Qualification is checked but never required.** A name you could have imported reaches you
  whether or not you wrote the import, provided it is exported. What an import buys is the
  module load, the check that the name exists where you took it from, and the rename forms.

Inline `module { }` blocks, `export import` re-exports, and the implicit prelude import are
still ahead.

Also not yet available: a qualified name in a `match` pattern (write the bare enum name —
the flat namespace makes it reach), a module-qualified trait in `impl`/`dyn` position, and
the functional-update form `mod::Point { x: 1.0, ..base }`.

Panic diagnostics report positions in the root file's coordinate space, since merged
modules share one span space — the same approximation the implicit prelude already carries.

## See also

- [`examples/modules/main.nr`](../../examples/modules/main.nr) — qualified paths, exit code `40`
- [`examples/modules/imports.nr`](../../examples/modules/imports.nr) — every import form
  over the same modules, exit code `40`
- [`examples/showcase/telemetry/`](../../examples/showcase/telemetry/) — modules, imports, and
  `export` working alongside structs, generics, `Vec<T>`, `Option`, and enums
