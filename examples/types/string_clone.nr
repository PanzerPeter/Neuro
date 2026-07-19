// Neuro Programming Language - String .clone() (Phase 1.7)
// Demonstrates the explicit deep-copy method on a non-Copy owned string.
//
// `.clone()` returns a fresh `string` equal to its receiver. It is the canonical
// opt-out of move-by-default for non-Copy types. Today strings are immutable and
// rodata-backed, so a clone copies the (ptr, len) fat pointer; when heap-backed
// strings land, it will duplicate the underlying buffer.

func main() -> i32 {
    val original: string = "neuro"
    val copy: string = original.clone()

    // The clone is byte-equal to the source.
    if original == copy {
        // `.clone()` yields a `string`, so builtin methods chain off it directly.
        val n: u64 = "hello".clone().len()
        return n as i32   // 5
    }

    return 0
}
