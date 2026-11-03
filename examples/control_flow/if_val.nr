// `if val` and `while val`: a single-pattern test used as a condition. `if val PATTERN =
// value { }` runs its block only on a match, with the pattern's names bound inside. With an
// `else` it yields a value, like `if`. `while val` evaluates `value` again before each pass
// and stops at the first non-match. Both take any `match` pattern. `val-else` handles the
// failing side of a single-pattern test.

enum Reading {
    Celsius(i32),
    Fahrenheit(i32),
    Missing
}

func to_celsius(r: Reading) -> i32 {
    if val Reading::Celsius(c) = r { c }
    else if val Reading::Fahrenheit(f) = r { (f - 32) * 5 / 9 }
    else { 0 }
}

func main() -> i32 {
    val found = Some(21)
    val absent: Option<i32> = None

    if val Some(t) = found {
        println("found {t}")
    }
    if val Some(t) = absent {
        println("never printed {t}")
    } else {
        println("absent has nothing")
    }

    val warm = to_celsius(Reading::Fahrenheit(77))
    val cool = to_celsius(Reading::Celsius(12))
    val none = to_celsius(Reading::Missing)
    println("readings: {warm} {cool} {none}")

    // `pop` is called once per pass; the loop ends at the first `None`.
    mut queue: Vec<i32> = Vec::new()
    for n in 1..=6 {
        queue.push(n)
    }
    mut odd_total = 0
    while val Some(n) = queue.pop() {
        if n % 2 == 0 { continue }
        odd_total += n
    }
    println("odd total: {odd_total}")

    (found ?? 0) + warm + cool + odd_total
}
