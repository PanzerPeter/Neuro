// Showcase — the codepoint iterators driving a small tokenizer.
//
// Cumulative integration example combining:
//   `.char_indices()` byte offsets and `.chars()` codepoint walks
//   · zero-copy `.slice(range)` views cut at those offsets
//   · the growable `String` buffer and `.to_string()`
//   · a heap-backed `Vec<string>` that frees at scope exit
//   · a struct with `&self` methods and an associated function
//   · `.enumerate()` over a `Vec` head and a `.filter(p)` adapter over the scalars
//   · string interpolation with the format mini-language
//
// The point of the combination: a scanner needs both units at once. It decides where a
// token ends by reading *characters* — a space is a character, and so is 'é' — but it
// cuts the token out by *bytes*, because that is what a zero-copy view takes. Getting
// one of the two wrong is invisible on ASCII and corrupts every accented word, which is
// why the text below is deliberately not ASCII.
//
// Expected exit code: 27

struct Token {
    text: string,
    scalars: i32
}

impl Token {
    // Built from a borrowed view, so the scanner never allocates while it walks; the
    // owned copy is made once, here, when the token is kept.
    func of(view: &string) -> Token {
        mut buffer = String::new()
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
// offsets — exactly what `.slice(range)` consumes.
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
    for (position, w) in words.enumerate() {
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

    val third = words[2]
    println("third       = {third}")

    val total = letters + wide + accented + (third.len() as i32)
    println("total       = {total}")
    return total                          // 19 + 2 + 2 + 4 = 27
}
