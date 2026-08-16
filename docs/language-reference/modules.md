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

There is no `import` yet — writing a qualified path is what pulls a module into the
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

## What has not landed yet

Modules currently share **one flat namespace**: qualifiers are checked against the module
that owns the name and then erased, so the rest of the compiler sees a single-file program.
Two consequences, both of which the visibility and import items lift:

- **Two modules cannot declare the same name.** The collision is reported, naming both
  files, rather than one declaration silently winning.
- **Nothing is private, and nothing is required to be qualified.** `export`, `import`
  (`import math::{sqrt}`, aliases, relative paths, variant imports), inline `module { }`
  blocks, and `export import` re-exports are all still ahead.

Also not yet available: a qualified name in a `match` pattern (write the bare enum name —
the flat namespace makes it reach), a module-qualified trait in `impl`/`dyn` position, and
the functional-update form `mod::Point { x: 1.0, ..base }`.

Panic diagnostics report positions in the root file's coordinate space, since merged
modules share one span space — the same approximation the implicit prelude already carries.

## See also

- [`examples/modules/`](../../examples/modules/) — the program shown above, exit code `40`
- [`examples/showcase/telemetry/`](../../examples/showcase/telemetry/) — modules working
  alongside structs, generics, `Vec<T>`, `Option`, and enums
