// End-to-end tests for `import`: the module, list, rename, alias, relative, and enum-variant
// forms, plus the diagnostics for an import that names nothing and a variant used with no
// import behind it.
//
// The unit tests in the `module-resolution` slice cover binding against a stub parser; these
// compile and run the real thing.

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

const MATH: &str = r#"
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
"#;

#[test]
fn a_bare_import_loads_a_module_nothing_else_references() {
    let files = [
        ("math.nr", MATH),
        (
            "main.nr",
            r#"
import math

func main() -> i32 {
    math::double(8)
}
"#,
        ),
    ];

    assert_eq!(run_program(&files, "main.nr"), Ok(16));
}

#[test]
fn imported_names_are_usable_unqualified() {
    let files = [
        ("math.nr", MATH),
        (
            "main.nr",
            r#"
import math::{Vec2, double, SCALE}

func main() -> i32 {
    val made: Vec2 = Vec2::new(2, 3)
    double(made.sum()) + SCALE
}
"#,
        ),
    ];

    // (2 + 3) * 2 + 3 = 13
    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn an_imported_name_may_be_renamed() {
    let files = [
        ("math.nr", MATH),
        (
            "main.nr",
            r#"
import math::{double as twice}
import math::SCALE as factor

func main() -> i32 {
    twice(5) + factor
}
"#,
        ),
    ];

    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn a_module_alias_qualifies_a_path() {
    let files = [
        (
            "utils/mod.nr",
            "export func triple(n: i32) -> i32 { n * 3 }\n",
        ),
        (
            "utils/io.nr",
            "export func width(n: i32) -> i32 { n + 1 }\n",
        ),
        (
            "main.nr",
            r#"
import ./utils::io as reader
import ./utils::{triple}

func main() -> i32 {
    reader::width(9) + triple(2)
}
"#,
        ),
    ];

    // 10 + 6 = 16
    assert_eq!(run_program(&files, "main.nr"), Ok(16));
}

#[test]
fn imported_variants_read_unqualified_in_values_and_patterns() {
    let source = r#"
import Option::{Some, None}

func halve(n: i32) -> Option<i32> {
    if n % 2 == 0 {
        return Some(n / 2)
    }
    None
}

func main() -> i32 {
    val even = match halve(20) {
        Some(half) => half,
        None       => 0
    }
    val odd = halve(7) ?? 5
    even + odd
}
"#;

    // 10 + 5 = 15
    assert_eq!(
        CompileTest::new().compile_and_run("main.nr", source),
        Ok(15)
    );
}

#[test]
fn an_import_naming_no_module_is_reported() {
    let files = [("main.nr", "import nowhere\nfunc main() -> i32 { 0 }\n")];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("does not name a module"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn an_import_of_an_undeclared_name_is_reported() {
    let files = [
        ("math.nr", MATH),
        ("main.nr", "import math::{cbrt}\nfunc main() -> i32 { 0 }\n"),
    ];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("declares no item named `cbrt`"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn an_unimported_variant_pattern_is_reported() {
    let files = [(
        "main.nr",
        r#"
func main() -> i32 {
    match Option::Some(3) {
        Some(n) => n,
        _       => 0
    }
}
"#,
    )];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("no import brings it into scope"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn one_name_may_not_be_imported_twice() {
    let files = [
        ("math.nr", MATH),
        (
            "main.nr",
            "import math::{double}\nimport math::SCALE as double\nfunc main() -> i32 { 0 }\n",
        ),
    ];

    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("is imported twice"),
        "unexpected error: {}",
        error
    );
}
