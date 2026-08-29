// Showcase — Option / Result working alongside structs, methods, arrays, generics,
// and pattern matching.
//
// Integration example combining several features at once:
//   standard-library `Option<T>` / `Result<T, E>` (generic enums, monomorphized per
//   type argument) · structs with `impl` methods (`&self`) · borrowed struct
//   parameters (`&Sensor`) · fixed-size arrays + `for`-in iteration · a generic
//   function (`choose<T>`) · pattern matching with a guard · compound assignment.
//
// A reading is looked up in a table (absent → `Option::None`), validated (out of range
// → `Result::Err`), and the results are folded into one exit code.

struct Sensor {
    id: i32,
    raw: i32
}

impl Sensor {
    func new(id: i32, raw: i32) -> Sensor {
        Sensor { id: id, raw: raw }
    }

    // Readings are stored at half scale, so calibration doubles them.
    func calibrated(&self) -> i32 {
        self.raw * 2
    }
}

// A generic function reused at two different type arguments below.
func choose<T>(cond: bool, a: T, b: T) -> T {
    if cond { a } else { b }
}

// Absence is an `Option`: the index when the id is present, `None` when it is not.
func index_of(ids: [i32; 4], target: i32) -> Option<i32> {
    mut idx: i32 = 0
    for id in ids {
        if id == target {
            return Option::Some(idx)
        }
        idx += 1
    }
    Option::None
}

// Failure is a `Result`: the calibrated value, or an error code saying what was wrong.
// A borrowed receiver keeps the sensor owned by the caller.
func validate(s: &Sensor) -> Result<i32, i32> {
    val value = s.calibrated()
    if value > 100 {
        Result::Err(2)
    } else {
        Result::Ok(value)
    }
}

func main() -> i32 {
    val ids: [i32; 4] = [11, 12, 13, 14]

    val found = index_of(ids, 13)
    val missing = index_of(ids, 99)

    mut total: i32 = 0

    // Deconstruct both Option instances; the payload binds at the concrete type.
    total += match found {
        Option::Some(i) => i,          // index 2  → 2
        Option::None => 0
    }
    total += match missing {
        Option::Some(i) => i,
        Option::None => 8              // absent   → 8   (total 10)
    }

    val ok = Sensor::new(1, 20)
    val hot = Sensor::new(2, 90)

    // A guard distinguishes a large-but-valid reading from a small one.
    total += match validate(&ok) {
        Result::Ok(v) if v > 30 => v,  // 20 * 2 = 40, so the guard fires → 40
        Result::Ok(v) => v * 2,
        Result::Err(e) => 0 - e
    }
    total += match validate(&hot) {
        Result::Ok(v) => v,
        Result::Err(e) => e            // 180 out of range → 2   (total 52)
    }

    println("index_of / validate running total = {total}")

    // The same generic function at `i32` and at `char`.
    total += choose(total > 50, 1, 0)  // → 1                    (total 53)
    val mark = choose(true, 'y', 'n')
    println("choose<char>(true, 'y', 'n')      = {mark}")
    if mark == 'y' {
        total += 4                     // → 4                    (total 57)
    }

    println("total                             = {total}")
    return total
}
