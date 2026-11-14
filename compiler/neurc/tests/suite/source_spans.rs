//! Per-file source spans: a compile-time diagnostic or a run-time panic raised in any
//! module names that module's own file, line and column, and one raised in a prelude body
//! names the prelude rather than a position in the program's file.

use crate::compile_harness::{CompileTest, neurc_path};
use std::path::PathBuf;
use std::process::Command;

/// Lay out a program whose root is `main.nr`, keeping the directory alive with it.
fn written(files: &[(&str, &str)]) -> (CompileTest, PathBuf) {
    let test = CompileTest::new();
    let mut root = None;
    for (path, source) in files {
        let at = test.write_source(path, source);
        if *path == "main.nr" {
            root = Some(at);
        }
    }
    (test, root.expect("main.nr is among the files"))
}

/// Compile and run a program that must panic, and return what it wrote to stderr.
fn panic_of(files: &[(&str, &str)]) -> String {
    let (test, root) = written(files);
    let exe = test
        .compile(&root)
        .unwrap_or_else(|e| panic!("the program should compile: {e}"));
    let output = Command::new(&exe).output().expect("run the program");
    assert!(!output.status.success(), "the program should panic");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `neurc check` a program it must reject or warn about, and return everything it printed.
fn check_of(files: &[(&str, &str)]) -> String {
    let (_test, root) = written(files);
    let output = Command::new(neurc_path())
        .arg("check")
        .arg(&root)
        .output()
        .expect("run neurc");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

const LIB: &str = r#"export struct Box { export v: i32 }

impl Box {
    func check(self) -> i32 {
        if self.v > 1 { panic("method") }
        self.v
    }
}

export func pick<T>(x: T, bad: bool) -> T {
    if bad { panic("generic") }
    x
}

export func add(a: i32, b: i32) -> i32 {
    a + b
}

export func at(i: i32) -> i32 {
    val xs = [1, 2, 3]
    xs[i]
}

export func maker() -> (i32) -> i32 {
    |x| if x > 0 { panic("closure") } else { x }
}
"#;

const SIDE: &str = r#"export func side(n: i32) -> i32 {
    assert(n < 0)
    n
}
"#;

/// One panic location reached seven ways: through a method, a generic, a checked
/// operator, an index, a returned closure, a nested module's path, and the root itself.
/// Every expected position was counted by hand from the sources above.
#[test]
fn a_panic_names_the_file_line_and_column_of_the_module_it_is_in() {
    let cases = [
        ("Box { v: 5 }.check()", "panic: method at ", "lib.nr:5:25"),
        ("pick(1, true)", "panic: generic at ", "lib.nr:11:14"),
        (
            "add(2147483647, 1)",
            "panic: integer overflow at ",
            "lib.nr:16:5",
        ),
        (
            "at(7)",
            "panic: array index out of bounds at ",
            "lib.nr:21:5",
        ),
        ("maker()(1)", "panic: closure at ", "lib.nr:25:20"),
        (
            "shapes::sq::side(2)",
            "assertion failed at ",
            "shapes/sq.nr:2:5",
        ),
        (
            "if at(0) == 1 { panic(\"root\") } else { 0 }",
            "panic: root at ",
            "main.nr:4:21",
        ),
    ];
    for (call, message, location) in cases {
        let main = format!(
            "import lib::{{Box, pick, add, at, maker}}\n\nfunc main() -> i32 {{\n    {call}\n}}\n"
        );
        let stderr = panic_of(&[
            ("main.nr", &main),
            ("lib.nr", LIB),
            ("shapes/mod.nr", ""),
            ("shapes/sq.nr", SIDE),
        ]);
        assert!(stderr.contains(message), "{call}: {stderr}");
        assert!(
            stderr.trim_end().ends_with(location),
            "{call} should panic at {location}: {stderr}"
        );
    }
}

/// BUG-155: a prelude body's spans were read against the program's file, so a panic in
/// one pointed at a line past the end of a five-line program.
#[test]
fn a_panic_in_a_prelude_body_names_the_prelude() {
    let nan =
        "func main() -> i32 {\n    val z = 0.0\n    val o = OrderedF64::new(z / z)\n    0\n}\n";
    let stride = "func main() -> i32 {\n    val r = 0..4\n    mut t = 0\n    for i in r.step(t) { t += i }\n    t\n}\n";
    for (source, message) in [
        (nan, "panic: OrderedF64 cannot hold NaN at <prelude>:"),
        (stride, "panic: range step must be positive at <prelude>:"),
    ] {
        let stderr = panic_of(&[("main.nr", source)]);
        assert!(stderr.contains(message), "{stderr}");
        assert!(!stderr.contains("main.nr"), "{stderr}");
    }
}

/// A diagnostic from every stage that reports one by span: the lexer, the parser, argument
/// binding and the type checker. Each used to print its message with no location once the
/// program had a second module.
#[test]
fn a_compile_error_in_a_module_names_that_modules_file() {
    let main = "import geo::f\n\nfunc main() -> i32 {\n    f()\n}\n";
    let cases = [
        (
            "export func f() -> i32 {\n    val s: i32 = \"no\"\n    s\n}\n",
            "type mismatch",
            "geo.nr:2:5",
            "2 |     val s: i32 = \"no\"",
        ),
        (
            "export func f() -> i32 {\n    1 $ 2\n}\n",
            "unexpected character '$'",
            "geo.nr:2:7",
            "2 |     1 $ 2",
        ),
        (
            "export func f() -> i32 {\n    val x = (1\n    x\n}\n",
            "error: ",
            "geo.nr:3:5",
            "3 |     x",
        ),
        (
            "func g(x: i32) -> i32 { x }\n\nexport func f() -> i32 {\n    g(y: 1)\n}\n",
            "error: ",
            "geo.nr:4:10",
            "4 |     g(y: 1)",
        ),
    ];
    for (geo, message, location, line) in cases {
        let printed = check_of(&[("main.nr", main), ("geo.nr", geo)]);
        assert!(printed.contains(message), "{geo}: {printed}");
        assert!(printed.contains(location), "{geo}: {printed}");
        assert!(printed.contains(line), "{geo}: {printed}");
    }
}

/// A warning is rendered like an error, at its own module's line and column, not as byte
/// offsets that, in one offset space across files, index no file the reader has open.
#[test]
fn a_warning_in_a_module_is_located_in_that_module() {
    let printed = check_of(&[
        (
            "main.nr",
            "import spin::go\n\nfunc main() -> i32 {\n    go()\n}\n",
        ),
        (
            "spin.nr",
            "export func go() -> i32 {\n    while true {\n        break\n    }\n    0\n}\n",
        ),
    ]);
    assert!(
        printed.contains("warning[prefer_loop_over_while_true]: "),
        "{printed}"
    );
    assert!(printed.contains("spin.nr:2:11"), "{printed}");
    assert!(printed.contains("Type checking passed"), "{printed}");
}
