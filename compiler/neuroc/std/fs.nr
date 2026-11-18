// `std::fs`: whole files, read and written in one call.
//
// Each call reaches the operating-system runtime through an intrinsic only the library may
// name. The runtime answers with a status, 0 on success, and either the data or the system's
// reason for the failure.

export func read_to_string(path: &string) -> Result<string, IoError> {
    val (text, status) = __read_text(path)
    if status != 0 {
        return Result::Err(failure(status, "cannot read {path}: {text}"))
    }
    Result::Ok(text)
}

export func read_bytes(path: &string) -> Result<Vec<u8>, IoError> {
    val (bytes, reason, status) = __read_bytes(path)
    if status != 0 {
        return Result::Err(failure(status, "cannot read {path}: {reason}"))
    }
    Result::Ok(bytes)
}

// Creates the file, or replaces what it held.
export func write(path: &string, text: &string) -> Result<(), IoError> {
    val (reason, status) = __write_text(path, text)
    if status != 0 {
        return Result::Err(failure(status, "cannot write {path}: {reason}"))
    }
    Result::Ok(())
}

export func write_bytes(path: &string, bytes: &[u8]) -> Result<(), IoError> {
    val (reason, status) = __write_bytes(path, bytes)
    if status != 0 {
        return Result::Err(failure(status, "cannot write {path}: {reason}"))
    }
    Result::Ok(())
}

export func remove_file(path: &string) -> Result<(), IoError> {
    val (reason, status) = __remove_file(path)
    if status != 0 {
        return Result::Err(failure(status, "cannot remove {path}: {reason}"))
    }
    Result::Ok(())
}

// The runtime numbers its failures from 1 in `IoErrorKind`'s declaration order. `std::io`
// keeps its own copy: a library module reaches another's exports only, and this is no API.
func failure(status: i32, message: string) -> IoError {
    val kind = match status {
        1 => IoErrorKind::NotFound,
        2 => IoErrorKind::PermissionDenied,
        3 => IoErrorKind::InvalidData,
        _ => IoErrorKind::Other
    }
    IoError { kind: kind, message: message }
}
