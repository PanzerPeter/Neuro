// End-to-end tests for `match` patterns that look inside a value: alternatives that bind,
// payload sub-patterns that test or nest, `@` bindings, and array patterns over arrays and
// slices, with the drops each must release exactly once.

use std::process::Command;

use crate::compile_harness::{CompileTest, run};

const TOKENS: &str = r#"
enum Tok { Num(i32), Neg(i32), Word(i32, bool), End }
enum Wrap { Inner(Tok), Pair(Tok, Tok), Nothing }
"#;

#[test]
fn alternatives_bind_from_whichever_one_matched() {
    let source = format!(
        r#"{TOKENS}
func value(t: Tok) -> i32 {{
    match t {{
        Tok::Num(x) | Tok::Neg(x) | Tok::Word(x, _) => x,
        Tok::End => 0
    }}
}}
func main() -> i32 {{
    (value(Tok::Num(1)) + value(Tok::Neg(2)) * 10 + value(Tok::Word(3, true)) * 100) % 256
}}
"#
    );
    assert_eq!(run("alt_bind.nr", &source), 321 % 256);
}

#[test]
fn alternatives_may_bind_one_name_from_different_positions() {
    let source = r#"
enum Two { A(i32, i32), B(i32, i32) }
func diff(t: Two) -> i32 {
    match t {
        Two::A(x, y) | Two::B(y, x) => x - y
    }
}
func main() -> i32 { diff(Two::A(9, 2)) * 10 + diff(Two::B(2, 9)) }
"#;
    assert_eq!(run("alt_positions.nr", source), 77);
}

#[test]
fn a_failed_guard_tries_the_next_alternative_with_its_bindings() {
    // `P(1, 5)` matches both alternatives: `x` is 1 in the first, whose guard fails, then 5
    // in the second.
    let source = r#"
enum P { P(i32, i32) }
func pick(p: P) -> i32 {
    match p {
        P::P(x, 5) | P::P(1, x) if x > 2 => x,
        _ => 0
    }
}
func main() -> i32 {
    val first = pick(P::P(1, 5))
    val second = pick(P::P(7, 5))
    val none = pick(P::P(1, 2))
    first * 10 + second + none
}
"#;
    assert_eq!(run("alt_guard.nr", source), 57);
}

#[test]
fn payload_sub_patterns_test_values_and_nest_enums() {
    let source = format!(
        r#"{TOKENS}
func score(w: Wrap) -> i32 {{
    match w {{
        Wrap::Inner(Tok::Num(0)) => 1,
        Wrap::Inner(Tok::Num(1..=9)) => 2,
        Wrap::Inner(Tok::Num(n)) => n,
        Wrap::Inner(Tok::Word(a, true)) => a + 3,
        Wrap::Inner(Tok::Word(a, false)) => a + 4,
        Wrap::Inner(_) => 5,
        Wrap::Pair(Tok::End, Tok::End) => 6,
        Wrap::Pair(Tok::Num(a), Tok::Num(b)) | Wrap::Pair(Tok::Neg(a), Tok::Neg(b)) => a + b,
        Wrap::Pair(_, _) => 7,
        Wrap::Nothing => 8
    }}
}}
func main() -> i32 {{
    mut failures = 0
    if score(Wrap::Inner(Tok::Num(0))) != 1 {{ failures += 1 }}
    if score(Wrap::Inner(Tok::Num(9))) != 2 {{ failures += 1 }}
    if score(Wrap::Inner(Tok::Num(40))) != 40 {{ failures += 1 }}
    if score(Wrap::Inner(Tok::Word(10, true))) != 13 {{ failures += 1 }}
    if score(Wrap::Inner(Tok::Word(10, false))) != 14 {{ failures += 1 }}
    if score(Wrap::Inner(Tok::End)) != 5 {{ failures += 1 }}
    if score(Wrap::Pair(Tok::End, Tok::End)) != 6 {{ failures += 1 }}
    if score(Wrap::Pair(Tok::Num(20), Tok::Num(1))) != 21 {{ failures += 1 }}
    if score(Wrap::Pair(Tok::Neg(30), Tok::Neg(2))) != 32 {{ failures += 1 }}
    if score(Wrap::Pair(Tok::Num(1), Tok::End)) != 7 {{ failures += 1 }}
    if score(Wrap::Nothing) != 8 {{ failures += 1 }}
    failures
}}
"#
    );
    assert_eq!(run("nested.nr", &source), 0);
}

#[test]
fn a_struct_variant_field_takes_a_pattern() {
    let source = r#"
enum Shape { Circle { r: i32 }, Rect { w: i32, h: i32 }, Dot }
func area(s: Shape) -> i32 {
    match s {
        Shape::Circle { r: 0 } => 1,
        Shape::Circle { .. } => 2,
        Shape::Rect { w: 1, h } | Shape::Rect { w: h, h: 1 } => h,
        Shape::Rect { w, .. } => w,
        Shape::Dot => 3
    }
}
func main() -> i32 {
    area(Shape::Circle { r: 0 }) + area(Shape::Circle { r: 5 }) * 2
        + area(Shape::Rect { w: 1, h: 10 }) + area(Shape::Rect { w: 20, h: 1 })
        + area(Shape::Rect { w: 30, h: 2 }) + area(Shape::Dot)
}
"#;
    assert_eq!(run("struct_fields.nr", source), 1 + 4 + 10 + 20 + 30 + 3);
}

#[test]
fn nested_generic_instances_resolve_at_each_level() {
    let source = r#"
func pick<T>(o: Option<Option<T>>, fallback: T) -> T {
    match o {
        Some(Some(x)) => x,
        Some(None) | None => fallback
    }
}
func parse(r: Result<Option<i32>, i32>) -> i32 {
    match r {
        Ok(Some(n)) => n,
        Ok(None) => 1,
        Err(0) => 2,
        Err(e) => e
    }
}
func main() -> i32 {
    val empty: Option<Option<i32>> = Some(None)
    pick(Some(Some(30)), 0) + pick(empty, 4) + parse(Ok(Some(10))) + parse(Ok(None))
        + parse(Err(0)) + parse(Err(50))
}
"#;
    assert_eq!(run("generic_nested.nr", source), 30 + 4 + 10 + 1 + 2 + 50);
}

#[test]
fn an_at_binding_keeps_the_value_a_pattern_tested() {
    let source = r#"
func digit(code: i32) -> i32 {
    match code {
        n @ 1..=9 => n * 10,
        other => other
    }
}
func inner(o: Option<i32>) -> i32 {
    match o {
        whole @ Some(1..=5) => whole ?? 0,
        Some(n @ 6..=9) => n * 2,
        w @ Some(v) => v + (w ?? 0),
        None => 0
    }
}
func main() -> i32 {
    digit(7) + digit(20) + inner(Some(3)) + inner(Some(7)) + inner(Some(20)) + inner(None)
}
"#;
    assert_eq!(run("at.nr", source), 70 + 20 + 3 + 14 + 40);
}

#[test]
fn array_patterns_split_a_fixed_array() {
    let source = r#"
func read(a: [i32; 3]) -> i32 {
    match a {
        [0, x, y] => x + y,
        [first, ..rest] if first > 100 => rest[0] * rest[1],
        [_, 7, ..] => 7,
        [p, q, r] => p * q * r
    }
}
func main() -> i32 {
    val a = [2, 3, 4]
    read([0, 1, 2]) + read([200, 2, 5]) + read([5, 7, 2]) + read(a) + a[0]
}
"#;
    assert_eq!(run("array_fixed.nr", source), 3 + 10 + 7 + 24 + 2);
}

#[test]
fn over_a_size_parameter_a_pattern_of_another_length_never_matches() {
    let source = r#"
func first<const N: u64>(a: [i32; N]) -> i32 {
    match a {
        [] => 1,
        [x, ..] => x
    }
}
func pair<const N: u64>(a: [i32; N]) -> i32 {
    match a {
        [x, y] => x + y,
        [_, _, ..rest] => rest[0],
        _ => 2
    }
}
func main() -> i32 {
    first([7, 8]) + pair([10, 20]) * 2 + pair([1, 2, 30]) + pair([1])
}
"#;
    assert_eq!(run("array_size_param.nr", source), 7 + 60 + 30 + 2);
}

#[test]
fn patterns_compose_across_arrays_enums_and_slices() {
    let source = r#"
enum Grid { Cells([i32; 2]), Row(&[i32]), Blank }
func elements(a: [Option<i32>; 2]) -> i32 {
    match a {
        [Some(x), None] | [None, Some(x)] => x,
        [Some(x), Some(y)] => x + y,
        [None, None] => 0
    }
}
func inside(g: Grid) -> i32 {
    match g {
        Grid::Cells([1, y]) => y,
        Grid::Cells([x, ..]) => x,
        Grid::Row([x, ..]) => x,
        Grid::Row([]) => 1,
        Grid::Blank => 0
    }
}
func main() -> i32 {
    val row = [40, 41]
    val s: &[i32] = &row
    val empty: &[i32] = s.slice(0..0)
    mut failures = 0
    if elements([Some(3), None]) != 3 { failures += 1 }
    if elements([None, Some(4)]) != 4 { failures += 1 }
    if elements([Some(3), Some(4)]) != 7 { failures += 1 }
    if inside(Grid::Cells([1, 9])) != 9 { failures += 1 }
    if inside(Grid::Cells([2, 9])) != 2 { failures += 1 }
    if inside(Grid::Row(s)) != 40 { failures += 1 }
    if inside(Grid::Row(empty)) != 1 { failures += 1 }
    failures
}
"#;
    assert_eq!(run("compose.nr", source), 0);
}

#[test]
fn array_patterns_walk_a_slice_by_length() {
    let source = r#"
func sum(s: &[i32]) -> i32 {
    match s {
        [] => 0,
        [head, ..rest] => head + sum(rest)
    }
}
func shape(s: &[i32]) -> i32 {
    match s {
        [] => 1,
        [_] => 2,
        [_, _] => 3,
        [0, ..] => 4,
        [_, ..tail] => 5 + (tail.len() as i32)
    }
}
func tail(s: &[i32]) -> &[i32] {
    match s {
        [_, ..rest] => rest,
        [] => s
    }
}
func main() -> i32 {
    val a = [1, 2, 3, 4]
    val zero = [0, 9, 9]
    val s: &[i32] = &a
    val z: &[i32] = &zero
    mut failures = 0
    if sum(s) != 10 { failures += 1 }
    if shape(s.slice(0..0)) != 1 { failures += 1 }
    if shape(s.slice(0..1)) != 2 { failures += 1 }
    if shape(s.slice(0..2)) != 3 { failures += 1 }
    if shape(z) != 4 { failures += 1 }
    if shape(s) != 8 { failures += 1 }
    if tail(s).len() != 3 { failures += 1 }
    if tail(s)[0] != 2 { failures += 1 }
    failures
}
"#;
    assert_eq!(run("array_slice.nr", source), 0);
}

#[test]
fn a_borrow_is_matched_through_at_every_level() {
    let source = r#"
enum Kind { Num(i32), Blank }
func direct(k: &Kind) -> i32 {
    match k {
        Kind::Num(n @ 1..=5) => n,
        Kind::Num(_) | Kind::Blank => 0
    }
}
func held(o: Option<&Kind>) -> i32 {
    match o {
        Some(Kind::Num(n)) => n,
        Some(Kind::Blank) => 1,
        None => 2
    }
}
func fixed(a: &[i32; 2]) -> i32 {
    match a {
        [x, y] => x * y
    }
}
func main() -> i32 {
    val four = Kind::Num(4)
    val blank = Kind::Blank
    val pair = [3, 5]
    direct(&four) + direct(&blank) + held(Some(&four)) * 10 + held(Some(&blank)) * 100
        + held(None) + fixed(&pair)
}
"#;
    assert_eq!(run("borrow.nr", source), 4 + 40 + 100 + 2 + 15);
}

#[test]
fn every_single_pattern_form_takes_the_new_patterns() {
    let source = r#"
func pairs(s: &[i32]) -> i32 {
    mut cur = s
    mut total = 0
    while val [a, b, ..rest] = cur {
        total += a * b
        cur = rest
    }
    if val [last] = cur { total += last }
    total
}
func unwrap(r: Result<Option<i32>, i32>) -> i32 {
    val Ok(Some(x @ 1..=9)) = r else { return 100 }
    x
}
func main() -> i32 {
    val a = [1, 2, 3, 4, 5]
    val s: &[i32] = &a
    (pairs(s) + unwrap(Ok(Some(4))) + unwrap(Ok(Some(40))) + unwrap(Ok(None)) + unwrap(Err(1))) % 256
}
"#;
    assert_eq!(run("single_pattern.nr", source), (19 + 4 + 300) % 256);
}

#[test]
fn a_closure_sees_nested_bindings_as_its_own() {
    let source = r#"
func main() -> i32 {
    val n = 2
    val f = |p: Option<Option<i32>>| match p {
        Some(Some(n @ 1..=9)) => n * 10,
        Some(None) | None => n,
        _ => 0
    }
    val none: Option<Option<i32>> = None
    f(Some(Some(3))) + f(none)
}
"#;
    assert_eq!(run("closure.nr", source), 32);
}

#[test]
fn a_nested_function_value_is_called_from_its_binding() {
    let source = r#"
func add1(n: i32) -> i32 { n + 1 }
func run(o: Option<Option<(i32) -> i32>>) -> i32 {
    match o {
        Some(Some(f)) => f(1),
        _ => 0
    }
}
func main() -> i32 {
    val none: Option<Option<(i32) -> i32>> = None
    run(Some(Some(add1))) + run(none)
}
"#;
    assert_eq!(run("function_value.nr", source), 2);
}

const PROBE: &str = r#"
struct Probe { id: i32 }
impl Drop for Probe {
    func drop(&mut self) { println("drop {self.id}") }
}
enum Slot { One(Probe), Two(Probe), Empty }
enum Holder { Hold(Option<Probe>), Bare }
"#;

fn stdout(name: &str, source: &str) -> String {
    let test = CompileTest::new();
    let exe = test
        .compile(&test.write_source(name, source))
        .expect("compile failed");
    let run = Command::new(exe).output().expect("run failed");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert!(run.status.success(), "stdout: {stdout}");
    stdout
}

#[test]
fn an_owner_bound_by_an_alternative_drops_once() {
    let source = format!(
        r#"{PROBE}
func take(s: Slot) -> i32 {{
    match s {{
        Slot::One(p) | Slot::Two(p) => p.id,
        Slot::Empty => 0
    }}
}}
func main() -> i32 {{
    println("{{take(Slot::One(Probe {{ id: 1 }}))}}")
    println("{{take(Slot::Two(Probe {{ id: 2 }}))}}")
    0
}}
"#
    );
    assert_eq!(stdout("drop_alt.nr", &source), "drop 1\n1\ndrop 2\n2\n");
}

#[test]
fn a_failed_guard_leaves_the_owner_to_the_arm_that_runs() {
    let source = format!(
        r#"{PROBE}
func pick(s: Slot, limit: i32) -> i32 {{
    match s {{
        Slot::One(p) | Slot::Two(p) if p.id > limit => p.id * 10,
        Slot::One(p) => p.id,
        Slot::Two(_) => -2,
        Slot::Empty => -3
    }}
}}
func main() -> i32 {{
    println("{{pick(Slot::One(Probe {{ id: 9 }}), 5)}}")
    println("{{pick(Slot::One(Probe {{ id: 3 }}), 5)}}")
    println("{{pick(Slot::Two(Probe {{ id: 4 }}), 5)}}")
    0
}}
"#
    );
    assert_eq!(
        stdout("drop_guard.nr", &source),
        "drop 9\n90\ndrop 3\n3\ndrop 4\n-2\n"
    );
}

#[test]
fn a_nested_owner_drops_once_and_a_test_alone_takes_nothing() {
    let source = format!(
        r#"{PROBE}
func deep(h: Holder) -> i32 {{
    match h {{
        Holder::Hold(Some(p)) => p.id,
        Holder::Hold(None) => -1,
        Holder::Bare => -2
    }}
}}
func tested(h: Holder) -> i32 {{
    match h {{
        Holder::Hold(Some(_)) => 1,
        Holder::Hold(None) | Holder::Bare => 0
    }}
}}
func main() -> i32 {{
    println("{{deep(Holder::Hold(Some(Probe {{ id: 7 }})))}}")
    println("{{tested(Holder::Hold(Some(Probe {{ id: 8 }})))}}")
    0
}}
"#
    );
    assert_eq!(stdout("drop_nested.nr", &source), "drop 7\n7\ndrop 8\n1\n");
}

#[test]
fn array_elements_bound_by_value_drop_once() {
    let source = format!(
        r#"{PROBE}
func split(a: [Probe; 3]) -> i32 {{
    match a {{
        [x, ..rest] => x.id * 10 + rest[1].id
    }}
}}
func main() -> i32 {{
    println("{{split([Probe {{ id: 1 }}, Probe {{ id: 2 }}, Probe {{ id: 3 }}])}}")
    0
}}
"#
    );
    let out = stdout("drop_array.nr", &source);
    let mut drops: Vec<&str> = out.lines().filter(|l| l.starts_with("drop")).collect();
    drops.sort_unstable();
    assert_eq!(drops, vec!["drop 1", "drop 2", "drop 3"], "{out}");
    assert!(out.ends_with("13\n"), "{out}");
}

#[test]
fn matching_moves_a_scrutinee_only_when_it_binds_an_owner() {
    let test = CompileTest::new();
    let moved = r#"
func main() -> i32 {
    val v: Vec<i32> = Vec::new()
    val o = Some(Some(v))
    val r = match o { Some(Some(inner)) => inner.len() as i32, _ => 0 }
    val again = o
    r
}
"#;
    let err = test
        .check("moved_nested.nr", moved)
        .expect_err("`o` gave its payload up");
    assert!(err.contains("use of moved value 'o'"), "{err}");

    let copied = r#"
func main() -> i32 {
    val a = [1, 2]
    val r = match a { [x, _] => x }
    a[1] + r
}
"#;
    assert_eq!(run("copied_array.nr", copied), 3);
}

#[test]
fn diagnostics_name_what_a_pattern_leaves_out() {
    let test = CompileTest::new();
    let cases = [
        (
            "func f(o: Option<Option<i32>>) -> i32 { match o { Some(Some(x)) => x, None => 0 } }",
            "no arm matches `Option::Some(Option::None)`",
        ),
        (
            "func f(s: &[i32]) -> i32 { match s { [] => 0, [a] => a } }",
            "no arm matches `[_, _, ..]`",
        ),
        (
            "func f(o: Option<i32>) -> i32 { match o { Some(x) | None => 1 } }",
            "is not bound in every alternative",
        ),
        (
            "func f(o: Option<Vec<i32>>) -> i32 { match o { w @ Some(v) => 1, None => 0 } }",
            "cannot bind a part of it",
        ),
        (
            "func f(a: [i32; 2]) -> i32 { match a { [x] => x, _ => 0 } }",
            "array pattern binds 1 element(s) but the array has 2",
        ),
        (
            "func f(o: Option<string>) -> i32 { match o { Some(\"x\") => 1, _ => 0 } }",
            "a pattern cannot test a string value",
        ),
        (
            "func f(p: (i32, i32)) -> i32 { match p { _ => 1 } }",
            "cannot match on a value of type (i32, i32)",
        ),
    ];
    for (i, (body, want)) in cases.iter().enumerate() {
        let source = format!("{body}\nfunc main() -> i32 {{ 0 }}\n");
        let err = test
            .check(&format!("diagnostic_{i}.nr"), &source)
            .expect_err("compile should fail");
        assert!(err.contains(want), "wanted `{want}` in: {err}");
    }
}
