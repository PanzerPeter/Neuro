// End-to-end tests for the codepoint iterators: `.chars()` and `.char_indices()`.
//
// The two answer different questions about the same walk. `.chars()` hands out an
// iterator over Unicode scalar values, so it composes with the rest of the protocol —
// `.enumerate()` numbers the code points, an adapter transforms them, and the iterator
// itself is a value that can be stepped by hand. `.char_indices()` is a `for` head that
// binds the *byte* offset of each scalar, which is what a tokenizer feeds back into
// `.slice(range)`.
//
// Every program compiles to a native binary and runs: a decode that mis-read a
// continuation byte, or a cursor that advanced by the wrong width, would produce the
// wrong exit code here rather than merely type-check.

mod common;
use common::CompileTest;

use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn neurc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_neurc"))
}

/// Type-check `source`, reporting whether it was accepted and what it said.
fn check_source(source: &str) -> (bool, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source_path = temp_dir.path().join("test.nr");
    fs::write(&source_path, source).expect("Failed to write source file");

    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&source_path)
        .output()
        .expect("Failed to execute neurc check");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), stderr)
}

fn run_expecting(source: &str, expected: i32) {
    let test = CompileTest::new();
    let code = test
        .compile_and_run("test.nr", source)
        .expect("program should compile and run");
    assert_eq!(code, expected, "unexpected exit code");
}

/// The count that separates a codepoint walk from a byte walk: `.len()` is 9 bytes here
/// and the text is 5 scalars, one from each UTF-8 width.
#[test]
fn chars_counts_code_points_not_bytes() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢🎉z"
    mut scalars = 0
    for c in text.chars() {
        scalars = scalars + 1
    }
    scalars
}
"#;
    run_expecting(source, 5);
}

/// Each width decodes to its own scalar value, so a lead byte read with the wrong mask
/// or a continuation byte folded in the wrong order changes the answer.
#[test]
fn every_utf8_width_decodes_to_its_scalar() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢🎉"
    mut wrong = 0
    mut seen = 0
    for c in text.chars() {
        val code = c as u32
        if seen == 0 {
            if code != 97 { wrong = wrong + 1 }
        }
        if seen == 1 {
            if code != 233 { wrong = wrong + 2 }
        }
        if seen == 2 {
            if code != 28450 { wrong = wrong + 4 }
        }
        if seen == 3 {
            if code != 127881 { wrong = wrong + 8 }
        }
        seen = seen + 1
    }
    wrong
}
"#;
    run_expecting(source, 0);
}

/// An empty string yields nothing at all: the guard runs before the first decode.
#[test]
fn an_empty_string_yields_no_scalars() {
    let source = r#"
func main() -> i32 {
    mut steps = 0
    for c in "".chars() {
        steps = steps + 1
    }
    steps
}
"#;
    run_expecting(source, 0);
}

/// `.chars()` is an iterator like any other, so `.enumerate()` numbers its steps — and
/// those numbers count code points, which is exactly where they differ from the byte
/// offsets `.char_indices()` binds.
#[test]
fn chars_composes_with_enumerate() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢"
    mut last = 0
    for (i, c) in text.chars().enumerate() {
        last = i as i32
    }
    last
}
"#;
    run_expecting(source, 2);
}

/// An adapter chain folds into the same loop it decorates, over scalars like over any
/// other element stream.
#[test]
fn an_adapter_chain_filters_scalars() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢🎉z"
    mut wide = 0
    for code in text.chars().map(|c: char| c as u32).filter(|u: u32| u > 127) {
        wide = wide + 1
    }
    wide
}
"#;
    run_expecting(source, 3);
}

/// The iterator is a value, not only a head: it can be held, stepped by hand, and its
/// `Option::None` end read directly.
#[test]
fn the_iterator_is_a_value_that_steps_by_hand() {
    let source = r#"
func main() -> i32 {
    mut walk = "ab".chars()
    val first = walk.next() ?? 'z'
    val second = walk.next() ?? 'z'
    val past_end = walk.next() ?? 'z'
    mut score = 0
    if first == 'a' { score = score + 1 }
    if second == 'b' { score = score + 2 }
    if past_end == 'z' { score = score + 4 }
    score
}
"#;
    run_expecting(source, 7);
}

/// The receiver is borrowed, not consumed: the text is still usable after the walk, and
/// a `&string` walks exactly as an owned `string` does.
#[test]
fn walking_borrows_the_text_rather_than_consuming_it() {
    let source = r#"
func count_scalars(text: &string) -> i32 {
    mut n = 0
    for c in text.chars() {
        n = n + 1
    }
    n
}

func main() -> i32 {
    val text = "héllo"
    mut total = 0
    for c in text.chars() {
        total = total + 1
    }
    total + count_scalars(text.slice(0..6)) + (text.len() as i32)
}
"#;
    run_expecting(source, 5 + 5 + 6);
}

/// The byte offsets `.char_indices()` binds are exactly the ones `.slice(range)` takes,
/// which is the pairing the method exists for.
#[test]
fn char_indices_binds_byte_offsets_that_slice_accepts() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢🎉z"
    mut found: u64 = 0
    for (off, c) in text.char_indices() {
        if c == 'z' { found = off }
    }
    val tail = text.slice(found..text.len())
    if tail != "z" { return 99 }
    found as i32
}
"#;
    run_expecting(source, 10);
}

/// Each offset names the scalar the step yields, not the one after it: the cursor is
/// sampled before `next` advances it.
#[test]
fn each_offset_names_the_scalar_that_step_yields() {
    let source = r#"
func main() -> i32 {
    val text = "aé漢"
    mut wrong = 0
    mut seen = 0
    for (off, c) in text.char_indices() {
        if seen == 0 {
            if off != 0 { wrong = wrong + 1 }
            if c != 'a' { wrong = wrong + 1 }
        }
        if seen == 1 {
            if off != 1 { wrong = wrong + 2 }
            if c != 'é' { wrong = wrong + 2 }
        }
        if seen == 2 {
            if off != 3 { wrong = wrong + 4 }
            if c != '漢' { wrong = wrong + 4 }
        }
        seen = seen + 1
    }
    wrong
}
"#;
    run_expecting(source, 0);
}

/// A `continue` skips the body, never the sample: the offsets on the far side of one
/// still name their own scalar.
#[test]
fn a_continue_leaves_the_offsets_aligned() {
    let source = r#"
func main() -> i32 {
    val text = "a漢b漢c"
    mut total: u64 = 0
    for (off, c) in text.char_indices() {
        if (c as u32) > 127 { continue }
        total = total + off
    }
    total as i32
}
"#;
    // The ASCII scalars sit at byte 0, 4 and 8; the two 3-byte kanji fill the gaps.
    run_expecting(source, 12);
}

/// A labelled `break` leaves the walk like any other loop.
#[test]
fn a_labelled_break_leaves_the_walk() {
    let source = r#"
func main() -> i32 {
    val text = "abcdef"
    mut steps = 0
    scan: for (off, c) in text.char_indices() {
        if c == 'd' { break scan }
        steps = steps + 1
    }
    steps
}
"#;
    run_expecting(source, 3);
}

/// A pair binds a position and a value; `.chars()` yields only the value, so the head
/// arity is rejected rather than silently binding the scalar twice.
#[test]
fn a_pair_head_over_chars_is_rejected() {
    let (success, stderr) = check_source(
        r#"
func main() -> i32 {
    for (i, c) in "hi".chars() { }
    0
}
"#,
    );
    assert!(
        !success,
        "a pair head needs a position-yielding iterable; got: {stderr}"
    );
}

/// `.char_indices()` yields both halves, so a single binding cannot take them.
#[test]
fn a_single_binding_over_char_indices_is_rejected() {
    let (success, stderr) = check_source(
        r#"
func main() -> i32 {
    for c in "hi".char_indices() { }
    0
}
"#,
    );
    assert!(!success, "char_indices binds a pair; got: {stderr}");
}

/// The head already carries a position, so decorating it would leave two sources for
/// one binding.
#[test]
fn decorating_a_char_indices_head_is_rejected() {
    let (success, stderr) = check_source(
        r#"
func main() -> i32 {
    for (i, c) in "hi".char_indices().enumerate() { }
    0
}
"#,
    );
    assert!(
        !success,
        "char_indices takes no .enumerate(); got: {stderr}"
    );
}

/// The decode step behind the prelude's iterator is not part of the language: a program
/// has no byte-indexed read of a string.
#[test]
fn the_decode_intrinsic_is_not_reachable_from_a_program() {
    let (success, stderr) = check_source(
        r#"
func main() -> i32 {
    val c = "hi".__char_at(0)
    0
}
"#,
    );
    assert!(
        !success,
        "__char_at belongs to the prelude alone; got: {stderr}"
    );
}
