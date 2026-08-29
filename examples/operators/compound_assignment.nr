// Demonstrates compound assignment operators: +=, -=, *=, /=, %=
// Each operator desugars to a plain assignment at parse time.
//
// The score is printed after every operator so the desugaring is checkable by
// reading the output rather than by tracing the arithmetic.

func main() -> i32 {
    mut score: i32 = 100
    println("start   {score}")

    score += 50
    println("+= 50   {score}")
    score -= 25
    println("-= 25   {score}")
    score *= 2
    println("*= 2    {score}")
    score /= 5
    println("/= 5    {score}")
    score %= 13
    println("%= 13   {score}")

    // Typical loop accumulator pattern
    mut sum: i32 = 0
    mut i: i32 = 1
    while i <= 4 {
        sum += i
        i += 1
    }
    // sum = 1+2+3+4 = 10
    println("loop    {sum}")

    val total = score + sum
    println("total   {total}")

    return total  // 11 + 10 = 21
}
