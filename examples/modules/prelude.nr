// The implicit prelude: what every module gets without writing an `import`. Each module
// begins as if it had written `import std::prelude::{Option, Some, None, Result, Ok, Err,
// println, print}`. This file imports nothing and still names all of them. A module's own
// declaration of `Option` keeps its meaning, and an explicit `import` overrides the
// prelude. A file that wants none of it writes `@no_prelude` (see `no_prelude.nr`).

// `Option` names itself, and so do its variants.

func half(n: i32) -> Option<i32> {
    if n % 2 == 0 {
        Some(n / 2)
    } else {
        None
    }
}

// `Result` and its variants read the same way, and `?` propagates the failure as-is.
func checked_half(n: i32) -> Result<i32, i32> {
    if n < 0 {
        return Err(n)
    }
    match half(n) {
        Some(value) => Ok(value),
        None        => Err(n)
    }
}

func quarter(n: i32) -> Result<i32, i32> {
    val once = checked_half(n)?
    checked_half(once)
}

func main() -> i32 {
    // A pattern names the variants unqualified too.
    val eight = match half(16) {
        Some(value) => value,           // 8
        None        => 0
    }

    // `??` supplies the fallback when the value is absent.
    val fallback = half(7) ?? 3         // 7 is odd -> None -> 3

    val quartered = match quarter(20) {
        Ok(value)  => value,            // 20 -> 10 -> 5
        Err(_)     => 0
    }

    // An odd number stops the chain at its first step, and `?` carries the reason out.
    val stopped = match quarter(9) {
        Ok(value)  => value,
        Err(reason) => reason           // 9
    }

    // `println` needed no import either: it is a compiler builtin rather than a
    // prelude declaration, so even `@no_prelude` leaves it in place.
    println("half(16)     Some -> {eight}")
    println("half(7)      None -> {fallback}")
    println("quarter(20)  Ok   -> {quartered}")
    println("quarter(9)   Err  -> {stopped}")

    val total = eight + fallback + quartered + stopped
    println("total             -> {total}")

    total                                    // 8 + 3 + 5 + 9 = 25
}
