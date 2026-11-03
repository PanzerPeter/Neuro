// Showcase: a config manifest rendered from typed records and checked against the document
// it should produce. Integration example: triple-quoted block strings, a `@derive(Copy)`
// struct, an enum with a payload matched, a fixed array walked by `for`, format specs, and
// `.len()`. Each block string is dedented to the column of its closing `"""`, so literals
// stay indented with the code. The exit code is the byte length of the rendered manifest.

enum Setting {
    Flag(bool),
    Count(i32),
    Ratio(f64)
}

@derive(Copy, Clone)
struct Entry {
    key: i32,
    raw: f64
}

impl Entry {
    // A key below 10 is a boolean flag, below 20 a count, otherwise a ratio.
    func setting(&self) -> Setting {
        if self.key < 10 {
            return Setting::Flag(self.raw > 0.0)
        }
        if self.key < 20 {
            return Setting::Count(self.raw as i32)
        }
        return Setting::Ratio(self.raw)
    }

    // Names are keyed by position because `[string; N]` needs non-Copy elements.
    func name(&self) -> string {
        match self.key {
            1 => "verbose",
            11 => "workers",
            _ => "momentum"
        }
    }

    func render(&self) -> string {
        val label = self.name()
        return "  {label:<10}= " + describe(self.setting())
    }
}

func describe(setting: Setting) -> string {
    match setting {
        Setting::Flag(on) => render_flag(on),
        Setting::Count(n) => "{n:>4}",
        Setting::Ratio(r) => "{r:.3}"
    }
}

func render_flag(on: bool) -> string {
    if on {
        return "  on"
    }
    return " off"
}

func main() -> i32 {
    val header = """
    # neuro manifest
    """

    val footer = """
    # end
    """

    val entries: [Entry; 3] = [
        Entry { key: 1, raw: 1.0 },
        Entry { key: 11, raw: 8.0 },
        Entry { key: 21, raw: 0.9 }
    ]

    mut body = ""
    for entry in entries {
        body = body + entry.render() + "\n"
    }

    // A block ends at its last content line, so the separator between the header
    // and the body is written here rather than left to the closing delimiter.
    val manifest = header + "\n" + body + footer

    // The same document written out literally. A block string can hold the `#`
    // comment markers and the aligned columns verbatim, so the expectation reads
    // exactly like the output it is checking.
    val expected = """
    # neuro manifest
      verbose   =   on
      workers   =    8
      momentum  = 0.900
    # end
    """

    println(manifest)

    if manifest != expected {
        println("manifest did not match the expected document")
        return 0
    }

    val bytes = manifest.len() as i32
    println("manifest bytes: {bytes}")
    return bytes
}
