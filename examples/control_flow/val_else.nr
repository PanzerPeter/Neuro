// `val-else` — unwrap a pattern or leave the scope.
//
// `val PATTERN = value else { ... }` binds the pattern's names for the REST of the
// enclosing block, not just one arm. The `else` branch handles the failure and must
// exit the scope (`return`, `break`, `continue`, `panic(...)`, `unreachable()`), so
// the binding is always initialized on the path that continues.
//
// Writing `else |name|` binds the value the failure carried. What `name` is depends on
// the scrutinee's type:
//
//   Result<T, E>   the `Err` payload
//   Option<T>      nothing — `None` is empty, so only `else |_|` (or plain `else`)
//   any other enum the original scrutinee, unmodified, for a nested `match`
//
// Every form is exercised twice — once binding, once failing — and both results
// are printed, so which side of each `val-else` ran is visible in the output.

enum Shape {
    Circle { radius: i32 },
    Square(i32),
    Empty
}

func parse(raw: i32) -> Result<i32, i32> {
    if raw > 0 {
        Result::Ok(raw * 2)
    } else {
        Result::Err(9)
    }
}

func lookup(key: i32) -> Option<i32> {
    if key == 1 {
        Option::Some(6)
    } else {
        Option::None
    }
}

// Result: `|err|` names the `Err` payload, forwarded as this function's answer.
func doubled_or_error(raw: i32) -> i32 {
    val Result::Ok(value) = parse(raw) else |err| { return err }
    // `value` is in scope from here to the end of the function.
    value + 1
}

// Option: `None` carries nothing, so the branch simply returns a default.
func reading_or_default(key: i32) -> i32 {
    val Option::Some(value) = lookup(key) else { return 4 }
    value
}

// A plain enum: `|other|` is the untouched `Shape`, discriminated by a nested match.
func area(s: Shape) -> i32 {
    val Shape::Circle { radius } = s else |other| {
        match other {
            Shape::Square(side) => { return side * side },
            _ => { return 0 }
        }
    }
    radius * 3
}

// `break` also ends the scope, which is what makes `val-else` the natural drain loop.
func next(i: i32, limit: i32) -> Option<i32> {
    if i < limit {
        Option::Some(i + 1)
    } else {
        Option::None
    }
}

func drain(limit: i32) -> i32 {
    mut total: i32 = 0
    mut i: i32 = 0
    loop {
        val Option::Some(v) = next(i, limit) else { break }
        total = total + v
        i = i + 1
    }
    total
}

func main() -> i32 {
    val ok = doubled_or_error(5)        // 11
    val failed = doubled_or_error(-1)   // 9
    println("Result  bound       = {ok}")
    println("Result  else |err|  = {failed}")

    val present = reading_or_default(1) // 6
    val absent = reading_or_default(2)  // 4
    println("Option  bound       = {present}")
    println("Option  else        = {absent}")

    val circle = area(Shape::Circle { radius: 4 })  // 12
    val square = area(Shape::Square(3))             // 9
    val empty = area(Shape::Empty)                  // 0
    println("Shape   bound       = {circle}")
    println("Shape   else |other| Square -> {square}")
    println("Shape   else |other| Empty  -> {empty}")

    val drained = drain(4)              // 1 + 2 + 3 + 4 = 10
    println("loop    else break  = {drained}")

    val total = ok + failed + present + absent + circle + square + empty + drained
    println("total               = {total}")

    total  // 61
}
