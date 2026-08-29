// Neuro Programming Language - Growable strings (`String`)
// Demonstrates the mutable text buffer that `string` deliberately is not.
//
// `string` is immutable, so building text with `s = s + piece` in a loop
// reallocates and recopies everything accumulated so far on every step. `String`
// is the growable counterpart — one owned heap buffer that appends in amortized
// O(1) — the same relationship `Vec<T>` has to `[T; N]`.
//
// Surface: `String::new()`, `.push_str(text)`, `.len()`, `.clear()`, and
// `.to_string()`, which copies the accumulated bytes back out as an ordinary
// immutable `string`. The buffer is freed when its owner leaves scope.

func main() -> i32 {
    // No annotation is needed: `String` takes no type arguments, so there is
    // nothing to infer.
    mut report = String::new()

    report.push_str("run")
    report.push_str(": ")

    // The appended text is read, never moved, so `status` is still usable after.
    val status: string = "ok"
    report.push_str(&status)
    report.push_str(" (")

    // Appending in a loop is what the buffer exists for — no intermediate
    // string is allocated per step.
    mut i: i32 = 0
    while i < 3 {
        report.push_str("..")
        i = i + 1
    }
    report.push_str(")")

    // `.to_string()` copies the bytes into an owned immutable `string`, which
    // then behaves like any other: it concatenates, compares, and has a length.
    val line: string = report.to_string()
    if line != "run: ok (......)" {
        return 91
    }
    if status.len() != 2 {
        return 92
    }

    // `.clear()` resets the length but keeps the buffer, so refilling the
    // builder does not reallocate.
    val filled: u64 = report.len()
    report.clear()
    if report.len() != 0 {
        return 93
    }
    report.push_str("second pass")

    // 16 + 11 = 27
    return (filled + report.len()) as i32
}
