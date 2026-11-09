# Modules

Neuro programs may span several files. Every `.nr` file is a module, and a directory holding a
`mod.nr` is a module with children.

```text
examples/modules/
  main.nr          <- the root module (the file you pass to neurc)
  geometry.nr      <- module `geometry`
  shapes/
    mod.nr         <- module `shapes`
    area.nr        <- module `shapes::area`
```

You compile the **root**. Every module it reaches is pulled in with it.

```bash
neurc compile examples/modules/main.nr
```

## Reaching into a module

An `import`, or a qualified path written without one, is what pulls a module into the build. A path
is `module::item`, and it may descend through directory modules.

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

Functions, constants, structs, enums, newtypes and traits are reachable this way, in every position a
name takes: a call, a type annotation, a pattern, a struct literal, an `impl` header, a bound and a
`dyn` type. Structs and enums are described in [Structs and Enums](structs.md).

```neuro
match shape {
    shapes::Shape::Circle(r) => r * 3,                       // a pattern
    shapes::Shape::Square(w) => w * w
}
val moved = geo::Point { x: 2, ..geo::origin() }            // a struct literal with a base
impl geo::Drawable for Dot { func size(&self) -> i32 { 1 } } // an impl header
func total(d: &dyn geo::Drawable) -> i32 { d.size() }       // a `dyn` type
```

A **method** call needs no qualifier. It resolves on the value's type, wherever that type was declared.

Because only referenced modules load, a directory full of unrelated single-file programs still
compiles one program at a time.

## Every module is a namespace

A name declared in one module is a different item from the same name declared in another. Two files
may each declare a private `helper`, or an exported `size`, and each module's code reaches its own:

```neuro
// geo.nr
func helper() -> i32 { 100 }
export func area() -> i32 { helper() }     // geo's helper

// main.nr
func helper() -> i32 { 1 }
func main() -> i32 { geo::area() + helper() }   // 100 + 1
```

Inside a module, its own declarations are in scope by their bare names. Anything else is reached by a
path or brought into scope by an `import`. A name from another module is never in scope just because
that module was loaded. The two names only meet where one scope imports both, and that import is the
error.

## Importing

An `import` does two things. It pulls the module into the build, even when no qualified path reaches
into it. It also binds names locally, so the rest of the file can drop the qualifier.

```neuro
import math                          // the module, still written `math::sqrt` below
import ./utils                       // a module beside this file
import ../shared::config             // a module one directory above this file
import math::{sqrt, sin}             // two names, usable bare
import math::{sin as sine}           // one of them under a different name
import math::matrix as mat           // a child module under a shorter qualifier
import math::sqrt as root            // a single item under a different name
import Option::{Some, None}          // enum variants, usable without `Option::`
import Shape::Circle                 // one variant
```

A path written without `./` or `../` starts at the program's root directory, the one holding the file
you passed to `neurc`. `./` starts at the importing file's own directory, and each `../` climbs one
level above it. A brace entry may name a **child module** as well as an item.
`import ./utils::{io}` binds the module `utils::io`, so `io::read(x)` resolves.

An import may not bind a name the module already declares, or one another import already bound.
`as` renames one of them. A local variable may shadow an imported name, renamed or not.

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

A variant written without its enum is readable when an import accounts for it, or when the prelude
does. Otherwise write `Shape::Circle`. Imports bind per module, so two modules may bind the same name
to different things.

## The implicit prelude

Every module begins as if it had written

```text
import std::prelude::{Option, Some, None, Result, Ok, Err}
```

so the two fallible types and their four variants are in scope in every file, with no `import` at all.
`ParseError`, the error [`.parse`](strings.md#parsing-text) returns, is in scope the same way, but its
variants keep the `ParseError::` prefix.


```neuro
func halve(n: i32) -> Option<i32> {
    if n % 2 == 0 { return Some(n / 2) }
    None
}
```

A prelude binding is the weakest one there is, and shadowing it is never an error:

- a **local declaration** of the name wins inside the module that declares it. A module with its own
  `Option`, or its own `None`, keeps its own meaning;
- an **explicit import** of the name wins too. `import Reading::{Some}` binds `Some` to that enum
  rather than colliding with the prelude's.

A file that wants none of it writes `@no_prelude` on its **first line**, before any declaration. It
cannot appear inside a `module { }` block, which is not a file. An inline block inherits the answer of
the file that holds it.

```neuro
@no_prelude

// `Some`, `None`, `Ok`, and `Err` are ordinary names in this file.
func triangular(n: i32) -> i32 {
    n * (n + 1) / 2
}
```

On a non-root file, `@no_prelude` takes away that file's variant bindings. On the **root** file it also
drops the prelude's declarations from the whole program. `Option` and `Result` are then declared
nowhere: the prelude's declarations are either in the program or absent from all of it.

`print` / `println` (and the panic family) are **not** in the prelude. They are compiler builtins
resolved by name, so `@no_prelude` does not take them away. Only a local declaration of the same name
shadows one. See [Standard Output Builtins](functions.md#standard-output-builtins).

## How a path is resolved

The first segment is looked up where the path is written: a type, an imported name, or an inline block
in scope comes first. `Point::new` is the associated function of the `Point` in scope, and adding a
`Point.nr` can never silently re-point it. A first segment that nothing in scope names starts at the
program's root directory, so `io::read` means the root's `io.nr` from every file of the program. Each
further segment is looked up below the module reached so far:

1. `<dir>/<segment>/mod.nr`, a directory module, whose own children are the files beside its `mod.nr`.
2. `<dir>/<segment>.nr`, a leaf module. A leaf has **no** children. `math.nr` is a file, so
   `math::io::read` cannot reach an `io.nr` sitting next to it.
3. An inline `module` block the module declares.

A segment that names no module ends the lookup, and what is left is an ordinary type path. That is how
`shapes::Shape::Circle` and `Option::Some` keep their meaning. The root file is a module like any
other, so another file reaches its exported items by its name: `main::Config` for a root `main.nr`.

## Visibility

A declaration is **private to its file** unless it carries `export`. A single-file program is one module,
so nothing changes there. Once a second file reads your code, you choose what it may see.

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

`export` goes between any `@derive(...)` attributes and the item keyword, the position `pub` takes in
Rust. It applies to `func`, `struct`, `enum`, `trait`, `const`, and `newtype` declarations, and to each
struct field independently. An exported struct may keep a field to itself, which is what makes an
invariant hold across a module boundary.

A private field has two consequences, and both are enforced:

- another module cannot **read or write** it (`c.timeout`, `c.timeout = 5`), and
- another module cannot **construct** the struct at all. Listing the field is refused, and so is
  reaching it through `..base`, since the update form supplies every field you did not list. See
  [Struct Instantiation](structs.md#struct-instantiation).

There is no `export` on an `impl` block or a `type` alias. An `impl` declares no name of its own. An
alias is expanded at parse time, so nothing of it survives to be reached. Each is rejected with a
message that says so. An inline `module` block takes `export` like an item (below), and an `import`
takes it as the re-export form.

Item visibility is settled while modules are resolved, so it is reported before type checking. Field
visibility needs the receiver's type and is reported by the type checker.

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

A block is a module in every sense a file is one, and the same rules reach it. Its items have names of
their own and are private unless written with `export`. An `import` binds from it
(`import geometry::{Circle}`), and blocks nest (`outer::inner::deep()`). Four consequences follow:

- The module that declares a block is **outside** it. `export` is the only way in, exactly as for a file
  module.
- A block **sees the names around it**: the declarations of the module it is written in, sibling blocks
  included, and their private ones too, since it is part of that module.
- A block is itself **private** unless written `export module`. Without it, only code inside the
  declaring module can reach the block, whatever the block's own items export.
- A block has **no file children**. A path through one stops where a leaf `.nr` file's does. A block
  in scope **wins over a same-named file**, on the rule that a locally declared type follows.

Use a block for logical grouping within one file. For an independent, separately compilable unit, prefer
a file module.

## Re-exporting

`export import` binds names locally like any import *and* makes them reachable through the importing
module. A facade can therefore offer a flatter public API than its internals:

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

A rename rides along and is undone on the way through. What `api` calls `SHIFT` is `geometry`'s
`ORIGIN_SHIFT`, and that is the name the program is built with. Facades chain: a module may re-export
what another module re-exported.

Only an **item** can be re-exported. A module and an enum variant are each reached through something
else, so `export import ./utils` and `export import Option::{Some}` are errors, not silent no-ops. A
re-export also cannot open a private declaration. `export` still has to be written where the
declaration lives.

## Diagnostics

| Situation | What you get |
|---|---|
| `math::cbrt` where `math.nr` has no `cbrt` | ``module `math` declares no item named `cbrt` `` |
| `utils::read` where `utils/` has no `mod.nr` | ``utils` is a directory with no `mod.nr` … add `utils/mod.nr`` |
| `missing::inner::value()` naming no module at all | ``missing::inner::value` does not name a module … expected `missing.nr` or `missing/mod.nr` beside it`` |
| A root-file name used bare in another module | ``secret` is declared in `main.nr`, not in `lib.nr` … import it or write its path`` |
| An import of a name the module declares | ``size` is imported in `main.nr`, which declares a `size` of its own … rename the import with `as`` |
| `import math::{cbrt}` where `math.nr` has no `cbrt` | ``module `math` declares no item named `cbrt` `` |
| Two imports binding one name | ``sqrt` is imported twice … rename one of them with `as`` |
| `Some(n)` in a pattern with no import behind it | ``variant `Some` is used without its enum … write `Enum::Some` or add `import Enum::{Some}`` |
| Reaching a declaration with no `export` | ``internal` is private to module `lib` … write `export` before its declaration`` |
| Reaching a field with no `export` | ``field 'timeout' of struct 'Config' is private to the module that declares it`` |
| `export` on an `impl` or a `type` alias | ``export` cannot be applied to …`` |
| `export import` naming a module or a variant | ``export import` … would re-export `x`, which names a module rather than an item`` |
| Two inline blocks in one module sharing a name | ``m` is declared twice as an inline module … rename one of them`` |

## What has not landed yet

The prelude is not a module you can name. `std::prelude` describes the effect, but the driver prepends
the declarations and seeds the variant bindings directly. So `import std::prelude::{Option}` is an error
(`std::prelude` does not name a module), and there is no way to import a *subset* of the prelude.
`@no_prelude` on a non-root file drops that file's variant bindings only; `Option` and `Result` still
reach it.

A root file that declares a name the prelude also declares, such as its own `Result`, replaces the
prelude's for the whole program.

Diagnostics do not yet name the file they come from. A compile-time error in a program of more than one
module prints no location, and a panic in a module other than the root reports a position in the root
file's coordinates.

## See also

- [Structs and Enums](structs.md), struct fields, visibility of fields, and enum variants
- [Generics and Traits](generics-and-traits.md), traits, `impl` blocks and bounds
- [Functions](functions.md), the standard output builtins that the prelude does not cover
- [`examples/modules/main.nr`](../../examples/modules/main.nr), qualified paths, exit code `40`
- [`examples/modules/imports.nr`](../../examples/modules/imports.nr), every import form over the same
  modules, exit code `40`
- [`examples/modules/inline.nr`](../../examples/modules/inline.nr), inline `module` blocks, nested, with a
  private helper, exit code `26`
- [`examples/modules/namespaces.nr`](../../examples/modules/namespaces.nr), the root and `geometry.nr`
  declaring the same names, a single-variant import, and a local shadowing an import, exit code `30`
- [`examples/modules/reexports.nr`](../../examples/modules/reexports.nr), an `export import` facade over
  `geometry.nr`, exit code `10`
- [`examples/modules/prelude.nr`](../../examples/modules/prelude.nr), `Option`, `Result`, and their
  variants named with no import at all, exit code `25`
- [`examples/modules/no_prelude.nr`](../../examples/modules/no_prelude.nr), the `@no_prelude` opt-out, exit
  code `36`
- [`examples/showcase/telemetry/`](../../examples/showcase/telemetry/), file modules, an inline block, an
  `export import` re-export, `export` visibility, the implicit prelude, and one module opting out with
  `@no_prelude`, working alongside structs, generics, `Vec<T>`, `Option`, and enums
