func abs(n: i32) -> i32 {
    val result = if n >= 0 { n } else { 0 - n }
    result
}

func clamp(n: i32, lo: i32, hi: i32) -> i32 {
    val clamped = if n < lo { lo } else if n > hi { hi } else { n }
    clamped
}

func main() -> i32 {
    val a = abs(0 - 7)

    val area = {
        val w: i32 = 6
        val h: i32 = 7
        w * h
    }

    val b = clamp(150, 0, 100)

    a + area + b
}
