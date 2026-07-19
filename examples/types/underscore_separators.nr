// Demonstrates underscore digit separators in numeric literals.
// Underscores group digits for readability and are ignored by the compiler.
// They work in every base, in floats, in exponents, and alongside type suffixes.

func main() -> i32 {
    val million = 1_000_000      // decimal grouping
    val grouped = 0xFF_FF        // hex grouping        -> 65535
    val nibble  = 0b1010_0011    // binary nibble groups -> 163
    val perms   = 0o7_5_5        // octal grouping       -> 493
    val ratio   = 1_000.000_5    // float with separators

    // Reduce to a deterministic exit code of 0.
    val base = (million / 1_000) - 1_000        // 0
    return base
        + (grouped - 65_535)                    // 0
        + (nibble - 163)                        // 0
        + (perms - 493)                         // 0
        + ((ratio as i32) - 1_000)              // 0
}
