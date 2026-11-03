// `.clone()` on a non-`Copy` owned `string`: the explicit deep copy. `.clone()` returns a
// fresh `string` with the same bytes, in a buffer of its own. It is the opt-out from
// move-by-default, so the source and the copy are independent owners.

func main() -> i32 {
    val original: string = "neuro"
    val copy: string = original.clone()

    val equal = original == copy
    println("original       = {original}")
    println("clone          = {copy}")
    println("byte-equal     = {equal}")

    // The clone is byte-equal to the source.
    if original == copy {
        // `.clone()` yields a `string`, so builtin methods chain off it directly.
        val n: u64 = "hello".clone().len()
        println("chained .len() = {n}")
        return n as i32   // 5
    }

    return 0
}
