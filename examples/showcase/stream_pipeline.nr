// Showcase — the iteration protocol carrying a small stream pipeline.
//
// Integration example combining several features at once:
//   the `IntoIterator` / `Iterator` protocol · the `.map(f)` / `.filter(p)` head
//   adapters · trait associated types (`type Item`) and an associated-type bound
//   (`S: Iterator<Item = i32>`) · generic structs monomorphized per instance ·
//   closures stored in a struct field · `@derive(Copy)` structs with `&mut self`
//   methods · `Option` + `match` · `Vec<i32>` + `for`-in · string interpolation
//   with the format mini-language.
//
// The point is composition: a source, a transform, and a filter are three separate
// types, each implementing `Iterator`, and a single `for` head drives the whole chain
// one element at a time — no intermediate array is ever built.

// A container. It holds no cursor, so it may be walked more than once: each `for`
// head asks it for a fresh iterator through `into_iter()`.
@derive(Copy, Clone)
struct Readings {
    samples: [i32; 6]
}

// The cursor `Readings` hands out. `Copy`, so it can be an adapter's type argument.
@derive(Copy, Clone)
struct ReadingsIter {
    samples: [i32; 6],
    position: u64
}

impl Iterator for ReadingsIter {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        if self.position >= self.samples.len() {
            return Option::None
        }
        val sample = self.samples[self.position]
        self.position = self.position + 1
        Option::Some(sample)
    }
}

impl IntoIterator for Readings {
    type Item = i32
    type Iter = ReadingsIter

    func into_iter(self) -> ReadingsIter {
        ReadingsIter { samples: self.samples, position: 0 }
    }
}

// A transform. The `Iterator<Item = i32>` bound is what lets the body call
// `self.inner.next()` and know an `i32` comes back.
@derive(Copy, Clone)
struct Scaled<S> {
    inner: S,
    factor: i32
}

impl<S: Iterator<Item = i32>> Iterator for Scaled<S> {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        match self.inner.next() {
            Option::Some(value) => Option::Some(value * self.factor),
            Option::None => Option::None
        }
    }
}

// A filter. One `next()` may pull several from the source, which is exactly why a
// filter has to be an iterator of its own rather than a shape the loop can special-case.
@derive(Copy, Clone)
struct Above<S> {
    inner: S,
    floor: i32
}

impl<S: Iterator<Item = i32>> Iterator for Above<S> {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        loop {
            match self.inner.next() {
                Option::Some(value) => {
                    if value > self.floor {
                        break Option::Some(value)
                    }
                }
                Option::None => { break Option::None }
            }
        }
    }
}

// A transform carrying a closure instead of a constant. The closure is stored in the
// struct and called per element, so the adapter is reusable for any i32 → i32 rule.
struct Shaped<S> {
    inner: S,
    rule: (i32) -> i32
}

impl<S: Iterator<Item = i32>> Iterator for Shaped<S> {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        match self.inner.next() {
            Option::Some(value) => {
                val apply = self.rule
                Option::Some(apply(value))
            }
            Option::None => Option::None
        }
    }
}

func main() -> i32 {
    val readings = Readings { samples: [3, 8, 1, 12, 5, 9] }

    // The container in a `for` head: `into_iter()` once, `next()` per step.
    mut raw_total = 0
    for sample in readings {
        raw_total = raw_total + sample
    }
    println("raw samples total {raw_total}")

    // `readings` is Copy and stateless, so the second walk starts over. The position
    // an enumerated head binds is the loop's own count, not anything the source holds.
    mut counted = 0
    for (position, sample) in readings.enumerate() {
        println("sample {position:>2}: {sample:>3}")
        counted = counted + 1
    }
    println("walked {counted} samples")

    // Two adapters stacked over one source, driven by a single `for` head. Nothing
    // between the source and the loop is ever materialized.
    val pipeline = Above {
        inner: Scaled { inner: readings.into_iter(), factor: 3 },
        floor: 20
    }
    mut kept: Vec<i32> = Vec::new()
    for value in pipeline {
        kept.push(value)
    }

    mut kept_total = 0
    for value in kept {
        println("kept {value}")
        kept_total = kept_total + value
    }
    println("kept {kept.len()} of {counted}, totalling {kept_total}")

    // The same source through a closure-carrying adapter.
    val shaped = Shaped {
        inner: readings.into_iter(),
        rule: |sample: i32| -> i32 { sample % 5 }
    }
    mut remainders = 0
    for value in shaped {
        remainders = remainders + value
    }
    println("sample remainders mod 5 total {remainders}")

    // The same pipeline written with the compiler's own adapters. `.map` and
    // `.filter` are part of the `for` head rather than values, so one head
    // expresses what `Scaled` + `Above` needed two types for — and the head they
    // decorate is the very same `IntoIterator` container.
    mut inline_total = 0
    for value in readings
        .map(|sample: i32| -> i32 { sample * 3 })
        .filter(|scaled: i32| -> bool { scaled > 20 }) {
        inline_total = inline_total + value
    }
    println("inline pipeline totals {inline_total}, matching {kept_total}")

    // An adapted head over an array, enumerated. The position counts what the
    // chain YIELDED, so it stays dense however much the filter drops.
    val labels: [i32; 6] = [4, 11, 6, 15, 2, 19]
    mut flagged = 0
    for (rank, value) in labels.filter(|v: i32| -> bool { v > 10 }).enumerate() {
        println("rank {rank:>2} flagged {value:>3}")
        flagged = flagged + value
    }
    println("flagged {flagged} across the run")

    return raw_total + counted + kept_total + remainders + flagged
}
