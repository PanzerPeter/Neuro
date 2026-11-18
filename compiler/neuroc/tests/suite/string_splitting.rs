// End-to-end tests for `text.lines()` and `text.split(sep)`: the prelude's `Lines` and
// `Split`, each yielding `&string` views into the text. The piece rules, the values binding,
// passing, returning and taking the adapters, and the borrows of the text and the separator
// that keep either from dying or moving under them.
use std::process::Command;

use crate::compile_harness::{CompileTest, assert_check_error, assert_exit, stdout_of};

/// Print each piece `pieces` yields for `text`, bracketed, one per line.
fn pieces_of(name: &str, calls: &[&str]) -> String {
    let mut body = String::new();
    for call in calls {
        body.push_str(&format!(
            "    for p in {call} {{\n        print(\"[{{p}}]\")\n    }}\n    println(\"\")\n"
        ));
    }
    stdout_of(name, &format!("func main() -> i32 {{\n{body}    0\n}}\n"))
}

/// A line ends at `\n` and takes a `\r` before it along; a final newline opens no empty
/// line, and a `\r` not followed by `\n` stays in the text.
#[test]
fn lines_end_at_each_newline() {
    let out = pieces_of(
        "lines_rules.nr",
        &[
            r#""one\r\ntwo\n\nthree\n".lines()"#,
            r#""no newline".lines()"#,
            r#""".lines()"#,
            r#""\n".lines()"#,
            r#""\n\n".lines()"#,
            r#""a\rb\r".lines()"#,
            r#""é\r\n→\n".lines()"#,
        ],
    );
    assert_eq!(
        out,
        "[one][two][][three]\n[no newline]\n\n[]\n[][]\n[a\rb\r]\n[é][→]\n"
    );
}

/// Every piece between two separators is kept, empty ones included; text with no
/// separator is one piece, and empty text one empty piece.
#[test]
fn split_keeps_every_piece() {
    let out = pieces_of(
        "split_rules.nr",
        &[
            r#""a,,b".split(",")"#,
            r#"",a,".split(",")"#,
            r#""".split(",")"#,
            r#""none".split(",")"#,
            r#""x::y:z::".split("::")"#,
            r#""aaa".split("aa")"#,
            r#""héllo→wörld→ü".split("→")"#,
            r#""1 2".split(" ")"#,
        ],
    );
    assert_eq!(
        out,
        "[a][][b]\n[][a][]\n[]\n[none]\n[x][y:z][]\n[][a]\n[héllo][wörld][ü]\n[1][2]\n"
    );
}

/// The pieces are views: their lengths are the bytes between separators, and the text
/// they point into is unchanged and still owned after the walk.
#[test]
fn pieces_are_views_of_the_text() {
    let source = r#"
func main() -> i32 {
    val text = "ab,cde,f".clone()
    mut lengths: u64 = 0
    for p in text.split(",") {
        lengths = lengths * 10 + p.len()
    }
    val again = text.clone()
    if lengths == 231 && again.len() == 8 { 1 } else { 0 }
}
"#;
    assert_exit("split_views.nr", source, 1);
}

const TOTAL: &str = r#"
func total<I: Iterator<Item = &string>>(pieces: I) -> i32 {
    mut sum = 0
    for p in pieces {
        sum += p.parse::<i32>() ?? 0
    }
    sum
}
"#;

/// One sum over the numbers of a two-line table, written five ways: a tail call on the
/// iterators, a bound iterator returned, a nested `if` / `match` arm, through a generic
/// consumer through a closure adapter, and as a call argument. Each is
/// 1 + 2 + 3 + 40 + 50 = 96.
#[test]
fn equivalent_forms_agree() {
    let source = format!(
        r#"{TOTAL}
func tail(text: &string) -> i32 {{
    mut s = 0
    for line in text.lines() {{
        s += total(line.split(","))
    }}
    s
}}

func bound(text: &string) -> i32 {{
    val rows = text.lines()
    mut s = 0
    for line in rows {{
        val cells = line.split(",")
        s += total(cells)
    }}
    s
}}

func nested(text: &string, flag: bool) -> i32 {{
    mut s = 0
    for line in text.lines() {{
        s += if flag {{
            match line.len() {{
                0 => 0,
                _ => total(line.split(",")),
            }}
        }} else {{
            0
        }}
    }}
    s
}}

func adapted(text: &string) -> i32 {{
    mut s = 0
    for line in text.lines() {{
        s += total(line.split(",").filter(|p: &&string| p.len() > 0))
    }}
    s
}}

func argument(text: &string) -> i32 {{
    mut s = 0
    for line in text.lines() {{
        s = s + total(line.split(","))
    }}
    s
}}

func main() -> i32 {{
    val text = "1,2,3\r\n40,50\n".clone()
    val a = tail(&text)
    val b = bound(&text)
    val c = nested(&text, true)
    val d = adapted(&text)
    val e = argument(&text)
    if a == b && b == c && c == d && d == e {{ a }} else {{ 0 }}
}}
"#
    );
    assert_exit("split_forms.nr", &source, 96);
}

/// The iterators bind, pass, return over a parameter, sit in a struct field, take the
/// value adapters, and are driven by hand with `next`.
#[test]
fn iterators_are_values() {
    let source = format!(
        r#"{TOTAL}
struct Table {{
    rows: Lines
}}

func cells(row: &string) -> Split {{
    row.split(";")
}}

func widths(text: &string) -> Map<Lines, &string, u64> {{
    text.lines().map(|l: &string| l.len())
}}

func main() -> i32 {{
    val text = "7;8\n9".clone()
    mut table = Table {{ rows: text.lines() }}
    val first = table.rows.next() ?? ""
    val second = table.rows.next() ?? ""
    val rest = table.rows.next() ?? "none"
    mut widest: u64 = 0
    for w in widths(&text) {{
        if w > widest {{
            widest = w
        }}
    }}
    total(cells(first)) + total(cells(second)) * 10 + (rest.len() as i32) + (widest as i32) * 10
}}
"#
    );
    assert_exit("split_values.nr", &source, 15 + 90 + 4 + 30);
}

/// An owned text, an owned separator and a temporary `for` head, looped so the leak
/// check sees anything the pieces or the iterators lose.
#[test]
fn owned_text_and_separator_release_cleanly() {
    let source = r#"
func row(i: i32) -> string {
    "a{i},b{i}\r\nc,{i}\n"
}

func main() -> i32 {
    mut total: u64 = 0
    mut i = 0
    while i < 50 {
        val text = row(i)
        val sep = ",".clone()
        for line in text.lines() {
            for cell in line.split(&sep) {
                total += cell.len()
            }
        }
        for p in row(i).split(",") {
            total += p.len()
        }
        mut it = text.split(&sep)
        total += (it.next() ?? "").len()
        i += 1
    }
    (total % 256) as i32
}
"#;
    let exit = CompileTest::new().run_leak_checked("split_owned.nr", source);
    let expected: usize = (0..50)
        .map(|i| {
            let row = format!("a{i},b{i}\r\nc,{i}\n");
            let lines: usize = row.lines().flat_map(|l| l.split(',')).map(str::len).sum();
            let pieces: usize = row.split(',').map(str::len).sum();
            lines + pieces + row.split(',').next().map_or(0, str::len)
        })
        .sum();
    assert_eq!(exit as usize, expected % 256);
}

/// A program's own `Split` is a different type: the method still hands out the library's.
#[test]
fn a_program_split_struct_coexists() {
    let source = r#"
struct Split {
    x: i32
}

func main() -> i32 {
    val mine = Split { x: 5 }
    mut n = 0
    for p in "a,b,c".split(",") {
        n += 1
    }
    n + mine.x
}
"#;
    assert_exit("split_own_struct.nr", source, 8);
}

#[test]
fn an_empty_separator_panics() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val empty = "".clone()
    for p in "a,b".split(&empty) {
        println("{p}")
    }
    0
}
"#;
    let path = test.write_source("split_empty_sep.nr", source);
    let exe = test.compile(&path).expect("compile failed");
    let output = Command::new(&exe).output().expect("run binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "an empty separator should abort");
    assert!(
        stderr.contains("panic: split separator is empty"),
        "unexpected panic: {stderr}"
    );
}

/// An iterator over a local, or holding a borrow of a local separator, cannot leave the
/// function or the block that declares it.
#[test]
fn an_iterator_over_a_dying_place_is_refused() {
    assert_check_error(
        "split_returns_local.nr",
        r#"
func f() -> Split {
    val s = "a,b".clone()
    s.split(",")
}
func main() -> i32 { 0 }
"#,
        "cannot return a reference to 's'",
    );
    assert_check_error(
        "lines_returns_local.nr",
        r#"
func f() -> Lines {
    val s = "a\nb".clone()
    s.lines()
}
func main() -> i32 { 0 }
"#,
        "cannot return a reference to 's'",
    );
    assert_check_error(
        "split_returns_local_sep.nr",
        r#"
func f(text: &string) -> Split {
    val sep = ",".clone()
    text.split(&sep)
}
func main() -> i32 { 0 }
"#,
        "cannot return a reference to 'sep'",
    );
    assert_check_error(
        "split_block_sep.nr",
        r#"
func main() -> i32 {
    val text = "a,b"
    mut it = {
        val sep = ",".clone()
        text.split(&sep)
    }
    val first = it.next() ?? ""
    0
}
"#,
        "'sep' does not live long enough",
    );
}

/// The text and the separator stay borrowed while the iterator lives.
#[test]
fn the_text_and_separator_cannot_move_under_an_iterator() {
    for (moved, setup) in [("text", "val m = text"), ("sep", "val m = sep")] {
        let source = format!(
            r#"
func main() -> i32 {{
    val text = "a,b".clone()
    val sep = ",".clone()
    mut it = text.split(&sep)
    {setup}
    val first = it.next() ?? ""
    0
}}
"#
        );
        assert_check_error(
            "split_move.nr",
            &source,
            &format!("cannot move out of '{moved}' while it is borrowed"),
        );
    }
}

#[test]
fn a_bound_iterator_over_a_temporary_is_refused() {
    assert_check_error(
        "split_temporary.nr",
        r#"
func make() -> string {
    "a,b".clone()
}
func main() -> i32 {
    mut it = make().split(",")
    val first = it.next() ?? ""
    0
}
"#,
        "this borrow of a temporary may outlive its statement",
    );
}

#[test]
fn wrong_arguments_are_refused() {
    let cases = [
        (
            "\"x\".split()",
            "incorrect number of arguments: expected 1, found 0",
        ),
        (
            "\"x\".lines(1)",
            "incorrect number of arguments: expected 0, found 1",
        ),
        (
            "\"x\".split(5)",
            "type mismatch: expected &string, found i32",
        ),
        (
            "\"x\".split(\",\".clone())",
            "type mismatch: expected &string, found string",
        ),
    ];
    for (call, expected) in cases {
        assert_check_error(
            "split_arguments.nr",
            &format!("func main() -> i32 {{\n    val it = {call}\n    0\n}}\n"),
            expected,
        );
    }
}
