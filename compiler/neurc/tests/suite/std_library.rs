// End-to-end tests for the standard library rooted at `std`: `std::prelude` as a module a
// program can name and import from, `@no_prelude` taking a file's prelude names away
// whichever file carries it, and a root declaration with a prelude name living beside the
// library's (LIM-027, LIM-272).

use crate::compile_harness::{assert_exit, rejected, run_program};

/// `half` answers `Option<i32>` and every program below reads `half(8)` and `half(5)` back
/// out, so each must return 4 + 9 = 13 however it names `Option`, `Some` and `None`.
#[test]
fn every_spelling_of_the_prelude_reaches_the_same_option() {
    let bare = r#"
func half(n: i32) -> Option<i32> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}
func main() -> i32 {
    val a = half(8) ?? 0
    val b = match half(5) { Some(v) => v, None => 9 }
    a + b
}
"#;
    let full_path = r#"
func half(n: i32) -> std::prelude::Option<i32> {
    if n % 2 == 0 {
        std::prelude::Option::Some(n / 2)
    } else {
        std::prelude::Option::None
    }
}
func main() -> i32 {
    val b = match half(5) {
        std::prelude::Option::Some(v) => v,
        std::prelude::Option::None => 9
    }
    (half(8) ?? 0) + b
}
"#;
    let module_alias = r#"
import std::prelude as pre
func half(n: i32) -> pre::Option<i32> {
    if n % 2 == 0 { pre::Option::Some(n / 2) } else { pre::Option::None }
}
func pick(o: pre::Option<i32>, fallback: i32) -> i32 {
    match o { pre::Option::Some(v) => v, pre::Option::None => fallback }
}
func main() -> i32 {
    pick(half(8), 0) + pick(half(5), 9)
}
"#;
    let opted_out = r#"
@no_prelude
import std::prelude::{Option as Maybe, Some, None}
func half(n: i32) -> Maybe<i32> {
    if n % 2 == 0 { Some(n / 2) } else { None }
}
func main() -> i32 {
    val read = |o: Maybe<i32>, fallback: i32| match o { Some(v) => v, None => fallback }
    read(half(8), 0) + read(half(5), 9)
}
"#;
    for (name, source) in [
        ("bare.nr", bare),
        ("full_path.nr", full_path),
        ("module_alias.nr", module_alias),
        ("opted_out.nr", opted_out),
    ] {
        assert_exit(name, source, 13);
    }
}

#[test]
fn no_prelude_takes_a_bare_prelude_type_away_and_names_the_import() {
    let error = rejected(
        "strict.nr",
        "@no_prelude\nfunc f() -> Option<i32> { Option::None }\nfunc main() -> i32 { 0 }\n",
    );
    assert!(
        error.contains("`Option` comes from `std::prelude`")
            && error.contains("import std::prelude::{Option}"),
        "{error}"
    );
}

#[test]
fn no_prelude_names_the_qualified_path_of_a_variant() {
    let error = rejected(
        "strict.nr",
        "@no_prelude\nfunc main() -> i32 {\n    val x = Some(1)\n    0\n}\n",
    );
    assert!(error.contains("std::prelude::Option::Some"), "{error}");
}

/// LIM-027: a non-root file's `@no_prelude` used to take its variants only, leaving `Option`
/// reachable by its bare name. It takes both now, and the modules that kept the prelude are
/// untouched.
#[test]
fn no_prelude_on_a_non_root_file_takes_its_types_away_too() {
    let strict = "@no_prelude\nexport func maybe() -> Option<i32> { Option::None }\n";
    let main = "import strict::{maybe}\nfunc main() -> i32 { maybe() ?? 3 }\n";
    let error = run_program(&[("strict.nr", strict), ("main.nr", main)], "main.nr")
        .expect_err("`Option` is not in scope in strict.nr");
    assert!(
        error.contains("`Option` comes from `std::prelude`") && error.contains("strict.nr"),
        "{error}"
    );

    let fixed = "@no_prelude\nimport std::prelude::{Option}\nexport func maybe() -> Option<i32> { Option::None }\n";
    assert_eq!(
        run_program(&[("strict.nr", fixed), ("main.nr", main)], "main.nr"),
        Ok(3)
    );
}

/// The traits a `for` loop and `@derive` dispatch through stay in scope in an opted-out
/// file, and the library is compiled with it all the same: `.parse` and range values work.
#[test]
fn an_opted_out_file_keeps_the_language_traits_and_the_library() {
    let source = r#"
@no_prelude
import std::prelude::{Option, Some, None}

struct Countdown { n: i32 }

impl Iterator for Countdown {
    type Item = i32
    func next(&mut self) -> Option<i32> {
        if self.n == 0 {
            return None
        }
        self.n -= 1
        Some(self.n)
    }
}

func main() -> i32 {
    mut total = 0
    val down = Countdown { n: 4 }
    for v in down {
        total += v
    }
    val r = 0..3
    for i in r {
        total += i
    }
    total + ("30".parse::<i32>() ?? 0)
}
"#;
    assert_exit("opted_out_library.nr", source, 39);
}

/// LIM-272: the root's `Result` once replaced the library's in every module, so `?` in
/// another module failed. Both are in the program now.
#[test]
fn a_root_result_coexists_with_the_library_s() {
    let checks = r#"
export func halve(n: i32) -> Result<i32, i32> {
    if n % 2 == 0 { Ok(n / 2) } else { Err(n) }
}
export func quarter(n: i32) -> Result<i32, i32> {
    val h = halve(n)?
    halve(h)
}
"#;
    let main = r#"
import checks::{quarter}

enum Result { Fine, Broken }

func verdict(n: i32) -> Result {
    if n > 0 { Result::Fine } else { Result::Broken }
}

func main() -> i32 {
    val q = quarter(20) ?? 0
    match verdict(q) {
        Result::Fine => q,
        Result::Broken => 0
    }
}
"#;
    assert_eq!(
        run_program(&[("checks.nr", checks), ("main.nr", main)], "main.nr"),
        Ok(5)
    );
}

/// `std` names the library from every module, so a `std.nr` beside the program is never
/// what `std::prelude` reaches.
#[test]
fn a_std_file_beside_the_program_does_not_shadow_the_library() {
    let decoy = "export func prelude() -> i32 { 1 }\n";
    let main = "func main() -> i32 {\n    std::prelude::Option::Some(6) ?? 0\n}\n";
    assert_eq!(
        run_program(&[("std.nr", decoy), ("main.nr", main)], "main.nr"),
        Ok(6)
    );
}

#[test]
fn a_name_the_prelude_does_not_declare_is_reported() {
    let error = rejected(
        "missing.nr",
        "import std::prelude::{Maybe}\nfunc main() -> i32 { 0 }\n",
    );
    assert!(
        error.contains("module `std::prelude` declares no item named `Maybe`"),
        "{error}"
    );
}

/// `Device` keeps its variants qualified (BUG-140), but as an enum the prelude declares it is
/// in scope, so its variants can be imported like any enum's.
#[test]
fn a_qualified_prelude_enum_s_variants_can_be_imported() {
    let source = "import Device::{CPU}\nfunc main() -> i32 {\n    val t: Tensor<f32, [2]> = [1.0, 2.0]\n    val moved = t.to(CPU)\n    7\n}\n";
    assert_exit("device_import.nr", source, 7);
}
