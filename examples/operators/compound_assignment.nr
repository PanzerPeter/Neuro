// Demonstrates compound assignment operators: +=, -=, *=, /=, %=
// Each operator desugars to a plain assignment at parse time.

func main() -> i32 {
    mut score: i32 = 100

    score += 50   // score = 150
    score -= 25   // score = 125
    score *= 2    // score = 250
    score /= 5    // score = 50
    score %= 13   // score = 11

    // Typical loop accumulator pattern
    mut sum: i32 = 0
    mut i: i32 = 1
    while i <= 4 {
        sum += i
        i += 1
    }
    // sum = 1+2+3+4 = 10

    return score + sum  // 11 + 10 = 21
}
