// Showcase: a training run that keeps its journal on disk, stops, and resumes from what the
// file says. Integration example: `std::fs` writes and reads the journal, `std::env::args()`
// takes the epoch count the way a training entry point does, `std::time::Instant` times the
// run, and the journal is built with `StringBuilder`, read back with `.chars()` and
// `.parse::<T>()`, and its failures lifted into one error enum with `.map_err` and `?`. The
// exit code is the number of epochs the journal holds at the end.

enum RunError {
    Io(IoError),
    Parse(ParseError),
    Empty
}

func explain(e: RunError) -> string {
    match e {
        RunError::Io(io) => "journal: {io.message}",
        RunError::Parse(why) => "journal entry: {why:?}",
        RunError::Empty => "journal is empty"
    }
}

// A loss that falls as training goes on.
func loss_at(epoch: i32) -> f64 {
    1.0 / (epoch as f64 + 1.0)
}

// Run `count` epochs starting at `first`, appending one `epoch loss` line each.
func train(journal: &mut StringBuilder, first: i32, count: i32) {
    for epoch in first..first + count {
        journal.push_str("{epoch} {loss_at(epoch):.4}\n")
    }
}

// The last line's epoch and every loss, read back from the text.
func read_journal(text: &string) -> Result<(i32, Vec<f64>), RunError> {
    mut losses: Vec<f64> = Vec::new()
    mut last_epoch = -1
    mut field = StringBuilder::new()
    mut epoch_text = ""
    for c in text.chars() {
        if c == ' ' {
            epoch_text = field.to_string()
            field.clear()
        } else if c == '\n' {
            last_epoch = epoch_text.parse::<i32>().map_err(RunError::Parse)?
            losses.push(field.to_string().parse::<f64>().map_err(RunError::Parse)?)
            field.clear()
        } else {
            field.push_str("{c}")
        }
    }
    if losses.len() == 0 {
        return Err(RunError::Empty)
    }
    Ok((last_epoch, losses))
}

func load(path: &string) -> Result<(i32, Vec<f64>), RunError> {
    val text = std::fs::read_to_string(path).map_err(RunError::Io)?
    read_journal(&text)
}

func mean(values: &Vec<f64>) -> f64 {
    mut total = 0.0
    for v in values.iter() {
        total += *v
    }
    total / values.len() as f64
}

func main() -> i32 {
    val path: &string = "neuro_showcase_journal.txt"
    val args = std::env::args()
    val epochs = args.get(1).map(|a| a.parse::<i32>() ?? 3) ?? 3
    val start = std::time::Instant::now()

    // A run with no journal yet starts from nothing.
    match load(path) {
        Ok(_) => println("a journal was left over"),
        Err(e) => println("fresh start ({explain(e).slice(0..7)})")
    }

    mut journal = StringBuilder::new()
    train(&mut journal, 0, epochs)
    std::fs::write(path, &journal.to_string()) ?? panic("cannot save the journal")

    // Resume: the file, not the program's memory, says where training stopped.
    val (stopped, first_losses) = load(path) ?? panic("cannot reload the journal")
    println("stopped after epoch {stopped}, mean loss {mean(&first_losses):.4}")
    train(&mut journal, stopped + 1, 2)
    std::fs::write(path, &journal.to_string()) ?? panic("cannot save the journal")

    val (last, losses) = load(path) ?? panic("cannot reload the journal")
    println("resumed to epoch {last}, mean loss {mean(&losses):.4}")

    // A damaged journal is a value to report, not a crash.
    std::fs::write(path, "0 0.5\n1 lots\n") ?? panic("cannot damage the journal")
    match load(path) {
        Ok(_) => println("read a damaged journal"),
        Err(e) => println(explain(e))
    }
    std::fs::remove_file(path) ?? panic("cannot remove the journal")
    println("timed: {start.elapsed().as_nanos() > 0}")
    losses.len() as i32
}
