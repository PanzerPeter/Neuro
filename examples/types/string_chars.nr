// Neuro Programming Language - Codepoint iterators `.chars()` / `.char_indices()` (Phase 2A)
//
// `.chars()` hands out an iterator over Unicode scalar values. Each step is O(1): the
// cursor reads the code point standing at its byte offset and advances by that code
// point's own UTF-8 width, so nothing is re-scanned. It is an ordinary `Iterator`, so a
// `for` head drives it, `.enumerate()` numbers the scalars, and the iterator itself can
// be held and stepped by hand.
//
// `.char_indices()` walks the same text but binds the *byte* offset of each scalar
// alongside it. That is the offset `.slice(range)` takes, which is what makes the pair
// the tokenizer's tool: find a position by reading characters, then cut by bytes.

func main() -> i32 {
    val text = "aé漢🎉"        // four scalars, one per UTF-8 width: 1 + 2 + 3 + 4 bytes

    println("text        = {text}")
    println("text.len()  = {text.len()}   (bytes, not characters)")

    // A codepoint walk. The count differs from the byte length on any non-ASCII text.
    mut scalars = 0
    for c in text.chars() {
        scalars = scalars + 1
    }
    println("scalars     = {scalars}")

    // `.enumerate()` numbers the code points; `.char_indices()` gives their byte
    // offsets. On this text the two disagree everywhere but the first step.
    for (position, c) in text.chars().enumerate() {
        println("  #{position} {c}")
    }
    for (offset, c) in text.char_indices() {
        println("  @{offset} {c} code {c as u32}")
    }

    // The offsets are byte offsets, so they feed straight back into `.slice(range)`.
    mut cut: u64 = 0
    for (offset, c) in text.char_indices() {
        if c == '漢' { cut = offset }
    }
    val tail = text.slice(cut..text.len())
    println("tail        = {tail}")

    // The iterator is a value: hold it, step it, and read its end as `Option::None`.
    mut walk = "ok".chars()
    val first = walk.next() ?? '?'
    val second = walk.next() ?? '?'
    val past_end = walk.next() ?? '!'
    println("stepped     = {first}{second}{past_end}")

    mut score: i32 = 0
    if scalars == 4 { score = score + 1 }
    if text.len() == 10 { score = score + 2 }
    if cut == 3 { score = score + 4 }
    if tail == "漢🎉" { score = score + 8 }
    if past_end == '!' { score = score + 16 }

    println("score       = {score}")
    return score                              // 1 + 2 + 4 + 8 + 16 = 31
}
