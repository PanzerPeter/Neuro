// Every module is a namespace. A name declared in one module is a different item from the
// same name declared in another, so this file declares a `Point` and an `absolute` of its
// own beside `geometry.nr`'s, and neither has to be renamed.
//
//   modules/
//     namespaces.nr  <- this file, the root module
//     geometry.nr    <- module `geometry`, which declares a `Point` and an `absolute` too
//     shapes/mod.nr  <- module `shapes`

import shapes::{perimeter}

enum Reading {
    Low(i32),
    High(i32)
}

// One variant, imported on its own. `Low` keeps its enum's name below.
import Reading::High

struct Point {
    x: i32
}

// `geometry.nr` keeps a private `absolute` of its own, which its `Point::manhattan` calls.
func absolute(v: i32) -> i32 {
    if v < 0 {
        0 - v
    } else {
        v
    }
}

func classify(v: i32) -> Reading {
    if v > 10 {
        High(v)
    } else {
        Reading::Low(v)
    }
}

func main() -> i32 {
    val mine = Point { x: -7 }
    val theirs: geometry::Point = geometry::Point::new(-3, 4)
    val near = absolute(mine.x)                  // this file's `absolute`: 7
    val far = theirs.manhattan()                 // geometry's, inside its method: 7
    println("absolute (this file)   = {near}")
    println("manhattan (geometry's) = {far}")

    // The initializer still calls the imported function; after it, the name is a local.
    val perimeter = perimeter(near, far)         // (7 + 7) * 2 = 28
    println("perimeter (a local)    = {perimeter}")

    val level = match classify(perimeter) {
        High(v) => v,
        Reading::Low(v) => 0 - v
    }
    println("High (single import)   = {level}")

    level + geometry::ORIGIN_SHIFT               // 28 + 2 = 30
}
