// Showcase — a tiny integer-math toolkit.
//
// Integration example combining several control-flow and operator features:
//   recursion · while loops · modulo (%) · compound assignment (+=)
//   · saturating and checked overflow methods · Option + match deconstruction
//   · tuples · loop-as-value · nested if/else · early return
//   · nesting block comments.
//
// Five classic routines are computed and composed into a single exit code.

// Integer square root via linear scan.
func isqrt(n: i32) -> i32 {
    mut r: i32 = 0
    while (r + 1) * (r + 1) <= n {
        r += 1
    }
    return r
}

/* Shelved: Newton's method converges in far fewer steps, but it needs a
   division-by-zero guard the linear scan above does not, so it stays parked
   here until the guard is worth writing. The whole block is inert — including
   the ordinary comment inside it, which is why the outer comment has to nest.

func isqrt_newton(n: i32) -> i32 {
    if n == 0 { return 0 }
    mut r: i32 = n
    loop {
        /* Averaging r and n/r halves the error each round. */
        val next = (r + n / r) / 2
        if next >= r { break }
        r = next
    }
    return r
}
*/

// Greatest common divisor — Euclid's algorithm, recursive.
func gcd(a: i32, b: i32) -> i32 {
    if b == 0 { return a }
    return gcd(b, a % b)
}

// Primality test by trial division.
func is_prime(n: i32) -> bool {
    if n < 2 { return false }
    mut d: i32 = 2
    while d * d <= n {
        if n % d == 0 { return false }
        d += 1
    }
    return true
}

// Integer power using saturating multiply, so it never traps on overflow.
func ipow(base: i32, exp: i32) -> i32 {
    mut acc: i32 = 1
    mut i: i32 = 0
    while i < exp {
        acc = acc.saturating_mul(base)
        i += 1
    }
    return acc
}

// The same power, but reporting overflow rather than clamping it away.
// `checked_mul` yields `Option::None` at the first step whose product does not fit,
// and the loop stops there with the absent answer.
func pow_checked(base: i32, exp: i32) -> Option<i32> {
    mut acc: i32 = 1
    mut i: i32 = 0
    val outcome = loop {
        if i == exp {
            break Option::Some(acc)
        }
        // One match, both halves: the product and whether it fit.
        val step: (i32, bool) = match acc.checked_mul(base) {
            Option::Some(v) => (v, true),
            Option::None => (acc, false)
        }
        if step.1 == false {
            break Option::None
        }
        acc = step.0
        i += 1
    }
    return outcome
}

func main() -> i32 {
    val s = isqrt(144)      // 12
    val g = gcd(48, 36)     // 12
    val p = ipow(2, 10)     // 1024

    // 2^10 fits in an i32; 1000^5 = 10^15 does not, and says so.
    val fits = match pow_checked(2, 10) {
        Option::Some(v) => v,          // 1024
        Option::None => 0
    }
    val refused = match pow_checked(1000, 5) {
        Option::Some(_) => 0,
        Option::None => 1              // 1
    }

    // Count primes in 0..=20: {2,3,5,7,11,13,17,19} -> 8.
    mut primes: i32 = 0
    mut k: i32 = 0
    while k <= 20 {
        if is_prime(k) { primes += 1 }
        k += 1
    }

    // Compose, gated on both power results agreeing.
    if p == 1024 {
        if fits == 1024 {
            return s + g + primes + refused   // 12 + 12 + 8 + 1 = 33
        }
    }
    return 1
}
