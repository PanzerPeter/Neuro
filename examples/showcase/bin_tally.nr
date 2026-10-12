// Showcase: the derivable traits as bounds, alongside earlier features.
//
// Cumulative integration example combining:
//   `Copy` / `Clone` / `Debug` / `PartialEq` / `Hashable`, derived and hand-written
//   · generic functions bounded by them · a `Vec<T>` built inside a generic body
//   · a `HashMap` keyed by a derived struct · an enum with a hand-written `Debug`
//     rendered through `match` · `for` over an array and a `Vec`
//   · string interpolation with `:?` and a width
//
// The point of the combination: one generic body (`repeat`, `report`) serves a derived
// struct, an enum with its own `Debug`, and a plain integer alike. Each meets the bound
// its own way, and the body cannot tell which.

@derive(Copy, Clone, Debug, PartialEq, Hashable)
struct Bin {
    aisle: u8,
    shelf: u8
}

// A payload of `i32` keeps the enum `Copy`, so it fits `repeat` below as well.
enum Grade {
    Fresh,
    Aged(i32)
}

impl Debug for Grade {
    func fmt_debug(&self, out: &mut StringBuilder) {
        match self {
            Grade::Fresh => out.push_str("fresh"),
            Grade::Aged(days) => out.push_str("aged {days}d")
        }
    }
}

// `T: Copy` lets the body push the same value into every slot.
func repeat<T: Copy>(v: T, n: i32) -> Vec<T> {
    mut out: Vec<T> = Vec::new()
    mut i = 0
    while i < n {
        out.push(v)
        i = i + 1
    }
    out
}

// `T: Debug` lets the body render whatever it is handed.
func report<T: Debug>(what: string, v: &T) {
    println("{what:<6}{v:?}")
}

func main() -> i32 {
    val picks = [
        Bin { aisle: 1, shelf: 2 },
        Bin { aisle: 3, shelf: 1 },
        Bin { aisle: 1, shelf: 2 },
        Bin { aisle: 1, shelf: 2 },
        Bin { aisle: 3, shelf: 1 }
    ]

    // Equal bins built separately are one key: the derived `==` and hash agree.
    mut counts: HashMap<Bin, i32> = HashMap::new()
    for bin in picks {
        val seen = counts.get(bin) ?? 0
        counts.insert(bin, seen + 1)
    }

    val busiest = Bin { aisle: 1, shelf: 2 }
    val hits = counts.get(busiest) ?? 0
    report("bin", &busiest)
    report("hits", &hits)

    val grades = repeat(Grade::Aged(hits), 2)
    for g in grades {
        report("grade", &g)
    }
    val fresh = Grade::Fresh
    report("grade", &fresh)

    val restock = repeat(busiest, hits)
    counts.len() as i32 * 10 + hits + restock.len() as i32 * 10
}
