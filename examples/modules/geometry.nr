// A sibling module. The file name is the module name: this is `geometry`.

struct Point {
    x: i32,
    y: i32
}

impl Point {
    func new(x: i32, y: i32) -> Point {
        Point { x: x, y: y }
    }

    func manhattan(&self) -> i32 {
        self.x + self.y
    }
}

const ORIGIN_SHIFT: i32 = 2
