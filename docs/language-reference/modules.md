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
`./`-first. A brace entry may name a **child module** as easily as an item
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

A variant written without its enum is readable when an import accounts for it, or when the
prelude does, otherwise write `Shape::Circle`. Imports bind per file, so two modules may
bind the same name to different things, but one module may not bind one name twice.

## The implicit prelude

Every module begins as if it had written

```neuro
import std::prelude::{Option, Some, None, Result, Ok, Err, println, print}
```

so the two fallible types, their four variants, and the printing functions are in scope in
every file without an `import` of any kind:

```neuro
func halve(n: i32) -> Option<i32> {
    if n % 2 == 0 { return Some(n / 2) }
    None
}
```

A prelude binding is the weakest one there is, and shadowing it is never an error:

- a **local declaration** of the name wins inside the module that declares it: a module
  with its own `Option`, or its own `None`, keeps its own meaning;
- an **explicit import** of the name wins too, so `import Reading::{Some}` binds `Some` to
  that enum rather than colliding with the prelude's.

A file that wants none of it writes `@no_prelude` on its **first line**, before any
declaration, and never inside a `module { }` block, which is not a file. An inline block
inherits the answer of the file that holds it.

```neuro
@no_prelude

// `Some`, `None`, `Ok`, and `Err` are ordinary names in this file.
func triangular(n: i32) -> i32 { ... }
```

On a non-root file `@no_prelude` takes away that file's bindings. On the **root** file it
also drops the prelude's declarations from the whole program, `Option` and `Result` are
then declared nowhere, because the merged namespace is flat, so those types are either in
the program or absent from all of it.

## How a path is resolved

Each segment is looked up beside the file that wrote the path:

1. `<dir>/<segment>/mod.nr`, a directory module, whose own children are the files beside
   its `mod.nr`.
2. `<dir>/<segment>.nr`, a leaf module. A leaf has **no** children: `math.nr` is a file,
   so `math::io::read` cannot reach an `io.nr` sitting next to it.

A segment that names no module ends the lookup, and what is left is an ordinary type path.
That is how `Point::new` and `Option::Some` keep their meaning. A locally declared type
also wins over a same-named file, so adding `Point.nr` beside your code can never silently
re-point an existing `Point::new`.

## Visibility

A declaration is **private to its file** unless it carries `export`. Nothing changes for a
single-file program, one file is one module, but the moment a second file reads your code,
you choose what it may see.

```neuro
// config.nr
export struct Config {
    export host: i32,   // part of the surface
    timeout: i32        // internal: readable only inside config.nr
}

impl Config {
    func new(host: i32) -> Config { Config { host: host, timeout: 30 } }

    // Methods carry the type's visibility; there is no `export` on a method.
    func timeout(&self) -> i32 { self.timeout }
}

export func make(host: i32) -> Config { Config::new(host) }

// No `export`: nothing outside config.nr can name it, so it stays free to change.
func validate(host: i32) -> bool { host > 0 }
```

`export` goes between any `@derive(...)` attributes and the item keyword, the position `pub`
takes in Rust. It applies to `func`, `struct`, `enum`, `trait`, `const`, and `newtype`
declarations, and to each struct field independently, an exported struct may still keep a
field to itself, which is what makes an invariant hold across a module boundary.

Two rules follow from a private field, and both are enforced:

- another module cannot **read or write** it (`c.timeout`, `c.timeout = 5`), and
- another module cannot **construct** the struct at all, whether by listing the field or by
  reaching it through `..base`, the update form supplies every field you did not list.

There is no `export` on an `impl` block, a `type` alias, or an inline `module` block. An `impl`
declares no name of its own; an alias is expanded at parse time, so nothing of it survives to be
reached; and a block's name is reached only from the file that declares it, so there is no
outside to open it to. Each is rejected with a message saying so. An `import` *does* take
`export`, that is the re-export form, below.

Item visibility is settled while modules are resolved, so it is reported before type checking.
Field visibility needs the receiver's type and is reported by the type checker.

## Inline modules

For a grouping that belongs with the code around it, write a `module` block instead of a file:

```neuro
module geometry {
    export struct Circle { export radius: i32 }

    export func area(c: &Circle) -> i32 { c.radius * c.radius }

    // Private to the block: `main` below cannot name it, same file or not.
    func validate(r: i32) -> bool { r > 0 }
}

func main() -> i32 {
    val c = geometry::Circle { radius: 3 }
    geometry::area(&c)
}
```

A block is a module in every sense a file is one, and the same rules reach it: its items are
private unless written with `export`, an `import` binds from it (`import geometry::{Circle}`),
and blocks nest (`outer::inner::deep()`). Three consequences are worth stating outright:

- The file that declares a block is **outside** it. `export` is the only way in, exactly as
  for a file module.
- A block has **no file children**. A path through one stops where a leaf `.nr` file's does.
- A block **wins over a same-named file**, on the rule that a locally declared type follows:
  adding a file must never silently re-point a path that already resolved.

Use a block for logical grouping within one file. For an independent, separately compilable
unit, prefer a file module.

## Re-exporting

`export import` binds names locally like any import *and* makes them reachable through the
importing module, so a facade can offer a flatter public API than its internals:

```neuro
// api.nr: declares nothing of its own
export import ./geometry::{Point, ORIGIN_SHIFT as SHIFT}
```

```neuro
// reexports.nr
import api::{Point}

func main() -> i32 {
    val corner: api::Point = api::Point::new(3, 4)   // through the facade
    val start = Point::new(1, 1)                     // imported from the facade
    (corner.manhattan() - start.manhattan()) * api::SHIFT
}
```

A rename rides along and is undone on the way through: what `api` calls `SHIFT` is
`geometry`'s `ORIGIN_SHIFT`, and that is the name the program is built with. Facades chain:
a module may re-export what another module re-exported.

Only an **item** can be re-exported. A module and an enum variant are each reached through
something else, so `export import ./utils` and `export import Option::{Some}` are errors
rather than silent no-ops. A re-export also cannot open a private declaration: `export`
still has to be written where the declaration lives.

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
| `export` on an `impl`, a `type` alias, or a `module` block | ``export` cannot be applied to …` |
| `export import` naming a module or a variant | ``export import` … would re-export `x`, which names a module rather than an item` |
| Two inline blocks in one module sharing a name | ``m` is declared twice as an inline module … rename one of them` |

## What has not landed yet

Modules still share **one flat namespace**: qualifiers are checked against the module that
owns the name and then erased, so the rest of the compiler sees a single-file program. Two
consequences:

- **Two modules cannot declare the same name**, even when both keep it private. The collision
  is reported, naming both files, rather than one declaration silently winning; renaming at
  the import site does not help, because the clash is between the declarations rather than the
  uses. Visibility says who may *reach* a name; lifting this needs the merge itself to stop
  being flat.
- **Qualification is checked but never required.** A name you could have imported reaches you
  whether or not you wrote the import, provided it is exported. What an import buys is the
  module load, the check that the name exists where you took it from, and the rename forms.

The prelude is not a module you can name: `std::prelude` describes the effect, but the driver
prepends the declarations and seeds the variant bindings directly, so there is no `std::` path
to import from and no way to import a *subset* of it. `@no_prelude` on a
non-root file therefore drops that file's bindings only; its declarations stay in the flat
namespace, which is the same limitation as above.

An inline block does not lift the flat namespace either: a block buys a private *surface*,
not a private namespace, so its items still collide with same-named declarations elsewhere in
the program.

Also not yet available: a qualified name in a `match` pattern (write the bare enum name
the flat namespace makes it reach), a module-qualified trait in `impl`/`dyn` position, and
the functional-update form `mod::Point { x: 1.0, ..base }`.

Panic diagnostics report positions in the root file's coordinate space, since merged
modules share one span space, the same approximation the prepended prelude already carries.

## See also

- [`examples/modules/main.nr`](../../examples/modules/main.nr), qualified paths, exit code `40`
- [`examples/modules/imports.nr`](../../examples/modules/imports.nr), every import form
  over the same modules, exit code `40`
- [`examples/modules/inline.nr`](../../examples/modules/inline.nr), inline `module` blocks,
  nested, with a private helper, exit code `26`
- [`examples/modules/reexports.nr`](../../examples/modules/reexports.nr), an `export import`
  facade over `geometry.nr`, exit code `10`
- [`examples/modules/prelude.nr`](../../examples/modules/prelude.nr), `Option`, `Result`, and
  their variants named with no import at all, exit code `25`
- [`examples/modules/no_prelude.nr`](../../examples/modules/no_prelude.nr), the `@no_prelude`
  opt-out, exit code `36`
- [`examples/showcase/telemetry/`](../../examples/showcase/telemetry/), file modules, an inline
  block, an `export import` re-export, `export` visibility, the implicit prelude, and one module
  opting out with `@no_prelude`, working alongside structs, generics, `Vec<T>`, `Option`, and enums
