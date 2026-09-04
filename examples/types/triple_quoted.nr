// Triple-quoted (block) strings: multi-line text with the closing delimiter's
// indentation stripped from every content line.
//
// The `"""` block spans lines verbatim. Whatever indentation the closing `"""`
// sits at is removed from each content line, so a block can be indented to match
// the code around it without that indentation leaking into the value. Escapes and
// `{...}` interpolation holes behave exactly as they do in a `"..."` literal.
//
// Every block below is checked against the exact text it should produce and then
// printed between markers, so the dedenting is visible column by column in the
// output. `main` returns the number of checks that passed.

func check(actual: string, expected: string) -> i32 {
    if actual == expected {
        print("[")
        print(actual)
        println("]")
        return 1
    }
    println("MISMATCH: rendered {actual}, expected {expected}")
    return 0
}

func main() -> i32 {
    mut passed: i32 = 0

    // The newlines that touch the delimiters are punctuation, not content: the one
    // right after the opening `"""` and the one before the closing line both go, so
    // the value is exactly its two lines. The four spaces the closing `"""` sits at
    // are stripped from both.
    val basic = """
    first
    second
    """
    passed = passed + check(basic, "first\nsecond")

    // Indentation deeper than the closing delimiter survives — only the common
    // prefix goes away, so nested structure inside the block is preserved.
    val nested = """
    root
        child
    """
    passed = passed + check(nested, "root\n    child")

    // A blank line needs no indentation of its own and normalizes to empty. It is
    // also how a trailing newline is written: put one before the closing line and
    // its terminator is the one that survives.
    val paragraphs = """
    one

    two
    """
    passed = passed + check(paragraphs, "one\n\ntwo")

    // Interpolation and the format mini-language work inside a block.
    val name = "Neuro"
    val ratio: f64 = 0.5
    val report = """
    engine: {name}
    ratio:  {ratio:.3}
    """
    passed = passed + check(report, "engine: Neuro\nratio:  0.500")

    // A `"` needs no escaping inside a block: only `"""` ends it.
    val quoted = """
    he said "yes"
    """
    passed = passed + check(quoted, "he said \"yes\"")

    // Escape sequences decode as usual.
    val escaped = """
    tab\there
    """
    passed = passed + check(escaped, "tab\there")

    return passed
}
