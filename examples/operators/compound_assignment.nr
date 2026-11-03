// Compound assignment: `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=` and `<<=`. On an
// integer each one is the plain assignment `x = x OP rhs`. The score is printed after every
// operator, so the desugaring can be checked by reading the output.

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

    // The bitwise forms, on a byte of flags
    mut flags: u8 = 0b0000_0011u8
    flags <<= 2u8
    println("<<= 2   {flags:08b}")
    flags |= 0b1000_0001u8
    println("|= 0x81 {flags:08b}")
    flags &= 0b1111_0001u8
    println("&= 0xF1 {flags:08b}")
    flags ^= 0b1111_1111u8
    println("^= 0xFF {flags:08b}")

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
