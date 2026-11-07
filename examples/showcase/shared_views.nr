// Showcase: references passed where a generic asks for `Copy` or `Clone`, alongside earlier
// features. Integration example: one `Slot<T>` and one `pick` hold both owned records and
// borrows of them. `Run` has a hand-written `clone` that counts itself in the name, so the
// output shows which clones copied a record and which only copied a reference to it. Also
// combines a `@derive(Copy, Clone)` generic struct, a closure parameter, an enum payload and
// `&[T]` slices.

struct Run {
    name: string,
    score: i32
}

impl Clone for Run {
    func clone(&self) -> Run {
        Run { name: "{self.name}'", score: self.score }
    }
}

struct Slot<T> {
    held: T
}

impl<T: Clone> Slot<T> {
    func read(&self) -> T {
        self.held.clone()
    }
}

@derive(Copy, Clone)
struct Pair<T> {
    first: T,
    second: T
}

func pair_of<T: Copy>(v: T) -> Pair<T> {
    Pair { first: v, second: v }
}

enum Pick<T> {
    Found(T),
    Missing
}

// The first element `keep` accepts. With `T = &Run` the result borrows what `items` borrows,
// so nothing is copied out of the records.
func pick<T: Clone>(items: &[T], keep: (&T) -> bool) -> Pick<T> {
    for x in items {
        if keep(x) {
            return Pick::Found(x.clone())
        }
    }
    Pick::Missing
}

func score_of<T: Copy>(pick: Pick<T>, read: (&T) -> i32) -> i32 {
    match pick {
        Pick::Found(x) => read(&x),
        Pick::Missing => 0,
    }
}

func main() -> i32 {
    val ada = Run { name: "ada", score: 31 }
    val lin = Run { name: "lin", score: 12 }

    // An owned slot clones the record; a borrowing one copies the reference.
    val owned = Slot { held: lin.clone() }
    val shared = Slot { held: &ada }
    println("owned {owned.read().name}")
    println("shared {shared.read().name}")

    val pair = pair_of(&ada)
    println("pair {pair.first.score + pair.second.score}")

    val board = [&ada, &lin]
    val hit = pick(&board, |r: &&Run| r.score < 20)
    val miss = pick(&board, |r: &&Run| r.score > 99)
    val top = score_of(hit, |r: &&Run| r.score)
    val absent = score_of(miss, |r: &&Run| r.score)
    println("hit {top} miss {absent}")

    val scores = [4, 8]
    val borrowed = Slot { held: &scores }
    val copied = borrowed.read()
    println("scores {copied[1]}")

    top + pair.first.score
}
