// End-to-end tests for the implicit prelude: `Some` / `None` / `Ok` / `Err` written with
// no import, in every module of a program, and the `@no_prelude` opt-out.
//
// The unit tests in the `module-resolution` slice cover the binding table against a stub
// parser; these compile and run the real thing.

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

fn run(source: &str) -> Result<i32, String> {
    CompileTest::new().compile_and_run("main.nr", source)
}

#[test]
fn option_variants_need_no_import() {
    let exit = run(r#"
func half(n: i32) -> Option<i32> {
    if n % 2 == 0 {
        Some(n / 2)
    } else {
        None
    }
}

func main() -> i32 {
    val even = match half(10) {
        Some(v) => v,
        None    => 0
    }
    val odd = match half(7) {
        Some(v) => v,
        None    => 1
    }
    even + odd
}
"#)
    .expect("program compiles and runs");

    assert_eq!(exit, 6);
}

#[test]
fn result_variants_need_no_import() {
    let exit = run(r#"
func checked(n: i32) -> Result<i32, i32> {
    if n < 0 {
        return Err(0 - n)
    }
    Ok(n * 2)
}

func main() -> i32 {
    val good = match checked(10) {
        Ok(v)  => v,
        Err(e) => e
    }
    val bad = match checked(0 - 5) {
        Ok(v)  => v,
        Err(e) => e
    }
    good + bad
}
"#)
    .expect("program compiles and runs");

    assert_eq!(exit, 25);
}

#[test]
fn the_prelude_reaches_a_non_root_module() {
    let files = [
        (
            "parsing.nr",
            r#"
export func digit(c: i32) -> Option<i32> {
    if c >= 48 {
        Some(c - 48)
    } else {
        None
    }
}
"#,
        ),
        (
            "main.nr",
            r#"
import parsing::{digit}

func main() -> i32 {
    match digit(55) {
        Some(v) => v,
        None    => 0
    }
}
"#,
        ),
    ];

    assert_eq!(
        run_program(&files, "main.nr").expect("program compiles and runs"),
        7
    );
}

#[test]
fn the_prelude_reaches_an_inline_module_block() {
    let exit = run(r#"
module lookup {
    export func at(index: i32) -> Option<i32> {
        if index == 0 {
            Some(9)
        } else {
            None
        }
    }
}

func main() -> i32 {
    match lookup::at(0) {
        Some(v) => v,
        None    => 0
    }
}
"#)
    .expect("program compiles and runs");

    assert_eq!(exit, 9);
}

#[test]
fn an_explicit_variant_import_still_compiles() {
    let exit = run(r#"
import Option::{Some, None}

func main() -> i32 {
    val value: Option<i32> = Some(4)
    match value {
        Some(v) => v,
        None    => 0
    }
}
"#)
    .expect("the explicit import wins over the implicit one rather than colliding");

    assert_eq!(exit, 4);
}

#[test]
fn a_local_declaration_shadows_a_prelude_name() {
    let exit = run(r#"
func None() -> i32 {
    11
}

func main() -> i32 {
    None()
}
"#)
    .expect("program compiles and runs");

    assert_eq!(exit, 11);
}

#[test]
fn no_prelude_compiles_a_program_that_needs_none_of_it() {
    let exit = run(r#"@no_prelude

func main() -> i32 {
    val total = 6 * 7
    total
}
"#)
    .expect("program compiles and runs");

    assert_eq!(exit, 42);
}

#[test]
fn no_prelude_takes_the_variant_bindings_away() {
    let error = run(r#"@no_prelude

func main() -> i32 {
    match Some(1) {
        Some(v) => v,
        None    => 0
    }
}
"#)
    .expect_err("the bare variants no longer resolve");

    assert!(
        error.contains("Some"),
        "the diagnostic should name the unresolved variant: {}",
        error
    );
}

#[test]
fn no_prelude_takes_the_declarations_away_too() {
    let error = run(r#"@no_prelude

func maybe() -> Option<i32> {
    Option::Some(1)
}

func main() -> i32 {
    0
}
"#)
    .expect_err("`Option` is not declared without the prelude");

    assert!(
        error.contains("Option"),
        "the diagnostic should name the missing type: {}",
        error
    );
}

#[test]
fn a_misplaced_no_prelude_is_reported() {
    let error = run(r#"
func main() -> i32 { 0 }

@no_prelude
"#)
    .expect_err("the marker is not at the top of the file");

    assert!(
        error.contains("@no_prelude"),
        "the diagnostic should name the marker: {}",
        error
    );
}
