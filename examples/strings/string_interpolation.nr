// String interpolation: embedded expressions and the format mini-language.
//
// A `{...}` hole holds any expression. An optional `:spec` after it chooses the
// rendering: precision, radix, sign, field width, and alignment. Every literal
// below is checked against the exact text it should produce and then printed, so
// the output is a rendering table for the whole mini-language. `main` returns the
// number of checks that passed.

func check(actual: string, expected: string) -> i32 {
    if actual == expected {
        println(actual)
        return 1
    }
    println("MISMATCH: rendered {actual}, expected {expected}")
    return 0
}

func area(width: i32, height: i32) -> i32 {
    return width * height
}

func main() -> i32 {
    val name = "Neuro"
    val version: i32 = 3
    val pi: f64 = 3.14159
    val flags: i32 = 255
    val delta: i32 = -42
    val ready = true
    val grade: char = 'A'

    mut passed: i32 = 0

    // A hole names a binding, or holds a whole expression.
    passed += check("Welcome to {name} v{version}", "Welcome to Neuro v3")
    passed += check("area = {area(3, 4)}", "area = 12")
    passed += check("{version} + 1 = {version + 1}", "3 + 1 = 4")

    // Floats: `.N` fixed-point, `e` scientific. Without a spec, a float keeps
    // its point so it still reads as a float.
    passed += check("{pi:.2}", "3.14")
    passed += check("{pi:.4}", "3.1416")
    passed += check("{pi:e}", "3.14159e0")

    // Integers: radix and sign.
    passed += check("{flags:x}", "ff")
    passed += check("{flags:X}", "FF")
    passed += check("{flags:b}", "11111111")
    passed += check("{flags:o}", "377")
    passed += check("{delta:+d}", "-42")

    // Width and alignment. Zero fill lands after the sign, not in front of it.
    passed += check("[{flags:8d}]", "[     255]")
    passed += check("[{name:<8}]", "[Neuro   ]")
    passed += check("[{name:>8}]", "[   Neuro]")
    passed += check("[{name:^9}]", "[  Neuro  ]")
    passed += check("{flags:08d}", "00000255")
    passed += check("{delta:06d}", "-00042")

    // `bool` and `char` render directly; `:?` quotes strings and chars.
    passed += check("ready={ready} grade={grade}", "ready=true grade=A")
    passed += check("{name:?} {grade:?}", "\"Neuro\" 'A'")

    // `\{` and `\}` write literal braces; an unescaped `}` outside a hole is an error.
    passed += check("\{not a hole\}", "\{not a hole\}")

    return passed
}
