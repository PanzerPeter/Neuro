// Showcase — `?` error propagation threaded through earlier features.
//
// Cumulative integration example combining:
//   the `?` operator on both `Result` and `Option`
//   · structs with `&self` methods · @derive(Copy)
//   · fixed-size arrays with `for x in arr` iteration
//   · `match` with enum-variant patterns · `val-else` with an error binding
//   · the `??` fallback · generic `Option` / `Result` from the implicit prelude
//
// The point of the combination: `?` inside a loop body leaves the *whole function*,
// not just the iteration, and the error it carries out is the one the validator
// produced — it survives the loop, the call chain, and the `match` that reads it.
//
// Expected exit code: 177

@derive(Copy, Clone)
struct Reading {
    raw: i32,
    scale: i32
}

impl Reading {
    func value(&self) -> i32 {
        self.raw * self.scale
    }
}

// A sample is valid only in range; the error carries the offending value itself.
func validate(raw: i32) -> Result<i32, i32> {
    if raw < 0 {
        Result::Err(raw)
    } else if raw > 100 {
        Result::Err(raw)
    } else {
        Result::Ok(raw)
    }
}

func scaled(raw: i32, scale: i32) -> Result<i32, i32> {
    val checked = validate(raw)?
    val reading = Reading { raw: checked, scale: scale }
    Result::Ok(reading.value())
}

// The first bad sample ends the whole batch: `?` returns from `batch_total`, not
// from the loop iteration.
func batch_total(samples: [i32; 4]) -> Result<i32, i32> {
    mut total: i32 = 0
    for s in samples {
        total = total + scaled(s, 2)?
    }
    Result::Ok(total)
}

func first_positive(samples: [i32; 4]) -> Option<i32> {
    for s in samples {
        if s > 0 {
            return Option::Some(s)
        }
    }
    Option::None
}

func headline(samples: [i32; 4]) -> Option<i32> {
    val best = first_positive(samples)?
    Option::Some(best * 10)
}

func main() -> i32 {
    val good: [i32; 4] = [1, 2, 3, 4]
    val bad: [i32; 4] = [1, 120, 3, 4]

    val total = batch_total(good) ?? 0        // (1 + 2 + 3 + 4) * 2 = 20

    // The rejected sample travels all the way out of the loop as the Err payload.
    val rejected = match batch_total(bad) {
        Result::Ok(v) => v,
        Result::Err(e) => e                   // 120
    }

    // `val-else` handles the other direction: unwrap here, or leave with the error.
    val Result::Ok(spread) = scaled(9, 3) else |e| { return e }   // 27

    val top = headline(good) ?? 0             // 1 * 10

    println("batch_total(good)  ?? 0     = {total}")
    println("batch_total(bad)   Err      = {rejected}   (? left the for-in loop)")
    println("scaled(9, 3)       val-else = {spread}")
    println("headline(good)     ?? 0     = {top}")

    val sum = total + rejected + spread + top
    println("total                       = {sum}")
    sum
}
