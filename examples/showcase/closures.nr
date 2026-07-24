// Showcase — closures and higher-order functions (§3.12).
//
// Integration example combining the new closures with several prior features:
//   · a higher-order function taking a `(i32) -> i32` closure parameter
//   · Copy-by-value capture of an enclosing variable
//   · a `move` closure with a block body and an explicit return type
//   · fixed-size arrays and indexed iteration
//   · a struct method, working alongside closures
//
// The results are composed into a single deterministic exit code.

// Apply `f` to each element of a 4-element array and sum the results.
func map_sum(xs: [i32; 4], f: (i32) -> i32) -> i32 {
    mut total: i32 = 0
    mut i: i32 = 0
    while i < 4 {
        total += f(xs[i])
        i += 1
    }
    return total
}

struct Scaler {
    factor: i32
}

impl Scaler {
    func apply(&self, x: i32) -> i32 {
        x * self.factor
    }
}

func main() -> i32 {
    val data: [i32; 4] = [1, 2, 3, 4]

    // A closure capturing a Copy local (`bias`) by value.
    val bias = 10
    val biased = map_sum(data, |x: i32| x + bias)   // 11+12+13+14 = 50

    // A `move` closure with a block body and early return.
    val scale = 3
    val scaled = map_sum(data, move |x: i32| -> i32 {
        val y = x * scale
        return y
    })                                              // 3+6+9+12 = 30

    // A struct method still resolves alongside closures.
    val s = Scaler { factor: 2 }
    val doubled = s.apply(5)                         // 10

    biased + scaled + doubled                        // 50 + 30 + 10 = 90
}
