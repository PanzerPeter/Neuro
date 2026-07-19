// Newtype declarations: `newtype` creates a DISTINCT nominal type that
// wraps an inner type. Unlike a transparent `type` alias, a newtype and its inner
// type are not interchangeable — `Meters` and `i32` are different types. Construct
// a newtype with `Name(value)`; read the wrapped value back with `.0`.

newtype Meters = i32
newtype Seconds = i32

struct Trip {
    distance: Meters,
    duration: Seconds
}

// Newtypes cross function boundaries as parameters and return types. Because the
// two arguments share the `Meters` type they add cleanly; a `Seconds` here would
// be a type error — that is the safety a newtype buys over a plain alias.
func add_distance(a: Meters, b: Meters) -> Meters {
    Meters(a.0 + b.0)
}

func trip_score(t: &Trip) -> i32 {
    // `.0` reads the inner `i32` out of each newtype field (through the borrow).
    t.distance.0 + t.duration.0
}

func main() -> i32 {
    val total: Meters = add_distance(Meters(10), Meters(20))   // Meters(30)
    val trip = Trip { distance: total, duration: Seconds(12) }
    trip_score(&trip)                                          // 30 + 12 = 42
}
