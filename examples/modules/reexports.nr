// `export import`: re-exporting another module's names to offer a flatter public API.
//
//   examples/modules/
//     reexports.nr     <- this file, the root module
//     api.nr           <- module `api`, which re-exports from `geometry`
//     geometry.nr      <- module `geometry`, where the declarations really live
//
// The names arrive by two routes below — qualified through the facade, and imported from
// it — and both land on the same declaration in `geometry`. The rename rides along: what
// `api` calls `SHIFT` is `geometry`'s `ORIGIN_SHIFT`.

import api::{Point}

func main() -> i32 {
    // Through the facade's qualifier: `api` never declared `Point`, only re-exported it.
    val corner: api::Point = api::Point::new(3, 4)

    // The same type, this time imported from the facade rather than qualified.
    val start = Point::new(1, 1)

    val span = corner.manhattan() - start.manhattan()   // 7 - 2 = 5
    println("api::Point (qualified) -> corner = {corner.x}, {corner.y}")
    println("Point (imported)       -> start  = {start.x}, {start.y}")
    println("span                             = {span}")

    // `api` renamed it, but it is `geometry::ORIGIN_SHIFT` that answers.
    val shift = api::SHIFT
    println("api::SHIFT (geometry::ORIGIN_SHIFT) = {shift}")

    val total = span * shift
    println("total                            = {total}")

    total                                               // 5 * 2 = 10
}
