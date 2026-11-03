// Growable strings: `StringBuilder`, the mutable buffer that `string` deliberately is not.
// `s = s + piece` in a loop recopies everything built so far on each step. `StringBuilder`
// appends in amortized O(1) into one owned buffer, the way `Vec<T>` relates to `[T; N]`.
// Its surface is `new`, `push_str`, `len`, `clear` and `to_string`, which copies the bytes
// out as an ordinary `string`.

func main() -> i32 {
    // No annotation is needed: `StringBuilder` takes no type arguments, so there is
    // nothing to infer.
    mut report = StringBuilder::new()

    report.push_str("run")
    report.push_str(": ")

    // The appended text is read, never moved, so `status` is still usable after.
    val status: string = "ok"
    report.push_str(&status)
    report.push_str(" (")

    // Appending in a loop is what the buffer exists for: no intermediate
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
    println("built: {line}")
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
    val emptied: u64 = report.len()
    println("len before clear: {filled}")
    println("len after clear:  {emptied}")
    if report.len() != 0 {
        return 93
    }
    report.push_str("second pass")

    val refilled: string = report.to_string()
    println("refilled: {refilled}")

    // 16 + 11 = 27
    val total = (filled + report.len()) as i32
    println("total len:        {total}")
    return total
}
