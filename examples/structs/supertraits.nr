// Supertraits: `trait Priced: Named` says every `Priced` type is also `Named`. An impl of
// `Priced` needs an impl of `Named` beside it, in any order. A default method, a `T: Priced`
// bound and a `&dyn Priced` all reach `Named`'s methods.

trait Named {
    func name(&self) -> string
}

trait Priced: Named {
    func cents(&self) -> i32

    // A default body may call the supertrait's method.
    func label(&self) -> string {
        "{self.name()} costs {self.cents()}"
    }
}

// Two supertraits. `Copy` is one like any other: a `T: Stocked` parameter copies, and a
// derive satisfies it as an impl would.
trait Stocked: Priced + Copy {
    func count(&self) -> i32
}

@derive(Copy)
struct Pen {
    cents: i32,
    count: i32
}

impl Stocked for Pen {
    func count(&self) -> i32 {
        self.count
    }
}

impl Priced for Pen {
    func cents(&self) -> i32 {
        self.cents
    }
}

impl Named for Pen {
    func name(&self) -> string {
        "pen"
    }
}

struct Book {
    cents: i32
}

impl Named for Book {
    func name(&self) -> string {
        "book"
    }
}

impl Priced for Book {
    func cents(&self) -> i32 {
        self.cents
    }
}

// A bound on the subtrait reaches its supertraits, all the way up.
func stock_value<T: Stocked>(item: T) -> i32 {
    val copy = item
    copy.cents() * item.count()
}

// A subtrait bound satisfies a supertrait bound.
func name_len<T: Named>(item: &T) -> i32 {
    item.name().len() as i32
}

func priced_name_len<T: Priced>(item: &T) -> i32 {
    name_len(item)
}

// A trait object's table holds the supertrait's methods too.
func show(item: &dyn Priced) {
    println("{item.name()}: {item.label()}")
}

func main() -> i32 {
    val pen = Pen { cents: 8, count: 4 }
    val book = Book { cents: 6 }
    show(&pen)
    show(&book)
    stock_value(pen) + priced_name_len(&pen) + priced_name_len(&book) + book.cents()
}
