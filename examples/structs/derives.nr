// Deriving `Debug` and `PartialEq`
//
// `@derive(Debug)` gives a struct the `{value:?}` rendering — its name, then each
// field in declaration order. `@derive(PartialEq)` gives it `==` / `!=`, compared
// field by field. Both recurse: a field that is itself a struct needs the same
// derive, which is what lets `Reading` render and compare through `Sensor`.
//
// Every name in a `@derive` list is checked. `@derive(Debug, PartialEq)` is the whole
// derivable-and-implemented set beyond `Copy` / `Clone`; anything else — a misspelling,
// or a trait no pass generates yet — is a compile error rather than a silent no-op.

@derive(Debug, PartialEq)
struct Sensor {
    id: i32,
    label: string
}

@derive(Debug, PartialEq)
struct Reading {
    sensor: Sensor,
    value: f64,
    valid: bool
}

func main() -> i32 {
    val a = Reading {
        sensor: Sensor { id: 7, label: "intake" },
        value: 21.5,
        valid: true
    }
    // Same fields, built independently — equality is structural, not identity.
    val b = Reading {
        sensor: Sensor { id: 7, label: "intake" },
        value: 21.5,
        valid: true
    }
    // Differs only in the nested struct's label, which the recursion reaches.
    val c = Reading {
        sensor: Sensor { id: 7, label: "exhaust" },
        value: 21.5,
        valid: true
    }

    // The debug form quotes a `string` field, so the label is unambiguous.
    println("a = {a:?}")
    println("c = {c:?}")
    println("a == b: {a == b}")
    println("a == c: {a == c}")

    if a == b && a != c {
        return 21                       // both comparisons held
    }
    return 0
}
