// End-to-end tests for inline `module { }` blocks and `export import` re-exports: the two
// halves of the module system that need no file of their own.
//
// The unit tests in the `module-resolution` slice cover resolution against a stub parser;
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

fn run_single(source: &str) -> Result<i32, String> {
    CompileTest::new().compile_and_run("main.nr", source)
}

#[test]
fn a_block_supplies_structs_functions_and_constants() {
    let exit = run_single(
        r#"
module geometry {
    export struct Circle {
        export radius: i32
    }

    impl Circle {
        func new(r: i32) -> Circle {
            Circle { radius: r }
        }

        func doubled(&self) -> i32 {
            scale(self.radius)
        }
    }

    export const UNIT: i32 = 1

    export func area(c: &Circle) -> i32 {
        c.radius * c.radius
    }

    // Private to the block, but reachable from inside it.
    func scale(v: i32) -> i32 {
        v * 2
    }
}

func main() -> i32 {
    val made = geometry::Circle::new(3)
    val built = geometry::Circle { radius: 2 }
    geometry::area(&made) + built.radius + made.doubled() + geometry::UNIT
}
"#,
    )
    .expect("program should compile and run");
    assert_eq!(exit, 18);
}

#[test]
fn a_block_item_is_private_unless_exported() {
    let error = run_single(
        "module geometry {\n    func scale(v: i32) -> i32 { v * 2 }\n}\n\
         func main() -> i32 { geometry::scale(4) }\n",
    )
    .expect_err("expected a visibility error");
    assert!(error.contains("is private to module"), "{}", error);
}

#[test]
fn blocks_nest_and_an_import_reaches_into_one() {
    let exit = run_single(
        r#"
module outer {
    module inner {
        export func deep() -> i32 { 7 }
    }

    export func reach() -> i32 { inner::deep() + 1 }
}

import outer::{reach}

func main() -> i32 { reach() + outer::inner::deep() }
"#,
    )
    .expect("program should compile and run");
    assert_eq!(exit, 15);
}

#[test]
fn a_block_wins_over_a_same_named_file() {
    let files = [
        ("geometry.nr", "export func area() -> i32 { 99 }\n"),
        (
            "main.nr",
            "module geometry {\n    export func area() -> i32 { 4 }\n}\n\
             func main() -> i32 { geometry::area() }\n",
        ),
    ];
    let exit = run_program(&files, "main.nr").expect("program should compile and run");
    assert_eq!(exit, 4);
}

#[test]
fn export_import_re_exports_through_a_facade() {
    let files = [
        (
            "internal.nr",
            "export struct Config { export timeout: i32 }\n\
             export func parse_config(t: i32) -> Config { Config { timeout: t } }\n",
        ),
        (
            "facade.nr",
            "export import ./internal::{Config, parse_config as build}\n",
        ),
        (
            "main.nr",
            r#"
import facade::{Config, build}

func main() -> i32 {
    val direct: Config = build(7)
    val qualified: facade::Config = facade::build(3)
    direct.timeout + qualified.timeout
}
"#,
        ),
    ];
    let exit = run_program(&files, "main.nr").expect("program should compile and run");
    assert_eq!(exit, 10);
}

#[test]
fn a_chain_of_re_exports_resolves() {
    let files = [
        ("deep.nr", "export func value() -> i32 { 5 }\n"),
        ("mid.nr", "export import ./deep::{value}\n"),
        ("top.nr", "export import ./mid::{value as v}\n"),
        ("main.nr", "func main() -> i32 { top::v() }\n"),
    ];
    let exit = run_program(&files, "main.nr").expect("program should compile and run");
    assert_eq!(exit, 5);
}

#[test]
fn a_plain_import_does_not_re_export() {
    let files = [
        ("internal.nr", "export func parse() -> i32 { 1 }\n"),
        ("facade.nr", "import ./internal::{parse}\n"),
        ("main.nr", "func main() -> i32 { facade::parse() }\n"),
    ];
    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(error.contains("declares no item named"), "{}", error);
}

#[test]
fn export_import_of_a_module_is_reported() {
    let files = [
        ("internal.nr", "export func parse() -> i32 { 1 }\n"),
        (
            "main.nr",
            "export import ./internal\nfunc main() -> i32 { internal::parse() }\n",
        ),
    ];
    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(error.contains("rather than an item"), "{}", error);
}

#[test]
fn two_blocks_may_not_share_a_name() {
    let error = run_single(
        "module m {\n    export func a() -> i32 { 1 }\n}\n\
         module m {\n    export func b() -> i32 { 2 }\n}\n\
         func main() -> i32 { m::a() }\n",
    )
    .expect_err("expected a module error");
    assert!(
        error.contains("declared twice as an inline module"),
        "{}",
        error
    );
}

#[test]
fn export_is_rejected_on_a_block() {
    let error = run_single("export module m { }\nfunc main() -> i32 { 0 }\n")
        .expect_err("expected a parse error");
    assert!(error.contains("`export` cannot be applied to"), "{}", error);
}

#[test]
fn a_sibling_block_in_an_import_path_is_reported_as_an_invisible_module() {
    // A block sees its own children and the file's siblings, not its parent's other
    // blocks. The path therefore resolves no module — but it must not be silently
    // re-read as an enum whose variants are being imported.
    let error = run_single(
        "module leaf   { export const BASE: i32 = 8 }\n\
         module facade { export import leaf::{BASE} }\n\
         func main() -> i32 { facade::BASE }\n",
    )
    .expect_err("expected a module error");
    assert!(
        !error.contains("enum variant"),
        "`BASE` is a const, not a variant; got: {}",
        error
    );
    assert!(
        error.contains("inline module"),
        "diagnostic should name the invisible block; got: {}",
        error
    );
}

#[test]
fn a_plain_import_of_a_sibling_block_is_reported_the_same_way() {
    // Dropping the `export` must not make the error go away: the plain form was
    // silently accepted as a variant binding of an enum that does not exist.
    let error = run_single(
        "module leaf   { export const BASE: i32 = 8 }\n\
         module facade { import leaf::{BASE}\n export func get() -> i32 { BASE } }\n\
         func main() -> i32 { facade::get() }\n",
    )
    .expect_err("expected a module error");
    assert!(
        error.contains("inline module"),
        "diagnostic should name the invisible block; got: {}",
        error
    );
}

#[test]
fn an_import_whose_head_names_nothing_is_reported() {
    // The single-segment fallback exists for enums — `import Option::{Some}`, whose
    // enum the prelude supplies later. A head that names no module and no enum
    // anywhere must not slip through it unnoticed.
    let error = run_single("import Nothing::{AtAll}\nfunc main() -> i32 { 0 }\n")
        .expect_err("expected a module error");
    assert!(
        error.contains("Nothing"),
        "diagnostic should name the unresolvable head; got: {}",
        error
    );
}

#[test]
fn an_explicit_import_of_a_prelude_enum_still_resolves() {
    // The reason the fallback exists: the prelude is prepended after import
    // resolution, so `Option` names no module and no *declared* enum at this point.
    let exit = run_single(
        "import Option::{Some as Just}\n\
         func main() -> i32 {\n\
         \x20   val v = Just(4)\n\
         \x20   match v { Option::Some(n) => n, Option::None => 0 }\n\
         }\n",
    )
    .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn an_import_of_a_locally_declared_enum_still_resolves() {
    let exit = run_single(
        "enum Color { Red, Green }\n\
         import Color::{Red}\n\
         func main() -> i32 {\n\
         \x20   match Red { Color::Red => 5, Color::Green => 0 }\n\
         }\n",
    )
    .expect("compile/run failed");
    assert_eq!(exit, 5);
}
