// Reading through a borrow: every built-in operator takes a borrowed operand where it
// takes an owned one, and a method call reads its receiver through any number of `&`
// and `&mut`. Everywhere else a borrow stays a value of its own type.
use crate::compile_harness::CompileTest;

fn run(source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run("test.nr", source)
        .expect("program should compile and run")
}

fn rejection(source: &str) -> String {
    match CompileTest::new().check("test.nr", source) {
        Ok(()) => panic!("program should be rejected"),
        Err(diagnostics) => diagnostics,
    }
}

#[test]
fn arithmetic_and_comparison_read_a_borrowed_scalar() {
    let source = r#"
func score(x: &i32, y: &i32) -> i32 {
    if x > 0 && y <= x && x != y {
        return x * 10 + y - 1 % x
    }
    return 0
}

func main() -> i32 {
    val a: i32 = 4
    val b: i32 = 2
    return score(&a, &b)
}
"#;
    assert_eq!(run(source), 41);
}

#[test]
fn unary_logical_and_bitwise_operators_read_a_borrowed_scalar() {
    let source = r#"
func mix(x: &u8, flag: &bool, n: &i32) -> i32 {
    val bits = (x & 6 | x << 1 ^ ~x) as i32
    if !flag { return bits - n }
    return 0
}

func main() -> i32 {
    val x: u8 = 3
    val flag = false
    val n: i32 = 1
    return mix(&x, &flag, &n)
}
"#;
    // 3 & 6 | ((3 << 1) ^ ~3) = 2 | (6 ^ 252) = 250, minus 1.
    assert_eq!(run(source), 249);
}

#[test]
fn a_literal_beside_a_borrowed_operand_takes_the_referent_type() {
    // `2 * x` is `i64` arithmetic: an `i32` literal would overflow the debug guard.
    let source = r#"
func double(x: &i64) -> i64 {
    return 2 * x
}

func main() -> i32 {
    val big: i64 = 3000000000
    return (double(&big) / 1000000000) as i32
}
"#;
    assert_eq!(run(source), 6);
}

#[test]
fn compound_assignment_reads_a_borrowed_right_operand() {
    let source = r#"
func main() -> i32 {
    mut total = 5
    val step = 3
    val r = &step
    total += r
    total *= r
    return total + r
}
"#;
    assert_eq!(run(source), 27);
}

#[test]
fn a_mutable_borrow_stays_usable_after_an_operator_reads_it() {
    let source = r#"
func triple(x: &mut i32) -> i32 {
    val sum = x + x
    *x = sum + x
    return x * 1
}

func main() -> i32 {
    mut a = 5
    val result = triple(&mut a)
    return result + a
}
"#;
    assert_eq!(run(source), 30);
}

#[test]
fn nested_borrows_are_read_through_by_an_operator() {
    let source = r#"
func next(x: & &i32) -> i32 {
    return x + 1
}

func main() -> i32 {
    val a = 5
    val r = &a
    return next(&r)
}
"#;
    assert_eq!(run(source), 6);
}

#[test]
fn a_scalar_method_dereferences_its_receiver() {
    let source = r#"
func clean(x: &f64, y: &mut u8) -> i32 {
    val fits = x.abs().to_checked::<u8>() ?? 0
    return fits as i32 + y.wrapping_add(250) as i32
}

func main() -> i32 {
    val x = -3.0
    mut y: u8 = 10
    return clean(&x, &mut y)
}
"#;
    // 3 + (10 + 250 wrapped to u8 = 4).
    assert_eq!(run(source), 7);
}

#[test]
fn a_struct_method_dereferences_a_nested_receiver() {
    let source = r#"
struct Counter { n: i32 }

impl Counter {
    func get(&self) -> i32 { return self.n }
    func bump(&mut self) { self.n = self.n + 1 }
}

func read(c: & &Counter) -> i32 {
    return c.get()
}

func bump_twice(c: &mut &mut Counter) {
    c.bump()
    (*c).bump()
}

func main() -> i32 {
    mut c = Counter { n: 4 }
    {
        mut inner = &mut c
        bump_twice(&mut inner)
    }
    val r = &c
    return read(&r)
}
"#;
    assert_eq!(run(source), 6);
}

#[test]
fn a_field_is_reached_through_an_explicitly_dereferenced_nested_borrow() {
    // `(*c).n` once read the borrow's own slot as the struct and wrote into it.
    let source = r#"
struct Counter { n: i32 }

func set(c: &mut &mut Counter) {
    (*c).n = 9
}

func get(c: & &Counter) -> i32 {
    return (*c).n
}

func main() -> i32 {
    mut c = Counter { n: 4 }
    {
        mut inner = &mut c
        set(&mut inner)
    }
    val r = &c
    return get(&r)
}
"#;
    assert_eq!(run(source), 9);
}

#[test]
fn binding_a_borrow_keeps_it_a_borrow() {
    let diagnostics = rejection(
        r#"
func main() -> i32 {
    val a = 5
    val r = &a
    val y: i32 = r
    return y
}
"#,
    );
    assert!(
        diagnostics.contains("expected i32, found &i32"),
        "{diagnostics}"
    );
}

#[test]
fn a_cast_does_not_read_through_a_borrow() {
    let diagnostics = rejection(
        r#"
func main() -> i32 {
    val a = 5
    val r = &a
    return r as i32
}
"#,
    );
    assert!(diagnostics.contains("found &i32"), "{diagnostics}");
}

#[test]
fn a_shared_borrow_of_a_mutable_borrow_cannot_call_a_mut_method() {
    let diagnostics = rejection(
        r#"
struct Counter { n: i32 }

impl Counter {
    func bump(&mut self) { self.n = self.n + 1 }
}

func bump(c: & &mut Counter) {
    c.bump()
}

func main() -> i32 {
    return 0
}
"#,
    );
    assert!(
        diagnostics.contains("cannot mutably borrow"),
        "{diagnostics}"
    );
}

#[test]
fn a_borrowed_aggregate_still_has_no_built_in_operator() {
    let diagnostics = rejection(
        r#"
func main() -> i32 {
    val xs = [1, 2]
    val r = &xs
    val total = r + 1
    return 0
}
"#,
    );
    assert!(
        diagnostics.contains("cannot apply binary operator +"),
        "{diagnostics}"
    );
}
