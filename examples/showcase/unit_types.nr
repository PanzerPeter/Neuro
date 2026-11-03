// Showcase: newtype units of measure, alongside structs, enums, methods, pattern matching
// and arrays. Integration example: `newtype` declarations with `.0` access, structs with
// newtype fields, `impl` methods on `&self`, enums, `match`, a fixed array walked with
// `for`, and compound assignment. `Meters` and `Seconds` are distinct types even though
// both wrap `i32`, so mixing them is a compile error. A plain `type` alias cannot give that
// safety.

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
    val pace1 = leg1.pace()
    val pace2 = leg2.pace()
    println("Leg::new(Meters(30), Seconds(12)).pace() = {pace1}")
    println("Leg \{ Meters(20), Seconds(8) \}.pace()    = {pace2}")
    total += pace1            // 30 - 12 = 18
    total += pace2            // 20 -  8 = 12   -> 30

    // Combine two newtype distances (Meters is Copy, so the fields are not moved).
    val combined: Meters = add_distance(leg1.distance, leg2.distance)  // Meters(50)
    println("add_distance -> Meters({combined.0})")
    total += combined.0       // + 50           -> 80

    // Array + for-in accumulation.
    val splits: [i32; 3] = [1, 2, 3]
    for s in splits {
        total += s            // + 1 + 2 + 3     -> 86
    }

    // Enum + match.
    val hill = terrain_cost(Terrain::Hill)
    val trail = terrain_cost(Terrain::Trail)
    println("terrain_cost(Hill)                       = {hill}")
    println("terrain_cost(Trail)                      = {trail}")
    total += hill             // + 5 -> 91
    total += trail            // + 3 -> 94

    println("total                                    = {total}")
    return total              // 94
}
