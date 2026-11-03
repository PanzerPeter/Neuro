// `.len()` on `string`: an intrinsic method on a builtin type. It reads the byte length
// stored in the fat pointer `(ptr, len)`, so it is O(1) and returns a `u64`. The length
// excludes any null terminator.

func main() -> i32 {
    val greeting: string = "hello, world"

    // 12 bytes: "hello, world" has no multi-byte code points.
    val n: u64 = greeting.len()

    // The empty string has length 0.
    val empty: string = ""
    val zero: u64 = empty.len()

    println("{greeting} -> len {n}")
    println("empty string -> len {zero}")

    if n == 12u64 {
        if zero == 0u64 {
            return 0
        }
    }
    return 1
}
