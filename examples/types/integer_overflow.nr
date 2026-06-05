// Neuro Example: Integer overflow semantics (§1.2)
//
// Debug builds (compiled with -O0) trap at runtime when a `+`, `-`, or `*`
// on an integer type overflows, turning a silent miscalculation into an
// immediate abort. Release builds (-O1..-O3) wrap using two's complement,
// matching hardware behavior with zero overhead.
//
// This program stays within range, so it returns the same value (55) in both
// debug and release builds. Push any of these sums past the type's maximum to
// observe the debug-build trap.

func main() -> i32 {
    mut acc: i32 = 0
    for i in 1..=10 {
        acc = acc + i        // 1+2+...+10 = 55, well within i32 range
    }
    return acc
}
