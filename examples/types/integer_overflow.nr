// Integer overflow semantics. Debug builds (`-O0`) trap when `+`, `-` or `*` overflows an
// integer, which turns a silent miscalculation into an abort. Release builds (`-O1` to
// `-O3`) wrap in two's complement. This program stays in range, so it returns 55 in both;
// push a sum past the type's maximum to see the trap.

func main() -> i32 {
    mut acc: i32 = 0
    for i in 1..=10 {
        acc = acc + i        // 1+2+...+10 = 55, well within i32 range
        println("+ {i} -> {acc}")
    }
    println("total (same in debug and release) = {acc}")
    return acc
}
