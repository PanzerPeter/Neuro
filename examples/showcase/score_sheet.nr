// Showcase: a score sheet read from text. Integration example: `.lines()` and `.split(sep)`
// views, `.parse::<T>()` through `?` in a `Result` function, a function returning
// `impl Iterator` over a `.filter` chain, a generic consumer bounded by
// `Iterator<Item = &string>`, a struct with a `&self` method, a `Vec` of records and format
// specs. Nothing is copied out of the text until a record keeps its name.

struct Student {
    name: string,
    total: i32,
    count: i32
}

impl Student {
    func mean(&self) -> f64 {
        (self.total as f64) / (self.count as f64)
    }
}

// The sheet's non-blank lines. A blank line only separates groups of students.
func entries(sheet: &string) -> impl Iterator<Item = &string> {
    sheet.lines().filter(|line: &&string| line.len() > 0)
}

// The sum and count of a run of scores. The first piece that is not a number ends the run
// and becomes the caller's error.
func tally<I: Iterator<Item = &string>>(scores: I) -> Result<(i32, i32), ParseError> {
    mut total = 0
    mut count = 0
    for score in scores {
        total += score.parse::<i32>()?
        count += 1
    }
    Ok((total, count))
}

// One `name;score score ...` line.
func read(line: &string) -> Result<Student, ParseError> {
    mut name: &string = ""
    mut scores: &string = ""
    mut part = 0
    for piece in line.split(";") {
        if part == 0 {
            name = piece
        } else {
            scores = piece
        }
        part += 1
    }
    val (total, count) = tally(scores.split(" "))?
    Ok(Student { name: name.clone(), total: total, count: count })
}

func main() -> i32 {
    val sheet = "ada;9 8 10\r\nbo;7 9\n\ncy;10 10 10 10\ndee;8 x\n"
    mut students: Vec<Student> = Vec::new()
    mut skipped = 0
    for line in entries(&sheet) {
        match read(line) {
            Ok(student) => students.push(student),
            Err(e) => {
                println("skipped '{line}': {e:?}")
                skipped += 1
            }
        }
    }

    mut points = 0
    for s in &students {
        println("{s.name:<4} {s.count} scores, mean {s.mean():.2}")
        points += s.total
    }
    println("points {points}, skipped {skipped}")
    points + skipped * 100                  // 27 + 16 + 40 + 100 = 183
}
