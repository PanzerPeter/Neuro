// End-to-end tests for multi-file compilation: a program spread across sibling files and
// directory modules, reached through qualified paths, plus the diagnostics for a path that
// names no module, an item a module does not declare, and a name two modules both declare.
//
// The unit tests in the `module-resolution` slice cover discovery against a stub parser;
// these compile and run the real thing.

mod common;
use common::CompileTest;

/// Compile `root` after writing every `(path, source)` pair, and return the exit code.
fn run_program(files: &[(&str, &str)], root: &str) -> Result<i32, String> {
    let test = CompileTest::new();
    let mut root_path = None;
    for (path, source) in files {
        let written = test.write_source(path, source);
        if *path == root {
            root_path = Some(written);
        }
    }
    let root_path = root_path.ok_or_else(|| format!("{} is not among the written files", root))?;
    let exe = test.compile(&root_path)?;
    test.run_executable(&exe)
}

#[test]
fn a_sibling_module_supplies_functions_structs_and_constants() {
    let files = [
        (
            "math.nr",
            r#"
export struct Vec2 {
    export x: i32,
    export y: i32
}

impl Vec2 {
    func new(x: i32, y: i32) -> Vec2 {
        Vec2 { x: x, y: y }
    }

    func sum(&self) -> i32 {
        self.x + self.y
    }
}

export const SCALE: i32 = 3

export func double(n: i32) -> i32 {
    n * 2
}
"#,
        ),
        (
            "main.nr",
            r#"
func main() -> i32 {
    val built = math::Vec2 { x: 2, y: 3 }
    val made: math::Vec2 = math::Vec2::new(1, 1)
    math::double(built.sum()) + made.sum() * math::SCALE
}
"#,
        ),
    ];

    // (2 + 3) * 2 + (1 + 1) * 3 = 16
    assert_eq!(run_program(&files, "main.nr"), Ok(16));
}

#[test]
fn a_directory_module_and_its_child_are_both_reachable() {
    let files = [
        (
            "utils/mod.nr",
            r#"
export func triple(n: i32) -> i32 {
    n * 3
}
"#,
        ),
        (
            "utils/io.nr",
            r#"
export func width(text: &string) -> i32 {
    text.len() as i32
}
"#,
        ),
        (
            "main.nr",
            r#"
func main() -> i32 {
    val label = "neuro"
    utils::triple(4) + utils::io::width(&label)
}
"#,
        ),
    ];

    // 12 + 5 = 17
    assert_eq!(run_program(&files, "main.nr"), Ok(17));
}

#[test]
fn modules_may_reference_each_other() {
    let files = [
        (
            "left.nr",
            r#"
export func base() -> i32 {
    4
}

export func combined() -> i32 {
    right::twice(base())
}
"#,
        ),
        (
            "right.nr",
            r#"
export func twice(n: i32) -> i32 {
    n * 2
}

export func offset() -> i32 {
    left::base() + 1
}
"#,
        ),
        (
            "main.nr",
            r#"
func main() -> i32 {
    left::combined() + right::offset()
}
"#,
        ),
    ];

    // (4 * 2) + (4 + 1) = 13
    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn a_module_enum_matches_from_the_root() {
    let files = [
        (
            "signal.nr",
            r#"
export enum Level {
    Low,
    High
}

export func classify(n: i32) -> Level {
    if n > 10 { Level::High } else { Level::Low }
}
"#,
        ),
        (
            "main.nr",
            r#"
func main() -> i32 {
    match signal::classify(42) {
        Level::High => 7,
        Level::Low => 1
    }
}
"#,
        ),
    ];

    assert_eq!(run_program(&files, "main.nr"), Ok(7));
}

#[test]
fn a_single_file_program_loads_exactly_one_module() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val p = Pair::new(2, 5)
    p.total()
}

struct Pair {
    a: i32,
    b: i32
}

impl Pair {
    func new(a: i32, b: i32) -> Pair {
        Pair { a: a, b: b }
    }

    func total(&self) -> i32 {
        self.a + self.b
    }
}
"#;
    // `Pair::new` is an associated-function path, not a module path: nothing extra loads.
    assert_eq!(test.compile_and_run("solo.nr", source), Ok(7));
}

#[test]
fn an_item_the_module_does_not_declare_is_reported() {
    let files = [
        ("math.nr", "func double(n: i32) -> i32 { n * 2 }\n"),
        ("main.nr", "func main() -> i32 { math::triple(2) }\n"),
    ];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("declares no item named `triple`"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_name_two_modules_declare_is_reported() {
    let files = [
        ("helper.nr", "func shared(n: i32) -> i32 { n }\n"),
        (
            "main.nr",
            "func shared(n: i32) -> i32 { n }\nfunc main() -> i32 { helper::shared(1) }\n",
        ),
    ];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("share one namespace"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_qualified_path_naming_no_module_is_reported() {
    let files = [(
        "main.nr",
        "func main() -> i32 { missing::inner::value() }\n",
    )];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("does not name a module"),
        "unexpected error: {}",
        error
    );
}
