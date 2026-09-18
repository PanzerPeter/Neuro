// Showcase: non-`Copy` values held inside aggregates, and the ownership that follows
// them there.
//
// Cumulative integration example combining:
//   arrays, tuples, enum payloads and newtypes holding an owned `string`
//   · a generic struct instantiated over a non-`Copy` type argument
//   · pattern matching over enum payloads that are not scalars
//   · destructuring a struct, a tuple, and an array element by element
//   · a consuming `self` method handing back two owned fields at once
//   · `Vec<string>` and the `Option<string>` its `.pop()` returns
//   · string interpolation and `println`
//
// The rule the whole program leans on: an aggregate holding an owner IS an owner. It
// moves rather than copies, it gives up one element at a time, and once any element has
// left it can no longer be read as a whole.

// A newtype over `string` forwards the inner type's ownership along with its name.
newtype Label = string

// A generic struct holds whatever it is instantiated over, `Copy` or not.
struct Cell<T> {
    value: T,
}

impl<T> Cell<T> {
    func into_value(self) -> T {
        self.value
    }
}

struct Point {
    x: i32,
    y: i32,
}

// Payloads are no longer restricted to scalars: a variant may carry a string, a struct,
// or a whole array of strings.
enum Field {
    Text(string),
    At(Point),
    Row([string; 2]),
    Blank,
}

struct Pair {
    head: string,
    tail: string,
}

impl Pair {
    // Two owned fields leave the receiver in one call, because each field is moved on
    // its own path rather than collapsing onto `self`.
    func into_parts(self) -> (string, string) {
        val head = self.head
        val tail = self.tail
        (head, tail)
    }
}

// A tuple carries an owned string out of a function by value.
func annotate(text: string) -> (string, i32) {
    val width = text.len() as i32
    (text, width)
}

func weigh(field: Field) -> i32 {
    match field {
        Field::Text(t) => t.len() as i32
        Field::At(p) => p.x + p.y
        Field::Row(cells) => (cells[0].len() + cells[1].len()) as i32
        Field::Blank => 0
    }
}

func main() -> i32 {
    // An array of owned strings. Reading one element out moves that element only.
    val headers: [string; 3] = ["name", "shape", "owner"]
    println("headers: {headers[0]}, {headers[1]}, {headers[2]}")

    val fields: [Field; 4] = [
        Field::Text("ledger"),
        Field::At(Point { x: 3, y: 4 }),
        Field::Row(["alpha", "bravo"]),
        Field::Blank
    ]

    mut total = 0
    for field in fields {
        total += weigh(field)
    }
    println("field weight total = {total}")

    // Destructuring: a struct, a tuple, and an array, each split element by element.
    val Pair { head, tail } = Pair { head: "left", tail: "right" }
    println("pair fields      = {head} / {tail}")

    val (annotated, width) = annotate("annotated")
    println("annotated        = {annotated} ({width})")

    val [first, second] = ["one", "two"]
    println("array pattern    = {first} {second}")

    // A consuming method handing back both owned halves.
    val (h, t) = Pair { head: "front", tail: "back" }.into_parts()
    println("into_parts       = {h} -> {t}")

    // A generic struct over a non-`Copy` argument, unwrapped by a consuming method.
    val held = Cell { value: "held by a generic" }
    val unwrapped = held.into_value()
    println("generic cell     = {unwrapped}")

    // A newtype wrapping an owned string.
    val label = Label("ledger-2026")
    println("label            = {label.0}")

    // A collection of owned strings, and the `Option<string>` its reader hands back.
    mut words: Vec<string> = Vec::new()
    words.push("first")
    words.push("second")
    words.push("third")
    match words.pop() {
        Option::Some(last) => println("popped           = {last}")
        Option::None => println("popped           = <empty>")
    }
    println("words remaining  = {words.len()}")

    total + width + (words.len() as i32)
}
