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

## What has not landed yet

Modules currently share **one flat namespace**: qualifiers are checked against the module
that owns the name and then erased, so the rest of the compiler sees a single-file program.
Two consequences, both of which the visibility item lifts:

- **Two modules cannot declare the same name.** The collision is reported, naming both
  files, rather than one declaration silently winning. Renaming at the import site does
  not help — the clash is between the declarations, not the uses.
- **Nothing is private, and nothing is required to be qualified.** An import is a
  convenience, not a gate: a name reaches whether or not you imported it. `export`, inline
  `module { }` blocks, `export import` re-exports, and the implicit prelude import are
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
- [`examples/showcase/telemetry/`](../../examples/showcase/telemetry/) — modules and imports
  working alongside structs, generics, `Vec<T>`, `Option`, and enums
