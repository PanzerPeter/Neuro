// Derivable traits, by derive and by hand
//
// `Copy`, `Clone`, `Debug`, `PartialEq` and `Hashable` are ordinary traits. `@derive`
// writes the impl field by field, or a struct writes it itself. Either way the type
// meets a bound that names the trait. The scalars and `string` meet these bounds too,
// so one generic body runs over all of them.

@derive(Copy, Clone, Debug, PartialEq, Hashable)
struct Tile {
    row: i32,
    col: i32
}

// A hand-written `Debug` chooses its own rendering, and `{x:?}` calls it.
struct Money {
    cents: i64
}

impl Debug for Money {
    func fmt_debug(&self, out: &mut StringBuilder) {
        out.push_str("${self.cents / 100}.{self.cents % 100:02}")
    }
}

// `Copy` written by hand is held to what `@derive(Copy)` accepts: every field `Copy`,
// and `Clone` implemented too.
impl Clone for Money {
    func clone(&self) -> Self {
        Money { cents: self.cents }
    }
}

impl Copy for Money {}

// `T: Copy` lets the body use `v` twice. Without the bound the second use is a
// use of a moved value.
func pair<T: Copy>(v: T) -> (T, T) {
    (v, v)
}

// `T: Debug` lets the body render `v`, however its type renders.
func label<T: Debug>(name: string, v: &T) -> string {
    "{name} = {v:?}"
}

func main() -> i32 {
    val (a, b) = pair(Tile { row: 2, col: 3 })
    val (p, q) = pair(Money { cents: 1250 })
    val count = 7
    println(label("tile", &a))
    println(label("price", &q))
    println(label("count", &count))

    // A struct that derives `PartialEq` and `Hashable` keys a map with no impls.
    mut seen: HashMap<Tile, i32> = HashMap::new()
    seen.insert(a, 1)
    seen.insert(Tile { row: 2, col: 3 }, 2)
    seen.insert(Tile { row: 0, col: 0 }, 3)
    println("{seen.len()} tiles; equal tiles hash alike: {a.hash() == b.hash()}")

    seen.len() as i32 * 10 + (seen.get(b) ?? 0) + p.clone().cents as i32 / 625
}
