// Showcase: the codepoint iterators driving a small tokenizer. Integration example:
// `.char_indices()` byte offsets, `.chars()`, zero-copy `.slice(range)` views, a
// `StringBuilder` and `.to_string()`, a `Vec<string>`, a struct with `&self` methods,
// `.enumerate()` over a `Vec`, a `.filter(p)` adapter, and format specs. A scanner decides
// where a token ends by reading characters, but it cuts the token out by bytes because a
// view takes bytes. The two units differ only on non-ASCII text, so the input is
// deliberately not ASCII.

struct Token {
    text: string,
    scalars: i32
}

impl Token {
    // Built from a borrowed view, so the scanner never allocates while it walks; the
    // owned copy is made once, here, when the token is kept.
    func of(view: &string) -> Token {
        mut buffer = StringBuilder::new()
        buffer.push_str(view)
        mut count = 0
        for c in view.chars() {
            count = count + 1
        }
        Token { text: buffer.to_string(), scalars: count }
    }

    // A token's two lengths disagree on any text outside ASCII, which is the whole
    // reason a byte offset and a codepoint count are different things.
    func is_wide(&self) -> bool {
        (self.text.len() as i32) > self.scalars
    }
}

// Split on spaces. The cut points come from `.char_indices()`, so they are byte
// offsets: exactly what `.slice(range)` consumes.
func scan(line: &string) -> Vec<string> {
    mut tokens: Vec<string> = Vec::new()
    mut start: u64 = 0
    mut open = false

    for (offset, c) in line.char_indices() {
        if c == ' ' {
            if open {
                val word = Token::of(line.slice(start..offset))
                tokens.push(word.text)
                open = false
            }
        } else {
            if !open {
                start = offset
                open = true
            }
        }
    }

    // The last word ends at the end of the line rather than at a space.
    if open {
        val word = Token::of(line.slice(start..line.len()))
        tokens.push(word.text)
    }

    tokens
}

func main() -> i32 {
    val line = "héllo wörld from neuro"

    println("line        = {line}")
    println("bytes       = {line.len()}")

    // Codepoint length: what `.len()` is not.
    mut scalars = 0
    for c in line.chars() {
        scalars = scalars + 1
    }
    println("scalars     = {scalars}")

    val words = scan(line.slice(0..line.len()))
    println("words       = {words.len()}")

    mut wide = 0
    mut letters = 0
    for (position, w) in (&words).enumerate() {
        val token = Token::of(w.slice(0..w.len()))
        letters = letters + token.scalars
        if token.is_wide() {
            wide = wide + 1
        }
        println("  {position}: {token.text:>6}  bytes {token.text.len():>2}  scalars {token.scalars:>2}")
    }
    println("wide        = {wide}")

    // A `.filter(p)` head over the scalar stream: the same walk, one adapter deep.
    mut accented = 0
    for c in line.chars().filter(|c: char| (c as u32) > 127) {
        accented = accented + 1
    }
    println("accented    = {accented}")

    val third = &words[2]
    println("third       = {third}")

    val total = letters + wide + accented + (third.len() as i32)
    println("total       = {total}")
    return total                          // 19 + 2 + 2 + 4 = 27
}
