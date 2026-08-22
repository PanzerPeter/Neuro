// A sibling module. The file name is the module name: this is `geometry`.
//
// Declarations are private to their module unless they carry `export`, so this file
// chooses what the rest of the program may name.

export struct Point {
    export x: i32,
    export y: i32
}

impl Point {
    func new(x: i32, y: i32) -> Point {
        Point { x: x, y: y }
    }

    func manhattan(&self) -> i32 {
        absolute(self.x) + absolute(self.y)
    }
}

// No `export`: nothing outside `geometry` can name this, so it stays free to change.
func absolute(v: i32) -> i32 {
    if v < 0 {
        0 - v
    } else {
        v
    }
}

export const ORIGIN_SHIFT: i32 = 2
