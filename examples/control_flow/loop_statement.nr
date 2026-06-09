// The `loop { ... }` infinite-loop statement (§3.7).
//
// `loop` is the canonical infinite loop — the form the `prefer-loop-over-
// while-true` lint suggests in place of `while true { ... }`. It has no
// condition: the only way out is `break`. `continue` re-enters the body
// from the top.
//
// This program sums 1 + 2 + 4 + 5 (skipping 3 via `continue`) and stops
// once the counter passes 5 via `break`, returning 12.

func main() -> i32 {
    mut i: i32 = 0
    mut sum: i32 = 0

    loop {
        i = i + 1
        if i == 3 {
            continue
        }
        if i > 5 {
            break
        }
        sum = sum + i
    }

    return sum
}
