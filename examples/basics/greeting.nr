// Neuro Example: writing to standard output
// Demonstrates the `print` / `println` builtins, including the interpolated holes
// they are usually given.

func main() -> i32 {
    val name: string = "Neuro"
    val version: i32 = 2
    val ratio: f64 = 0.875

    // `print` writes exactly its argument; `println` adds a trailing newline.
    print("hello from ")
    println(name)

    // The argument is one ordinary `string`, so interpolation — format spec and all —
    // has already rendered before the call.
    println("version {version}, phase {version} is {ratio:.2} done")

    // A `&string` sub-slice prints like any other text, and printing does not
    // consume the value it was given.
    println(name.slice(0..4))
    println(name)

    return name.len() as i32
}
