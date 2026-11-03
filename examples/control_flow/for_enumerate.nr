// `.enumerate()` in a `for` head: iterate a sequence and its position at once. The index is
// a `u64` position counting from zero, not a value the sequence holds, so it can index the
// sequence it walks. Over a range, `10..14` still yields positions 0, 1, 2, 3.

func label_scores(scores: [i32; 4]) -> i32 {
    mut weighted: i32 = 0

    for (place, score) in scores.enumerate() {
        println("place {place}: {score} (also readable as scores[{place}] = {scores[place]})")
        weighted = weighted + (place as i32) * score
    }

    return weighted
}

func main() -> i32 {
    val scores: [i32; 4] = [5, 4, 3, 2]
    val weighted = label_scores(scores)
    println("weighted total = {weighted}")

    // A range needs parentheses: `..` binds looser than a method call, so
    // `10..14.enumerate()` would enumerate the bound rather than the range.
    mut offsets: i32 = 0
    for (step, value) in (10..14).enumerate() {
        println("step {step} reaches {value}")
        offsets = offsets + (value - (step as i32))
    }
    println("value minus position, summed = {offsets}")

    return weighted + offsets
}
