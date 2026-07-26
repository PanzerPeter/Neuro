// Implicit standard library — compiled into every program.
//
// These declarations are ordinary Neuro source, parsed by the same parser as user
// code and prepended to the program's items. A user declaration of the same name
// replaces the entry here, so a program may define its own `Option` or `Result`.

// The absence-or-presence type: `Option::Some(v)` carries a value, `Option::None`
// carries nothing.
enum Option<T> {
    Some(T),
    None
}

// The success-or-failure type: `Result::Ok(v)` carries the produced value,
// `Result::Err(e)` the reason it failed.
enum Result<T, E> {
    Ok(T),
    Err(E)
}
