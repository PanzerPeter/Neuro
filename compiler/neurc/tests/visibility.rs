// End-to-end tests for `export`: what crosses a module boundary and what does not.
//
// Item visibility is settled during module resolution, which knows both the referencing
// file and the owning one. Field visibility needs the receiver's type, so it is settled by
// the type checker against the module each item carries — these tests exercise both paths
// through the real compiler.

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

/// The module a program's public surface is taken from throughout these tests.
const LIB: &str = r#"
export struct Config {
    export host: i32,
    timeout: i32
}

impl Config {
    func new(host: i32) -> Config {
        Config { host: host, timeout: 30 }
    }

    func timeout(&self) -> i32 {
        self.timeout
    }
}

export func make(host: i32) -> Config {
    Config::new(host)
}

func internal(n: i32) -> i32 {
    n * 2
}

export func doubled(n: i32) -> i32 {
    internal(n)
}
"#;

fn rejected(main: &str) -> String {
    run_program(&[("lib.nr", LIB), ("main.nr", main)], "main.nr")
        .expect_err("the program should be rejected")
}

#[test]
fn an_exported_surface_is_reachable_across_the_boundary() {
    let main = r#"
func main() -> i32 {
    val c = lib::make(5)
    c.host + lib::doubled(6) + c.timeout()
}
"#;
    // 5 + 12 + 30 = 47. The private field is read through the method that owns it.
    assert_eq!(
        run_program(&[("lib.nr", LIB), ("main.nr", main)], "main.nr"),
        Ok(47)
    );
}

#[test]
fn a_private_item_is_not_reachable_across_the_boundary() {
    let error = rejected("func main() -> i32 { lib::internal(2) }\n");
    assert!(
        error.contains("`internal` is private to module `lib`"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_private_field_cannot_be_read_across_the_boundary() {
    let error = rejected("func main() -> i32 { lib::make(1).timeout }\n");
    assert!(
        error.contains("field 'timeout'") && error.contains("is private"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_private_field_cannot_be_written_across_the_boundary() {
    let main = r#"
func main() -> i32 {
    mut c = lib::make(1)
    c.timeout = 5
    c.host
}
"#;
    let error = rejected(main);
    assert!(
        error.contains("field 'timeout'") && error.contains("is private"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_private_field_cannot_be_initialized_across_the_boundary() {
    let main = r#"
import lib::{Config}

func main() -> i32 {
    val c = Config { host: 1, timeout: 2 }
    c.host
}
"#;
    let error = rejected(main);
    assert!(
        error.contains("field 'timeout'") && error.contains("is private"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn a_struct_update_cannot_copy_a_private_field_across_the_boundary() {
    let main = r#"
import lib::{Config, make}

func main() -> i32 {
    val base = make(1)
    val c = Config { host: 2, ..base }
    c.host
}
"#;
    let error = rejected(main);
    assert!(
        error.contains("field 'timeout'") && error.contains("is private"),
        "unexpected error: {}",
        error
    );
}

#[test]
fn visibility_is_inert_inside_a_single_file_program() {
    let source = r#"
struct Config {
    host: i32,
    timeout: i32
}

func main() -> i32 {
    val c = Config { host: 5, timeout: 7 }
    c.host + c.timeout
}
"#;
    // One file is one module, so a field with no `export` is reachable throughout it.
    assert_eq!(
        CompileTest::new().compile_and_run("solo.nr", source),
        Ok(12)
    );
}

#[test]
fn a_generic_struct_instance_keeps_its_templates_field_visibility() {
    let lib = r#"
export struct Boxed<T> {
    export label: i32,
    payload: T
}

export func wrap(value: i32) -> Boxed<i32> {
    Boxed { label: 1, payload: value }
}
"#;
    let main = r#"
func main() -> i32 {
    lib::wrap(4).payload
}
"#;
    let error = run_program(&[("lib.nr", lib), ("main.nr", main)], "main.nr")
        .expect_err("the program should be rejected");
    assert!(
        error.contains("field 'payload'") && error.contains("is private"),
        "unexpected error: {}",
        error
    );
}
