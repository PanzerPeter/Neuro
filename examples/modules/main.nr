// Multi-file compilation: every `.nr` file is a module, and a directory holding a
// `mod.nr` is a module with children.
//
//   examples/modules/
//     main.nr          <- this file, the root module
//     geometry.nr      <- module `geometry`
//     shapes/
//       mod.nr         <- module `shapes`
//       area.nr        <- module `shapes::area`
//
// This program imports nothing on purpose: a qualified path pulls a module into the build
// on its own, so the unrelated programs sitting elsewhere under examples/ stay out of it.
// See imports.nr beside this file for the same modules named through `import`.

func main() -> i32 {
    // A struct from another module, built directly and through its associated function.
    val corner = geometry::Point { x: 3, y: 4 }
    val start: geometry::Point = geometry::Point::new(1, 1)

    // Methods resolve on the value, so only the type needed qualifying.
    val span = corner.manhattan() - start.manhattan()   // 7 - 2 = 5

    val rect = shapes::area::rectangle(span, 4)         // 5 * 4  = 20
    val edge = shapes::perimeter(span, 4)               // (5+4)*2 = 18

    rect + edge + geometry::ORIGIN_SHIFT                // 20 + 18 + 2 = 40
}
