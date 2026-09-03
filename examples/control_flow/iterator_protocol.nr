// The iteration protocol: `for` over a type you wrote yourself.
//
// `for x in e` is not a built-in over arrays and ranges alone. It calls
// `e.into_iter()` once and then `.next()` until that answers `None`, so any type
// implementing `IntoIterator` (a container that hands out an iterator) or
// `Iterator` (a value that is already one) can stand in a `for` head.
//
// Both traits live in the prelude, so nothing here is imported:
//
//     trait Iterator     { type Item; func next(&mut self) -> Option<Self::Item> }
//     trait IntoIterator { type Item; type Iter; func into_iter(self) -> Self::Iter }
//
// A type implementing `Iterator` needs no second `IntoIterator` impl — it is its
// own iterator, and the loop uses it directly.

// A cursor over the even numbers below a limit. This is the `Iterator` half.
@derive(Copy, Clone)
struct EvensIter {
    next_value: i32,
    limit: i32
}

impl Iterator for EvensIter {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        if self.next_value >= self.limit {
            return Option::None
        }
        val current = self.next_value
        self.next_value = self.next_value + 2
        Option::Some(current)
    }
}

// The container half: `Evens` holds no cursor of its own, so it may be iterated
// more than once — each `for` head asks it for a fresh `EvensIter`.
@derive(Copy, Clone)
struct Evens {
    limit: i32
}

impl IntoIterator for Evens {
    type Item = i32
    type Iter = EvensIter

    func into_iter(self) -> EvensIter {
        EvensIter { next_value: 0, limit: self.limit }
    }
}

// An adapter: an iterator wrapping another one. `S` is bounded by
// `Iterator<Item = i32>`, which is what lets `self.inner.next()` type-check, and
// the closure field holds the transformation. Adapters implement `Iterator`
// themselves, so one composes in a `for` head exactly like the source it wraps.
struct ScaledIter<S> {
    inner: S,
    factor: (i32) -> i32
}

impl<S: Iterator<Item = i32>> Iterator for ScaledIter<S> {
    type Item = i32

    func next(&mut self) -> Option<i32> {
        match self.inner.next() {
            Option::Some(value) => {
                val scale = self.factor
                Option::Some(scale(value))
            }
            Option::None => Option::None
        }
    }
}

func main() -> i32 {
    // A container: the loop calls `into_iter()` for a cursor.
    val evens = Evens { limit: 10 }
    mut sum = 0
    for value in evens {
        println("even: {value}")
        sum = sum + value
    }
    println("sum of evens below 10 = {sum}")

    // `evens` is Copy and holds no cursor, so a second loop starts over.
    mut count = 0
    for (position, value) in evens.enumerate() {
        println("position {position} holds {value}")
        count = count + 1
    }
    println("visited {count} values")

    // An iterator used directly, with no container in front of it.
    val cursor = EvensIter { next_value: 4, limit: 10 }
    mut tail = 0
    for value in cursor {
        tail = tail + value
    }
    println("evens from 4 sum to {tail}")

    // An adapter over that same iterator, driven by one `for` head.
    val tripled = ScaledIter {
        inner: EvensIter { next_value: 0, limit: 8 },
        factor: |value: i32| -> i32 { value * 3 }
    }
    mut scaled = 0
    for value in tripled {
        println("tripled: {value}")
        scaled = scaled + value
    }
    println("tripled evens below 8 sum to {scaled}")

    return sum + count + tail + scaled
}
