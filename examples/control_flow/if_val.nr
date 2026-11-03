// `if val` and `while val`: a single-pattern test as a condition.
//
// `if val PATTERN = value { ... }` runs its block only when `value` matches, with the
// pattern's names bound inside that block. With an `else` it is a value, like `if`;
// without one it is a statement. Links chain freely with plain `else if` conditions.
//
// `while val PATTERN = value { ... }` evaluates `value` again before every pass and
// stops at the first one that does not match. It is a `while`: `break` and `continue`
// work, labels too, and a `break` carries no value.
//
// Both take any pattern a `match` arm takes. `val-else` is the complement: it handles
// the failing side and keeps the binding for the rest of the block.

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
