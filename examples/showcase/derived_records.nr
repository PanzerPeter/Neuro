// Showcase: derived `Debug` and `PartialEq` alongside earlier features. Integration
// example: `@derive(Debug, PartialEq)` on nested structs, `@derive(Copy, Clone)`, `match`
// on enums, a counted `for` loop, a labeled `break`, and the `:?`, `:>W` and `:<W` specs.
// Derived `==` is structural and recurses into the nested struct, so two samples built from
// the same numbers compare equal. The derived `{s:?}` shows the duplicate in the output,
// quotes and all.

@derive(Copy, Clone, Debug, PartialEq)
struct Origin {
    bay: i32,
    slot: i32
}

@derive(Copy, Clone, Debug, PartialEq)
struct Sample {
    origin: Origin,
    label: char,
    reading: f64
}

impl Sample {
    // The derived `==` is structural, so identity plays no part: this weight is
    // deliberately derived from the fields alone, the same way equality is.
    func weight(&self) -> i32 {
        self.origin.bay * 10 + self.origin.slot
    }
}

enum Verdict {
    Fresh,
    Duplicate
}

func score(v: Verdict) -> i32 {
    match v {
        Verdict::Fresh => 3,
        Verdict::Duplicate => 1
    }
}

func main() -> i32 {
    // Two of these are structural duplicates of an earlier entry: index 2 repeats
    // index 0 exactly, and index 4 repeats index 1 exactly. Index 3 differs from
    // index 0 only inside the nested `Origin`, which the recursion has to catch.
    val samples = [
        Sample { origin: Origin { bay: 1, slot: 2 }, label: 'a', reading: 21.5 },
        Sample { origin: Origin { bay: 3, slot: 4 }, label: 'e', reading: 18.0 },
        Sample { origin: Origin { bay: 1, slot: 2 }, label: 'a', reading: 21.5 },
        Sample { origin: Origin { bay: 1, slot: 9 }, label: 'a', reading: 21.5 },
        Sample { origin: Origin { bay: 3, slot: 4 }, label: 'e', reading: 18.0 }
    ]

    mut total = 0
    mut duplicates = 0

    for i in 0..5 {
        // A sample is a duplicate when it equals an earlier one outright. The
        // labeled break stops at the first match: later ones say nothing new.
        mut verdict = Verdict::Fresh
        'scan: for j in 0..i {
            if samples[i] == samples[j] {
                verdict = Verdict::Duplicate
                break 'scan
            }
        }

        val points = score(verdict)
        val kind = match verdict {
            Verdict::Fresh => "fresh",
            Verdict::Duplicate => "dup"
        }
        if points == 1 {
            duplicates += 1
        }

        // `{s:?}` is the derived rendering: the whole record, nested struct and all.
        println("{i:>2}  {kind:<6} {samples[i]:?}")
        total += points + samples[i].weight()
    }

    println("")
    println("duplicates = {duplicates}")
    println("total      = {total}")

    return total
}
