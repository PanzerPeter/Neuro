// A range written outside a `for` head is a value: `Range<T>` over its bounds' integer
// type. It binds, passes and returns like any struct, and it is an iterator, so a `for`
// head, the adapter methods and a generic consumer all take it. `.rev()` and `.step(n)`
// return `Rev<T>` and `Step<T>`, walking what the head forms walk.

// A range a function hands back, strided before it leaves.
func evens_below(limit: i32) -> Step<i32> {
    (0..limit).step(2)
}

// Any stream of `i32`, a range included.
func total<I: Iterator<Item = i32>>(values: I) -> i32 {
    mut sum = 0
    for value in values {
        sum = sum + value
    }
    sum
}

func show<I: Iterator<Item = i32>>(label: string, values: I) {
    mut text = ""
    for value in values {
        text = text + "{value} "
    }
    println("{label}{text}")
}

func main() -> i32 {
    val window = 3..7
    show("3..7             = ", window)
    show("evens_below(9)   = ", evens_below(9))
    show("0..=10 down by 3 = ", (0..=10).rev().step(3))

    // The element type follows the bounds: a `u8` range walks up to 255 and stops.
    val top: Range<u8> = 252..=255
    mut bytes = ""
    for b in top {
        bytes = bytes + "{b} "
    }
    println("252..=255 as u8  = {bytes}")

    // `next` takes `&mut self`, so a borrowed head drives the range in place. A `break`
    // leaves it where it stopped, and `.rev()` walks only what is left.
    mut pending = 0..10
    mut first_part = ""
    for i in &mut pending {
        if i == 4 {
            break
        }
        first_part = first_part + "{i} "
    }
    println("walked first     = {first_part}")
    show("rest, newest     = ", pending.rev())

    // The adapters take a range value like any other iterator.
    val squares = (1..=4).map(|n| n * n)
    total(squares)
}
