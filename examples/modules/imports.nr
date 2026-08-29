// `import` declarations: the five ways to name what another module holds.
//
// An import does two things. It pulls the module into the build — without one, only a
// qualified path reaches into a module — and it binds names locally, so the rest of the
// file can drop the qualifier.
//
//   examples/modules/
//     imports.nr       <- this file, the root module
//     geometry.nr      <- module `geometry`
//     shapes/
//       mod.nr         <- module `shapes`
//       area.nr        <- module `shapes::area`

// Named items, one of them renamed. `Point` is a type; `ORIGIN_SHIFT` is a constant.
import geometry::{Point, ORIGIN_SHIFT as SHIFT}

// A child module of a directory module, under a shorter name.
import shapes::area as region

// The whole module, still written with its qualifier below.
import ./shapes

// Enum variants, spelled out. The implicit prelude already binds `Option`'s variants in
// every module (see `prelude.nr`), so this line is redundant here — it is the form to
// write when the variants belong to an enum of your own, and an explicit import of a
// prelude name takes precedence over the implicit one rather than colliding with it.
import Option::{Some, None}

// Absence is a value: a rectangle with no extent has no area worth reporting.
func area_of(width: i32, height: i32) -> Option<i32> {
    if width == 0 {
        return None
    }
    Some(region::rectangle(width, height))
}

func main() -> i32 {
    // The imported type and its associated function, both unqualified.
    val corner: Point = Point::new(3, 4)
    val start = Point::new(1, 1)
    val span = corner.manhattan() - start.manhattan()   // 7 - 2 = 5
    println("Point (named import)        = {span}")

    // A bare `import` binds no names, so this module keeps its qualifier.
    val edge = shapes::perimeter(span, 4)               // (5+4)*2 = 18
    println("shapes::perimeter (bare)    = {edge}")

    // The imported variants read as themselves in a pattern.
    val rect = match area_of(span, 4) {
        Some(value) => value,                           // 5 * 4 = 20
        None        => 0
    }
    println("region::rectangle (renamed) = {rect}")
    println("SHIFT (renamed constant)    = {SHIFT}")

    val total = rect + edge + SHIFT
    println("total                       = {total}")

    total                                               // 20 + 18 + 2 = 40
}
