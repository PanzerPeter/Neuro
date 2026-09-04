// Showcase — the 2A sub-phase end to end: standard I/O and the spec stragglers,
// combined in one program.
//
// Cumulative integration example combining the features 2A added:
//   `@derive(Debug, PartialEq)` — struct rendering and structural equality
//   · `println` to stdout · `.is_nan()` on a computed float
//   · `&[T]` borrowed slices passed to one signature from an array and from a `Vec`
//   · the `IntoIterator` / `Iterator` protocol with an associated type (`type Item`)
//   · an associated-type bound (`S: Iterator<Item = i32>`)
//   · `.enumerate()` and the `.filter(p)` head adapter
//   · `.chars()` codepoint iteration and `.char_indices()` byte offsets
//   — alongside earlier features: `@derive(Copy, Clone)` structs with `&self` methods,
//   fixed-size arrays, `Vec<T>`, `match` on `Option`, and the format mini-language.
//
// The point of the combination: every one of these is a *separate* addition, and the
// program only produces the right report if they agree with each other — the slice
// parameter must accept both an array and a `Vec`, the protocol's `Item` must be the
// type the bound demanded, the byte offsets must line up with the codepoints standing
// at them, and a NaN must fail `==` while the derived equality still finds duplicates
// among the readings that are not NaN.
//
// Expected exit code: 63

@derive(Copy, Clone, Debug, PartialEq)
struct Reading {
    id: i32,
    value: f64
}

impl Reading {
    // A reading is usable when its value is a real number. `0.0 / 0.0` is the one way
    // to reach NaN without a literal for it, and `.is_nan()` is the only test that
    // answers — NaN compares false against everything, itself included.
    func usable(&self) -> bool {
        !self.value.is_nan()
    }
}

// One signature over any contiguous run: the array below and the `Vec` below both
// satisfy `&[Reading]` with no copy.
func usable_count(samples: &[Reading]) -> i32 {
    mut n = 0
    for r in samples {
        if r.usable() {
            n += 1
        }
    }
    return n
}

// A cursor over a fixed set of ids, written against the prelude's protocol. Its
// associated `Item` is what the bound below demands.
@derive(Copy, Clone)
struct Ids {
    values: [i32; 5],
    pos: u64
}

impl Iterator for Ids {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        if self.pos >= 5 {
            return Option::None
        }
        val v = self.values[self.pos]
        self.pos = self.pos + 1
        return Option::Some(v)
    }
}

// The bound names the element type, so the body may do arithmetic on what it yields
// without knowing which iterator produced it.
func sum_odd<S: Iterator<Item = i32>>(source: S) -> i32 {
    mut cursor = source
    mut total = 0
    loop {
        match cursor.next() {
            Option::Some(v) => {
                if v % 2 == 1 {
                    total += v
                }
            }
            Option::None => break
        }
    }
    return total
}

func main() -> i32 {
    val nan = 0.0 / 0.0

    // Index 3 repeats index 0 exactly; index 4 carries the NaN.
    val samples = [
        Reading { id: 1, value: 21.5 },
        Reading { id: 2, value: 18.0 },
        Reading { id: 3, value: 30.25 },
        Reading { id: 1, value: 21.5 },
        Reading { id: 4, value: nan }
    ]

    // The derived rendering shows the whole record; the derived `==` finds the repeat.
    // `.enumerate()` supplies the position the report prints alongside it.
    mut duplicates = 0
    for (i, r) in samples.enumerate() {
        mut repeat = false
        for j in 0u64..i {
            if samples[j] == r {
                repeat = true
            }
        }
        val mark = if repeat { "dup " } else { "    " }
        if repeat {
            duplicates += 1
        }
        println("{i:>2} {mark}{r:?}")
    }

    // NaN is not equal to itself, so the derived comparison of the NaN-carrying record
    // against its own copy is false even though every other field matches.
    val nan_copy = Reading { id: 4, value: nan }
    println("")
    println("NaN record equals its copy : {samples[4] == nan_copy}")
    println("duplicates found           : {duplicates}")

    // The same `&[Reading]` parameter, once from the array and once from a `Vec`.
    mut heap: Vec<Reading> = Vec::new()
    heap.push(samples[0])
    heap.push(samples[4])
    println("usable in array            : {usable_count(&samples)}")
    println("usable in vec              : {usable_count(&heap)}")

    // The protocol drives a `for` head and the associated-type bound in turn, and a
    // `.filter(p)` adapter runs over the same cursor.
    val ids = Ids { values: [1, 2, 3, 4, 5], pos: 0 }
    mut walked = 0
    for v in ids {
        walked += v
    }
    mut evens = 0
    for v in ids.filter(|n: i32| -> bool { n % 2 == 0 }) {
        evens += v
    }
    val odds = sum_odd(ids)
    println("protocol walk / even / odd : {walked} / {evens} / {odds}")

    // Codepoints and byte offsets over the same non-ASCII text: `.chars()` counts
    // scalars, `.char_indices()` pairs each with the byte it starts at, and the two
    // only agree on the count — the final offset is larger because 'é' takes two bytes.
    val label = "café"
    mut scalars = 0
    for c in label.chars() {
        scalars += 1
    }
    mut last_offset = 0
    for (off, c) in label.char_indices() {
        last_offset = off as i32
    }
    println("'{label}' scalars / bytes     : {scalars} / {label.len()}, last starts at {last_offset}")

    val total = duplicates * 10 + usable_count(&samples) * 4 + walked + evens + odds + scalars + last_offset
    println("total                      : {total}")
    return total
}
