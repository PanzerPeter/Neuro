// char primitive type
//
// A `char` is a single Unicode scalar value (32-bit). Char literals are written
// with single quotes and support escapes (`'\n'`, `'\u{1F44D}'`). `char` is
// Copy, has a built-in total order, and `as`-casts to/from integer types — but
// has no arithmetic (compute on the integer code point instead).

func main() -> i32 {
    val letter: char = 'A'
    val newline: char = '\n'
    val emoji: char = '\u{1F44D}'   // thumbs-up, code point 0x1F44D

    // char is Copy: the source binding stays valid after a bind.
    val copy = letter
    val same: bool = letter == copy

    // Built-in ordering on code points.
    val ordered: bool = 'a' < 'b'

    // as-cast char -> integer and integer -> char (round-trips).
    val code: i32 = letter as i32            // 65
    val back: char = code as char            // 'A'
    val round_trips: bool = back == letter

    // u8 code point widens into a char.
    val nl_code: u8 = 10
    val nl: char = nl_code as char
    val nl_ok: bool = nl == newline

    // A `char` renders in a hole as the character itself.
    println("letter          = {letter}")
    println("emoji           = {emoji}")
    println("letter as i32   = {code}")
    println("code as char    = {back}")
    println("newline code    = {nl_code}")
    println("copy == letter  = {same}")
    println("'a' < 'b'       = {ordered}")
    println("round-trips     = {round_trips}")
    println("u8 -> char      = {nl_ok}")

    if same && ordered && round_trips && nl_ok && emoji == '\u{1F44D}' {
        return code        // 65 on success
    }
    return 1
}
