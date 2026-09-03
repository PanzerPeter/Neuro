// Adapter methods in a `for` head: `.map(f)` and `.filter(p)`.
//
// §3.7 names `.map()`, `.filter()`, and `.enumerate()` as adapters that "return
// new `Iterator`s, so they compose in a single `for` head". They are recognised
// as part of the head rather than resolved as methods, which is what lets them
// apply to a range and an array — neither of which is a value with methods on it.
//
//     .map(f)     replaces each element with `f(element)`
//     .filter(p)  drops each element for which `p(element)` is false
//
// The chain runs left to right, one element at a time: nothing between the source
// and the loop is ever materialized.

func main() -> i32 {
    // One adapter over an array.
    mut doubled = 0
    for value in [1, 2, 3, 4].map(|x: i32| -> i32 { x * 2 }) {
        println("doubled: {value}")
        doubled = doubled + value
    }

    // One over a range. A range needs parentheses, because `..` binds looser
    // than a method call.
    mut multiples = 0
    for value in (0..12).filter(|x: i32| -> bool { x % 4 == 0 }) {
        println("multiple of four: {value}")
        multiples = multiples + value
    }

    // A chain. The filter sees what the map before it produced, and the second
    // map sees what survived the filter.
    mut shaped = 0
    for value in [1, 2, 3, 4, 5]
        .map(|x: i32| -> i32 { x * x })
        .filter(|x: i32| -> bool { x > 4 })
        .map(|x: i32| -> i32 { x - 1 }) {
        println("shaped: {value}")
        shaped = shaped + value
    }

    // `.map` may change the element type, so the loop binding is whatever the
    // function returns.
    mut mean = 0.0
    for value in [2, 4, 6].map(|x: i32| -> f64 { x as f64 / 2.0 }) {
        mean = mean + value
    }
    println("halves total {mean}")

    // `.enumerate()` stays outermost, and counts what the chain YIELDED — not how
    // many source elements were stepped over to get there.
    for (position, value) in [10, 3, 20, 4, 30].filter(|x: i32| -> bool { x >= 10 }).enumerate() {
        println("kept {position}: {value}")
    }

    // The function is an ordinary expression, evaluated once before the loop, so a
    // closure binding used by a head is still usable afterwards.
    val scale = |x: i32| -> i32 { x * 10 }
    mut scaled = 0
    for value in [1, 2].map(scale) {
        scaled = scaled + value
    }
    println("scaled total {scaled}, one more {scale(3)}")

    return doubled + multiples + shaped + scaled
}
