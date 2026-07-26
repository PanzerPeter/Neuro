// Option<T> and Result<T, E> — the standard-library enums for absence and failure.
//
// Both are ordinary generic enums, available in every program without a declaration.
// A generic enum is monomorphized per set of type arguments, so `Option<i32>` and
// `Option<i64>` are distinct types with their own payload widths and zero runtime cost.

// `Option<T>`: a value that may be absent. Deconstructed with `match`.
func unwrap_or(o: Option<i32>, fallback: i32) -> i32 {
    match o {
        Option::Some(v) => v,
        Option::None => fallback
    }
}

// `Result<T, E>`: a computation that may fail, carrying why. Both branches build the
// declared return instance, which is what fixes their type arguments.
func divide(a: i32, b: i32) -> Result<i32, i32> {
    if b == 0 {
        Result::Err(1)
    } else {
        Result::Ok(a / b)
    }
}

// A user-declared generic enum works the same way — nothing about Option is special.
enum Slot<T> { Filled(T), Vacant }

func slot_or(s: Slot<i32>, fallback: i32) -> i32 {
    match s {
        Slot::Filled(v) => v,
        Slot::Vacant => fallback
    }
}

func main() -> i32 {
    // The type argument is inferred from the payload.
    val present = Option::Some(20)
    // A unit variant carries nothing, so its type comes from the annotation.
    val absent: Option<i32> = Option::None

    mut total: i32 = 0
    total = total + unwrap_or(present, 0)      // + 20 = 20
    total = total + unwrap_or(absent, 5)       // +  5 = 25

    total = total + match divide(30, 3) {
        Result::Ok(v) => v,                    // + 10 = 35
        Result::Err(e) => 0 - e
    }
    total = total + match divide(4, 0) {
        Result::Ok(v) => v,
        Result::Err(e) => e * 2                // +  2 = 37
    }

    // A second instance of the same template: `Option<char>` has its own payload type.
    val letter: Option<char> = Option::Some('n')
    val c = match letter {
        Option::Some(ch) => ch,
        Option::None => 'z'
    }
    if c == 'n' {
        total = total + 3                      // +  3 = 40
    }

    total = total + slot_or(Slot::Filled(2), 0)  // + 2 = 42
    total = total + slot_or(Slot::Vacant, 0)     // + 0 = 42

    return total
}
