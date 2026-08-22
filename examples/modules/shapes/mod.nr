// A directory module: `shapes/` is a module because it holds this `mod.nr`.
// Files beside it (`area.nr`) are its children, reached as `shapes::area`.

export func perimeter(width: i32, height: i32) -> i32 {
    (width + height) * 2
}
