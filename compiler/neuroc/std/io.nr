// `std::io`: standard input. Standard output is `print` and `println`, in the prelude.

// The status the runtime answers at the end of input.
const END_OF_INPUT: i32 = 5

// The next line of standard input without its terminator, or `None` once input is exhausted.
// Buffered standard output is drained first, so a prompt is on the screen while this waits.
export func read_line() -> Result<Option<string>, IoError> {
    val (line, status) = __read_line()
    if status == 0 {
        return Result::Ok(Option::Some(line))
    }
    if status == END_OF_INPUT {
        return Result::Ok(Option::None)
    }
    Result::Err(failure(status, "cannot read standard input: {line}"))
}

// The runtime numbers its failures from 1 in `IoErrorKind`'s declaration order. See the same
// function in `std::fs`.
func failure(status: i32, message: string) -> IoError {
    val kind = match status {
        1 => IoErrorKind::NotFound,
        2 => IoErrorKind::PermissionDenied,
        3 => IoErrorKind::InvalidData,
        _ => IoErrorKind::Other
    }
    IoError { kind: kind, message: message }
}
