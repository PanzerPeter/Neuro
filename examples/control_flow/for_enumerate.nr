// `.enumerate()` in a `for` head: iterate a sequence and its position at once.
//
// The head binds a pair `(index, value)`. The index is a `u64` counting from
// zero — a POSITION, not a value the sequence holds — which is what lets it
// index the very sequence it walks. A range makes the distinction visible: the
// positions of `10..14` are still 0, 1, 2, 3.

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
