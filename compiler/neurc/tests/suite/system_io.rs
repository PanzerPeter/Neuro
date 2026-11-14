// `std::fs`, `std::env`, `std::io::read_line` and `std::time`: the standard library's reach
// into the operating system, through the runtime the backend links into a program that uses it.
//
// Every program here runs in its own temporary directory, so the files it writes land there.

use crate::compile_harness::{CompileTest, assert_check_error, neurc_path};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What a run gets beyond its source: arguments, environment and standard input.
#[derive(Default)]
struct Run<'a> {
    args: &'a [&'a str],
    env: &'a [(&'a str, &'a str)],
    stdin: &'a [u8],
}

fn build(source: &str) -> (CompileTest, PathBuf) {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source("main.nr", source))
        .unwrap_or_else(|e| panic!("should compile: {e}"));
    (test, exe)
}

/// Run `exe` in its own directory, returning its exit code and standard output.
fn run_in_place(exe: &Path, run: &Run) -> (i32, String) {
    let mut command = Command::new(exe);
    command
        .args(run.args)
        .envs(run.env.iter().copied())
        .current_dir(exe.parent().expect("the binary has a directory"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = command.spawn().expect("run the program");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin.write_all(run.stdin).expect("feed standard input");
    }
    let output = child.wait_with_output().expect("wait for the program");
    let stdout = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    (output.status.code().unwrap_or(-1), stdout)
}

fn stdout_with(source: &str, run: &Run) -> String {
    let (_test, exe) = build(source);
    let (code, stdout) = run_in_place(&exe, run);
    assert_eq!(code, 0, "the program failed; it printed:\n{stdout}");
    stdout
}

#[test]
fn files_round_trip_and_report_each_kind_of_failure() {
    let out = stdout_with(
        r#"
func round_trip() -> Result<string, IoError> {
    std::fs::write("notes.txt", "alpha\nbeta\n")?
    val text = std::fs::read_to_string("notes.txt")?
    std::fs::remove_file("notes.txt")?
    Result::Ok(text)
}

func main() -> i32 {
    print(round_trip() ?? "round trip failed\n")
    mut bytes: Vec<u8> = Vec::new()
    bytes.push(0xffu8)
    bytes.push(0u8)
    bytes.push(66u8)
    std::fs::write_bytes("raw.bin", &bytes) ?? panic("write_bytes")
    val back = std::fs::read_bytes("raw.bin") ?? panic("read_bytes")
    println("{back.len()} {back[0]} {back[1]} {back[2]}")
    match std::fs::read_to_string("raw.bin") {
        Ok(_) => println("read raw bytes as text"),
        Err(e) => println("{e.kind:?}: {e.message}"),
    }
    std::fs::remove_file("raw.bin") ?? panic("remove")
    match std::fs::remove_file("raw.bin") {
        Ok(_) => println("removed twice"),
        Err(e) => println("{e.kind:?}"),
    }
    match std::fs::read_bytes("absent.bin") {
        Ok(_) => println("read a missing file"),
        Err(e) => println("{e.kind:?} {e.message.len() > 0}"),
    }
    match std::fs::write("no_such_dir/out.txt", "x") {
        Ok(_) => println("wrote into a missing directory"),
        Err(e) => println("{e.kind:?}"),
    }
    val nul = "bad\0name"
    match std::fs::read_to_string(&nul) {
        Ok(_) => println("opened a NUL path"),
        Err(e) => println("{e.kind:?} {e.message.len() > 0}"),
    }
    std::fs::write("empty.txt", "") ?? panic("write empty")
    val empty = std::fs::read_to_string("empty.txt") ?? panic("read empty")
    println("[{empty}]")
    std::fs::remove_file("empty.txt") ?? panic("remove empty")
    0
}
"#,
        &Run::default(),
    );
    assert_eq!(
        out,
        "alpha\nbeta\n\
         3 255 0 66\n\
         InvalidData: cannot read raw.bin: the file is not valid UTF-8\n\
         NotFound\n\
         NotFound true\n\
         NotFound\n\
         Other true\n\
         []\n"
    );
}

#[test]
fn a_failure_names_the_file_and_propagates_through_question_mark() {
    let out = stdout_with(
        r#"
func load(path: &string) -> Result<u64, IoError> {
    val text = std::fs::read_to_string(path)?
    Result::Ok(text.len())
}

func main() -> i32 {
    match load("missing.txt") {
        Ok(n) => println("{n}"),
        Err(e) => println("{e.message.slice(0..20)}"),
    }
    val Ok(n) = load("missing.txt") else |e| {
        println("else {e.kind:?}")
        return 0
    }
    println("{n}")
    0
}
"#,
        &Run::default(),
    );
    assert_eq!(out, "cannot read missing.\nelse NotFound\n");
}

#[test]
fn args_list_the_command_line_and_var_reads_the_environment() {
    let source = r#"
func main() -> i32 {
    val args = std::env::args()
    println("{args.len()}")
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            println("[{arg}]")
        }
    }
    println(std::env::var("NEURO_SET") ?? "unset")
    println(std::env::var("NEURO_UNSET") ?? "unset")
    println(std::env::var("NEURO_EMPTY") ?? "unset")
    println(std::env::var("") ?? "no name")
    println(std::env::var("A=B") ?? "no name")
    0
}
"#;
    let out = stdout_with(
        source,
        &Run {
            args: &["one", "two words", ""],
            env: &[("NEURO_SET", "héllo"), ("NEURO_EMPTY", "")],
            ..Run::default()
        },
    );
    assert_eq!(
        out,
        "4\n[one]\n[two words]\n[]\nhéllo\nunset\n\nno name\nno name\n"
    );
}

/// A command-line byte that begins no valid UTF-8 reads as U+FFFD.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_utf8_is_read_lossily() {
    use std::os::unix::ffi::OsStrExt;
    let (_test, exe) = build(
        r#"
func main() -> i32 {
    val args = std::env::args()
    println(args.get(1) ?? "missing")
    0
}
"#,
    );
    let output = Command::new(&exe)
        .arg(std::ffi::OsStr::from_bytes(b"a\xffb\xe2\x82"))
        .output()
        .expect("run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a\u{FFFD}b\u{FFFD}\u{FFFD}\n"
    );
}

#[test]
fn a_test_build_reads_its_command_line_too() {
    let test = CompileTest::new();
    let path = test.write_source(
        "args_test.nr",
        r#"
@test
func sees_itself() {
    assert(std::env::args().len() >= 1)
}
"#,
    );
    let output = Command::new(neurc_path())
        .arg("test")
        .arg(&path)
        .output()
        .expect("run neurc test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn read_line_yields_each_line_then_none() {
    let source = r#"
func main() -> i32 {
    print("first? ")
    loop {
        match std::io::read_line() {
            Ok(Some(line)) => println("[{line}]"),
            Ok(None) => {
                println("end")
                break
            }
            Err(e) => {
                println("{e.kind:?}")
                break
            }
        }
    }
    0
}
"#;
    let (_test, exe) = build(source);
    let (_, lines) = run_in_place(
        &exe,
        &Run {
            stdin: b"one\r\ntwo\n\nlast",
            ..Run::default()
        },
    );
    assert_eq!(lines, "first? [one]\n[two]\n[]\n[last]\nend\n");
    let (_, empty) = run_in_place(&exe, &Run::default());
    assert_eq!(empty, "first? end\n");
    let (_, invalid) = run_in_place(
        &exe,
        &Run {
            stdin: b"ok\n\xff\n",
            ..Run::default()
        },
    );
    assert_eq!(invalid, "first? [ok]\nInvalidData\n");
    let long = "x".repeat(200_000) + "\n";
    let (_, long_line) = run_in_place(
        &exe,
        &Run {
            stdin: long.as_bytes(),
            ..Run::default()
        },
    );
    assert_eq!(
        long_line.len(),
        "first? [".len() + 200_000 + "]\nend\n".len()
    );
}

#[test]
fn instants_measure_forward_and_durations_convert() {
    let out = stdout_with(
        r#"
func main() -> i32 {
    val start = std::time::Instant::now()
    mut spin: u64 = 0
    while spin < 2000000 {
        spin = spin + 1
    }
    val later = std::time::Instant::now()
    val forward = later.duration_since(&start)
    val backward = start.duration_since(&later)
    val elapsed = start.elapsed()
    println("{forward.as_nanos() > 0} {backward.as_nanos()} {elapsed.as_nanos() >= forward.as_nanos()}")
    println("{forward.as_millis() == forward.as_nanos() / 1000000}")
    println("{forward.as_secs_f64() * 1000000000.0 >= forward.as_nanos() as f64 - 1.0}")
    val copy = forward
    println("{copy.as_nanos() == forward.as_nanos()} {spin}")
    0
}
"#,
        &Run::default(),
    );
    assert_eq!(out, "true 0 true\ntrue\ntrue\ntrue 2000000\n");
}

#[test]
fn a_program_cannot_call_the_runtime_intrinsics() {
    assert_check_error(
        "intrinsic.nr",
        r#"
func main() -> i32 {
    val (text, status) = __read_text("x")
    status
}
"#,
        "undefined function '__read_text'",
    );
    assert_check_error(
        "clock.nr",
        "func main() -> i32 {\n    __monotonic_nanos() as i32\n}\n",
        "undefined function '__monotonic_nanos'",
    );
}

#[test]
fn io_error_kind_variants_are_written_with_the_enum_name() {
    assert_check_error(
        "bare.nr",
        "func main() -> i32 {\n    val k = NotFound\n    0\n}\n",
        "NotFound",
    );
}

/// The library's bodies are all compiled, so a program that does no I/O must lose the ones
/// it does not call, keep its own `main`, and link no runtime.
#[test]
fn a_program_without_io_links_no_runtime_and_keeps_its_main() {
    let ir = |source: &str| {
        let test = CompileTest::new();
        let path = test.write_source("ir.nr", source);
        let out = path.with_extension("ll");
        let status = Command::new(neurc_path())
            .args(["compile", "--emit", "llvm-ir", "-o"])
            .arg(&out)
            .arg(&path)
            .status()
            .expect("run neurc");
        assert!(status.success());
        std::fs::read_to_string(out).expect("read the IR")
    };
    let quiet = ir("func main() -> i32 {\n    println(\"hi\")\n    0\n}\n");
    assert!(quiet.contains("define i32 @main()"), "{quiet}");
    for absent in [
        "__neuro_read_text",
        "__neuro_arg",
        "__neuro_errno_location",
        "@\"std::fs::read_to_string\"(",
        "@\"std::env::args\"(",
        "neuro.stdin.sync",
    ] {
        assert!(
            !quiet.contains(absent),
            "`{absent}` in a program without I/O"
        );
    }
    let reader = ir("func main() -> i32 {\n    std::env::args().len() as i32\n}\n");
    assert!(reader.contains("define i32 @main(i32"), "{reader}");
    assert!(reader.contains("neuro.program_main"), "{reader}");
    assert!(
        !reader.contains("@\"std::fs::read_to_string\"("),
        "unused library functions survived"
    );
}

/// One file's byte count, reached five ways. The file holds "abc\n" (4 bytes) and a missing
/// file counts as 100, so each form answers 104.
#[test]
fn one_read_agrees_across_equivalent_forms() {
    let out = stdout_with(
        r#"
func tail(path: &string) -> Result<u64, IoError> {
    Result::Ok(std::fs::read_to_string(path)?.len())
}

func bound(path: &string) -> u64 {
    val text = std::fs::read_to_string(path)
    val n = match text {
        Ok(t) => t.len(),
        Err(_) => 100,
    }
    n
}

func nested(path: &string) -> u64 {
    if path.len() > 0 {
        match std::fs::read_bytes(path) {
            Ok(b) => if b.len() > 0 { b.len() } else { 0 },
            Err(e) => match e.kind {
                IoErrorKind::NotFound => 100,
                _ => 0,
            },
        }
    } else {
        0
    }
}

func through<T>(value: T, f: (T) -> u64) -> u64 {
    f(value)
}

func main() -> i32 {
    std::fs::write("data.txt", "abc\n") ?? panic("write")
    val a = (tail("data.txt") ?? 0) + (tail("gone.txt") ?? 100)
    val b = bound("data.txt") + bound("gone.txt")
    val c = nested("data.txt") + nested("gone.txt")
    val count = |p: &string| -> u64 { std::fs::read_to_string(p).map(|t| t.len()) ?? 100 }
    val d = count("data.txt") + count("gone.txt")
    val data: &string = "data.txt"
    val gone: &string = "gone.txt"
    val e = through(data, |p| bound(p)) + through(gone, |p| nested(p))
    std::fs::remove_file("data.txt") ?? panic("remove")
    println("{a} {b} {c} {d} {e}")
    0
}
"#,
        &Run::default(),
    );
    assert_eq!(out, "104 104 104 104 104\n");
}

/// Every buffer the runtime hands back is the program's: read, failed, written and listed,
/// in a loop, under the leak check.
#[test]
fn runtime_buffers_are_released() {
    let test = CompileTest::new();
    let dir = test.write_source("probe", "").with_file_name("");
    let dir = dir.to_string_lossy().replace('\\', "/");
    let source = format!(
        r#"
func cycle(round: u64) -> u64 {{
    val path = "{dir}leak_{{round}}.txt"
    val body = "line {{round}}"
    std::fs::write(&path, &body) ?? panic("write")
    val text = std::fs::read_to_string(&path) ?? panic("read")
    val bytes = std::fs::read_bytes(&path) ?? panic("bytes")
    std::fs::write_bytes(&path, &bytes) ?? panic("write bytes")
    std::fs::remove_file(&path) ?? panic("remove")
    val gone = match std::fs::read_bytes(&path) {{
        Ok(b) => b.len(),
        Err(e) => e.message.len(),
    }}
    val missing = match std::fs::read_to_string(&path) {{
        Ok(t) => t.len(),
        Err(e) => e.message.len(),
    }}
    val unset = std::env::var("NEURO_NEVER_SET").is_none()
    text.len() + bytes.len() + gone + missing + std::env::args().len() + if unset {{ 1 }} else {{ 0 }}
}}

func main() -> i32 {{
    mut round: u64 = 0
    mut total: u64 = 0
    while round < 50 {{
        total = total + cycle(round)
        round = round + 1
    }}
    val line = std::io::read_line()
    if total > 0 && line.is_ok() {{ 0 }} else {{ 1 }}
}}
"#
    );
    assert_eq!(test.run_leak_checked("leaks.nr", &source), 0);
}
