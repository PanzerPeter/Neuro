// `std::env`: the command line and the environment the program was started with.

// The status the runtime answers for a variable that is not set.
const NOT_SET: i32 = 5

// The command line, the program's own name first. A byte that begins no valid UTF-8 is read
// as U+FFFD.
export func args() -> Vec<string> {
    val count = __arg_count()
    mut all: Vec<string> = Vec::with_capacity(count)
    mut index: u64 = 0
    while index < count {
        all.push(__arg(index))
        index = index + 1
    }
    all
}

// The variable `name`, or `None` when it is not set. Read as `args` reads the command line.
export func var(name: &string) -> Option<string> {
    val (value, status) = __env_var(name)
    if status == NOT_SET {
        return Option::None
    }
    Option::Some(value)
}
