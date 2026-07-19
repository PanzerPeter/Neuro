// Type aliases: `type` introduces a transparent name for an existing
// type. The alias and its target are interchangeable — no new nominal type is
// created. Aliases here stand in for primitive and struct types across field,
// parameter, return, local-annotation, and cast positions.

type Meters = f64
type Celsius = f64
type Id = i64

struct Sensor {
    location: Meters,
    reading: Celsius
}

impl Sensor {
    func new(location: Meters, reading: Celsius) -> Sensor {
        Sensor { location: location, reading: reading }
    }

    // Return type uses the alias; the method yields a plain f64.
    func depth(&self) -> Meters {
        self.location
    }
}

func to_id(raw: i32) -> Id {
    raw as Id
}

func main() -> i32 {
    val s = Sensor::new(10.0, 21.5)
    val d: Meters = s.depth()        // 10.0
    val tag: Id = to_id(7)           // 7

    val total: i64 = tag + (d as i64)  // 7 + 10 = 17
    return total as i32
}
