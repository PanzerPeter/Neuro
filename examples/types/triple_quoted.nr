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

    // The newline right after the opening `"""` is punctuation, not content.
    // The four spaces the closing `"""` sits at are stripped from both lines.
    val basic = """
    first
    second
    """
    passed = passed + check(basic, "first\nsecond\n")

    // Indentation deeper than the closing delimiter survives — only the common
    // prefix goes away, so nested structure inside the block is preserved.
    val nested = """
    root
        child
    """
    passed = passed + check(nested, "root\n    child\n")

    // A blank line needs no indentation of its own and normalizes to empty.
    val paragraphs = """
    one

    two
    """
    passed = passed + check(paragraphs, "one\n\ntwo\n")

    // Interpolation and the format mini-language work inside a block.
    val name = "Neuro"
    val ratio: f64 = 0.5
    val report = """
    engine: {name}
    ratio:  {ratio:.3}
    """
    passed = passed + check(report, "engine: Neuro\nratio:  0.500\n")

    // A `"` needs no escaping inside a block: only `"""` ends it.
    val quoted = """
    he said "yes"
    """
    passed = passed + check(quoted, "he said \"yes\"\n")

    // Escape sequences decode as usual.
    val escaped = """
    tab\there
    """
    passed = passed + check(escaped, "tab\there\n")

    return passed
}
