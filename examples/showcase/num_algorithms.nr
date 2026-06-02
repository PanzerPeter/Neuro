// Showcase — a tiny integer-math toolkit.
//
// Integration example combining several control-flow and operator features:
//   recursion · while loops · modulo (%) · compound assignment (+=)
//   · saturating overflow method · nested if/else · early return.
//
// Four classic routines are computed and composed into a single exit code.

// Integer square root via linear scan.
func isqrt(n: i32) -> i32 {
    mut r: i32 = 0
    while (r + 1) * (r + 1) <= n {
        r += 1
    }
    return r
}

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

func main() -> i32 {
    val s = isqrt(144)      // 12
    val g = gcd(48, 36)     // 12
    val p = ipow(2, 10)     // 1024

    // Count primes in 0..=20: {2,3,5,7,11,13,17,19} -> 8.
    mut primes: i32 = 0
    mut k: i32 = 0
    while k <= 20 {
        if is_prime(k) { primes += 1 }
        k += 1
    }

    // Compose, gated on the power result.
    if p == 1024 {
        return s + g + primes   // 12 + 12 + 8 = 32
    }
    return 1
}
