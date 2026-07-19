// Showcase — newtype units of measure working alongside structs, enums, methods,
// pattern matching, and arrays.
//
// Integration example combining several Phase-1E features:
//   newtype declarations (distinct nominal wrappers constructed as
//   `Name(value)` with `.0` inner access) · structs with newtype fields ·
//   impl methods (&self) · enums · pattern matching ·
//   fixed-size arrays + for-in iteration · compound assignment.
//
// Newtypes make `Meters` and `Seconds` distinct types even though both wrap `i32`,
// so mixing them up is a compile error — the safety a plain `type` alias cannot
// give. Here they flow through a struct, methods, and functions, and their inner
// values are combined into a single numeric result.

newtype Meters = i32
newtype Seconds = i32

struct Leg {
    distance: Meters,
    time: Seconds
}

impl Leg {
    func new(d: Meters, t: Seconds) -> Leg {
        Leg { distance: d, time: t }
    }

    // A toy pace metric reading the inner `i32` out of each newtype field.
    func pace(&self) -> i32 {
        self.distance.0 - self.time.0
    }
}

enum Terrain { Flat, Hill, Trail }

func terrain_cost(t: Terrain) -> i32 {
    match t {
        Terrain::Flat => 0,
        Terrain::Hill => 5,
        Terrain::Trail => 3
    }
}

// Both arguments share the `Meters` type, so they add cleanly; a `Seconds` here
// would be rejected at compile time.
func add_distance(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}

func main() -> i32 {
    val leg1 = Leg::new(Meters(30), Seconds(12))
    val leg2 = Leg { distance: Meters(20), time: Seconds(8) }

    mut total: i32 = 0
    total += leg1.pace()      // 30 - 12 = 18
    total += leg2.pace()      // 20 -  8 = 12   -> 30

    // Combine two newtype distances (Meters is Copy, so the fields are not moved).
    val combined: Meters = add_distance(leg1.distance, leg2.distance)  // Meters(50)
    total += combined.0       // + 50           -> 80

    // Array + for-in accumulation.
    val splits: [i32; 3] = [1, 2, 3]
    for s in splits {
        total += s            // + 1 + 2 + 3     -> 86
    }

    // Enum + match.
    total += terrain_cost(Terrain::Hill)   // + 5 -> 91
    total += terrain_cost(Terrain::Trail)  // + 3 -> 94

    return total              // 94
}
