// End-to-end tests for module namespaces: every module, inline blocks included, holds
// names of its own, a bare name reaches only what is in scope where it is written, and a
// path reaches another module's item in every position a name can take.

use crate::compile_harness::{run_program, stdout_of};

#[test]
fn private_helpers_of_the_same_name_stay_apart() {
    let files = [
        (
            "geo.nr",
            "func helper() -> i32 { 100 }\nexport func area() -> i32 { helper() }\n",
        ),
        (
            "stats.nr",
            "func helper() -> i32 { 10 }\nexport func mean() -> i32 { helper() }\n",
        ),
        (
            "main.nr",
            "func helper() -> i32 { 1 }\n\
             func main() -> i32 { geo::area() + stats::mean() + helper() }\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(111));
}

#[test]
fn an_import_of_two_same_named_items_is_the_error() {
    let files = [
        ("geo.nr", "export func size() -> i32 { 1 }\n"),
        ("stats.nr", "export func size() -> i32 { 2 }\n"),
        (
            "main.nr",
            "import geo::{size}\nimport stats::{size}\nfunc main() -> i32 { size() }\n",
        ),
    ];
    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(error.contains("imported twice"), "{}", error);
}

#[test]
fn an_import_renamed_with_as_resolves_the_clash() {
    let files = [
        ("geo.nr", "export func size() -> i32 { 1 }\n"),
        ("stats.nr", "export func size() -> i32 { 2 }\n"),
        (
            "main.nr",
            "import geo::{size}\nimport stats::{size as stat_size}\n\
             func main() -> i32 { size() * 10 + stat_size() }\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(12));
}

#[test]
fn an_import_may_not_take_the_name_of_a_declaration() {
    let files = [
        ("geo.nr", "export func size() -> i32 { 1 }\n"),
        (
            "main.nr",
            "import geo::{size}\nfunc size() -> i32 { 2 }\nfunc main() -> i32 { size() }\n",
        ),
    ];
    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(error.contains("declares a `size` of its own"), "{}", error);
}

// BUG-138: a private item was reachable by its bare name once its module was loaded.
#[test]
fn regression_bug_138_private_items_are_not_reachable_by_bare_name() {
    let lib = "export func shown() -> i32 { 1 }\n\
               func internal() -> i32 { 4 }\n\
               struct Hidden { v: i32 }\n\
               const SECRET: i32 = 9\n\
               export func own() -> i32 {\n    val h = Hidden { v: SECRET }\n    internal() + h.v\n}\n";
    for main in [
        "import ./lib\nfunc main() -> i32 { internal() }\n",
        "import ./lib\nfunc main() -> i32 {\n    val h: Hidden = Hidden { v: 1 }\n    h.v\n}\n",
        "import ./lib\nfunc main() -> i32 { SECRET }\n",
    ] {
        let files = [("lib.nr", lib), ("main.nr", main)];
        let error = run_program(&files, "main.nr").expect_err("a private item stays private");
        assert!(
            error.contains("internal") || error.contains("Hidden") || error.contains("SECRET"),
            "{}",
            error
        );
    }
    // Inside their own module the same bare names still resolve.
    let files = [
        ("lib.nr", lib),
        ("main.nr", "func main() -> i32 { lib::own() }\n"),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn a_root_function_is_not_in_scope_in_another_module() {
    let files = [
        ("lib.nr", "export func leak() -> i32 { secret() }\n"),
        (
            "main.nr",
            "func secret() -> i32 { 9 }\nfunc main() -> i32 { lib::leak() }\n",
        ),
    ];
    let error = run_program(&files, "main.nr").expect_err("expected a module error");
    assert!(
        error.contains("`secret` is declared in `main.nr`, not in `lib.nr`"),
        "{}",
        error
    );
}

#[test]
fn a_local_shadows_an_imported_name() {
    let files = [
        ("geo.nr", "export func size() -> i32 { 1 }\n"),
        (
            "main.nr",
            "import geo::{size}\n\
             func main() -> i32 {\n    val first = size()\n    val size = 40\n    size + first\n}\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(41));
}

#[test]
fn a_local_shadows_a_module_item_inside_its_module() {
    let files = [
        (
            "geo.nr",
            "func step() -> i32 { 3 }\n\
             export func walk(step: i32) -> i32 { step * 2 }\n\
             export func run() -> i32 { step() }\n",
        ),
        (
            "main.nr",
            "func main() -> i32 { geo::walk(5) + geo::run() }\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn a_qualified_pattern_names_the_enum_through_its_module() {
    let files = [
        (
            "shapes.nr",
            "export enum Shape {\n    Circle(i32),\n    Square(i32)\n}\n",
        ),
        (
            "main.nr",
            "func area(s: shapes::Shape) -> i32 {\n    match s {\n        \
             shapes::Shape::Circle(r) => r * 3,\n        shapes::Shape::Square(w) => w * w\n    }\n}\n\
             func main() -> i32 { area(shapes::Shape::Circle(2)) + area(shapes::Shape::Square(3)) }\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(15));
}

#[test]
fn a_qualified_trait_reaches_impl_dyn_and_impl_trait_positions() {
    let files = [
        (
            "geo.nr",
            "export trait Sized {\n    func size(&self) -> i32\n}\n",
        ),
        (
            "main.nr",
            "struct Dot { r: i32 }\n\
             impl geo::Sized for Dot { func size(&self) -> i32 { self.r } }\n\
             func through_dyn(d: &dyn geo::Sized) -> i32 { d.size() }\n\
             func make(r: i32) -> impl geo::Sized { Dot { r: r } }\n\
             func bounded<T: geo::Sized>(t: &T) -> i32 { t.size() }\n\
             func main() -> i32 {\n    val d = Dot { r: 4 }\n    \
             through_dyn(&d) + make(5).size() + bounded(&d)\n}\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(13));
}

#[test]
fn a_qualified_struct_literal_takes_a_base() {
    let files = [
        (
            "geo.nr",
            "export struct Point { export x: i32, export y: i32 }\n\
             export func origin() -> Point { Point { x: 0, y: 9 } }\n",
        ),
        (
            "main.nr",
            "func main() -> i32 {\n    val p = geo::Point { x: 2, ..geo::origin() }\n    p.x + p.y\n}\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(11));
}

#[test]
fn a_trait_default_reaches_an_impl_in_another_module() {
    let files = [
        (
            "geo.nr",
            "export trait Sized {\n    func size(&self) -> i32\n    \
             func doubled(&self) -> i32 { self.size() * 2 }\n}\n",
        ),
        (
            "main.nr",
            "import geo::{Sized}\nstruct Dot { r: i32 }\n\
             impl Sized for Dot { func size(&self) -> i32 { self.r } }\n\
             func main() -> i32 {\n    val d = Dot { r: 4 }\n    d.doubled()\n}\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(8));
}

#[test]
fn a_bare_path_starts_at_the_program_root() {
    let files = [
        ("io.nr", "export func read() -> i32 { 1 }\n"),
        ("utils/io.nr", "export func read() -> i32 { 2 }\n"),
        (
            "utils/mod.nr",
            "import ./io as local\nexport func both() -> i32 { io::read() * 10 + local::read() }\n",
        ),
        ("main.nr", "func main() -> i32 { utils::both() }\n"),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(12));
}

#[test]
fn a_parent_relative_import_climbs_above_the_file() {
    let files = [
        ("shared/mod.nr", "// a directory module\n"),
        (
            "shared/util.nr",
            "export func triple(v: i32) -> i32 { v * 3 }\n",
        ),
        (
            "app/main.nr",
            "import ../shared::util::{triple}\nfunc main() -> i32 { triple(4) }\n",
        ),
    ];
    assert_eq!(run_program(&files, "app/main.nr"), Ok(12));
}

#[test]
fn a_single_variant_import_binds_one_variant() {
    let files = [(
        "main.nr",
        "enum Light {\n    Red,\n    Green\n}\nimport Light::Green\nimport Light::Red as Stop\n\
         func main() -> i32 {\n    match Green {\n        Stop => 1,\n        Green => 2\n    }\n}\n",
    )];
    assert_eq!(run_program(&files, "main.nr"), Ok(2));
}

#[test]
fn a_block_reaches_the_private_items_of_the_module_around_it() {
    let files = [(
        "main.nr",
        "func base() -> i32 { 5 }\n\
         module inner {\n    export func twice() -> i32 { base() * 2 }\n}\n\
         func main() -> i32 { inner::twice() }\n",
    )];
    assert_eq!(run_program(&files, "main.nr"), Ok(10));
}

#[test]
fn derived_debug_prints_the_declared_name() {
    let printed = stdout_of(
        "main.nr",
        "module geo {\n    @derive(Debug)\n    export struct Point { export x: i32 }\n}\n\
         func main() -> i32 {\n    val p = geo::Point { x: 3 }\n    println(\"{p:?}\")\n    0\n}\n",
    );
    assert_eq!(printed, "Point { x: 3 }\n");
}
