// `.lines()` and `.split(sep)`: the pieces of a text as `&string` views into it, so
// splitting copies nothing. A line ends at `\n` and takes a `\r` before it along, and a
// final newline opens no empty line. `.split` keeps every piece, empty ones included.

// A piece borrows the text, not the iterator, so it can be returned from the walk.
func field(row: &string, index: i32) -> &string {
    mut at = 0
    for cell in row.split(",") {
        if at == index {
            return cell
        }
        at += 1
    }
    ""
}

func main() -> i32 {
    val table = "name,qty\r\napples,3\npears,\nplums,4\n"
    mut total = 0
    mut rows = 0
    for line in table.lines() {
        rows += 1
        if rows == 1 {
            continue
        }
        val name = field(line, 0)
        val qty = field(line, 1)
        match qty.parse::<i32>() {
            Ok(n) => {
                println("{name}: {n}")
                total += n
            }
            Err(e) => println("{name}: no quantity ({e:?})")
        }
    }

    // Empty pieces count, so a text with three separators has four fields.
    mut fields = 0
    for field in "a,,b,".split(",") {
        fields += 1
    }
    println("fields: {fields}")

    // A separator may be longer than one byte, and need not be ASCII.
    for part in "x → y → z".split(" → ") {
        print("[{part}]")
    }
    println("")

    total * 10 + fields
}
