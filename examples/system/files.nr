// Whole files with `std::fs`. Every call names its file with a `&string` and returns a
// `Result` whose error is the prelude's `IoError`: a `kind` to branch on and a `message`
// for a person. The program writes in the working directory and removes what it wrote.

func save_and_load(path: &string) -> Result<string, IoError> {
    std::fs::write(path, "epochs = 3\nrate = 0.5\n")?
    std::fs::read_to_string(path)
}

func main() -> i32 {
    val path: &string = "neuro_example_settings.txt"
    val text = save_and_load(path) ?? panic("cannot save the settings")
    print(text)
    println("{text.len()} bytes of text")

    // `read_bytes` takes any file; `read_to_string` refuses one that is not UTF-8.
    mut raw: Vec<u8> = Vec::new()
    raw.push(0xC3u8)
    raw.push(0x28u8)
    std::fs::write_bytes(path, &raw) ?? panic("cannot write bytes")
    val bytes = std::fs::read_bytes(path) ?? panic("cannot read bytes")
    println("{bytes.len()} raw bytes, first {bytes[0]}")
    match std::fs::read_to_string(path) {
        Ok(_) => println("unexpected text"),
        Err(e) => println("{e.kind:?}"),
    }

    std::fs::remove_file(path) ?? panic("cannot remove the file")

    // A missing file is an `Err`, never a crash: branch on its kind.
    val code = match std::fs::read_to_string(path) {
        Ok(_) => 1,
        Err(e) => match e.kind {
            IoErrorKind::NotFound => 0,
            _ => 2,
        },
    }
    println("removed: {code == 0}")
    code
}
