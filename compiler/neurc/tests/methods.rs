// Method and impl block tests (Phase 2)
// Covers AC1–AC6: &self methods, associated functions, call syntax, error cases.
mod common;
use common::CompileTest;

// ── AC1/AC2: &self method — compiles and returns correct value ───────────────

#[test]
fn self_method_returns_field_value() {
    let test = CompileTest::new();
    let source = r#"
struct Counter {
    value: i32
}

impl Counter {
    func get(&self) -> i32 {
        self.value
    }
}

func main() -> i32 {
    val c = Counter { value: 42 }
    return c.get()
}
"#;
    let exit_code = test
        .compile_and_run("method_get.nr", source)
        .expect("&self method should compile and run");
    assert_eq!(exit_code, 42, "get() should return 42");
}

#[test]
fn self_method_with_arithmetic() {
    let test = CompileTest::new();
    let source = r#"
struct Rect {
    width: i32,
    height: i32
}

impl Rect {
    func area(&self) -> i32 {
        self.width * self.height
    }
}

func main() -> i32 {
    val r = Rect { width: 6, height: 7 }
    return r.area()
}
"#;
    let exit_code = test
        .compile_and_run("method_area.nr", source)
        .expect("area() method should compile and run");
    assert_eq!(exit_code, 42, "6 * 7 should equal 42");
}

// ── AC2: method with explicit non-self parameters ────────────────────────────

#[test]
fn self_method_with_extra_param() {
    let test = CompileTest::new();
    let source = r#"
struct Adder {
    base: i32
}

impl Adder {
    func add(&self, n: i32) -> i32 {
        self.base + n
    }
}

func main() -> i32 {
    val a = Adder { base: 10 }
    return a.add(32)
}
"#;
    let exit_code = test
        .compile_and_run("method_extra_param.nr", source)
        .expect("method with extra param should compile and run");
    assert_eq!(exit_code, 42, "10 + 32 should equal 42");
}

// ── AC3: associated function (no self) called via TypeName::func ─────────────

#[test]
fn associated_function_compiles_and_runs() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

impl Point {
    func new(x: i32, y: i32) -> Point {
        Point { x: x, y: y }
    }
}

func main() -> i32 {
    val p = Point::new(10, 32)
    return p.x + p.y
}
"#;
    let exit_code = test
        .compile_and_run("assoc_func.nr", source)
        .expect("associated function should compile and run");
    assert_eq!(exit_code, 42, "10 + 32 should equal 42");
}

#[test]
fn associated_function_combined_with_method() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

impl Point {
    func new(x: i32, y: i32) -> Point {
        Point { x: x, y: y }
    }

    func sum(&self) -> i32 {
        self.x + self.y
    }
}

func main() -> i32 {
    val p = Point::new(20, 22)
    return p.sum()
}
"#;
    let exit_code = test
        .compile_and_run("assoc_and_method.nr", source)
        .expect("combined assoc func + method should compile and run");
    assert_eq!(exit_code, 42, "20 + 22 should equal 42");
}

// ── AC4: &mut self mutates in place; consuming self is still rejected ─────────

#[test]
fn mut_self_method_mutates_in_place() {
    let test = CompileTest::new();
    let source = r#"
struct Counter {
    value: i32
}

impl Counter {
    func increment(&mut self) {
        self.value = self.value + 1
    }

    func get(&self) -> i32 {
        self.value
    }
}

func main() -> i32 {
    mut c = Counter { value: 40 }
    c.increment()
    c.increment()
    return c.get()
}
"#;
    let exit_code = test
        .compile_and_run("mut_self.nr", source)
        .expect("&mut self method should compile and run");
    assert_eq!(exit_code, 42, "two increments of 40 should yield 42");
}

#[test]
fn mut_self_on_immutable_binding_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Counter {
    value: i32
}

impl Counter {
    func increment(&mut self) {
        self.value = self.value + 1
    }
}

func main() -> i32 {
    val c = Counter { value: 0 }
    c.increment()
    return 0
}
"#;
    let source_path = test.write_source("mut_self_immutable.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "calling &mut self on a val binding must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("mutably borrow") || err.contains("CannotBorrowMutably"),
        "error should mention the receiver is not mutable, got: {}",
        err
    );
}

#[test]
fn mut_self_while_borrowed_is_rejected() {
    let test = CompileTest::new();
    // §2.5: calling a `&mut self` method takes an exclusive borrow, so it conflicts
    // with a live shared borrow of the same receiver.
    let source = r#"
struct Counter {
    value: i32
}

impl Counter {
    func increment(&mut self) {
        self.value = self.value + 1
    }
}

func main() -> i32 {
    mut c = Counter { value: 0 }
    val r = &c
    c.increment()
    return r.value
}
"#;
    let source_path = test.write_source("mut_self_borrowed.nr", source);
    let result = test.compile(&source_path);
    assert!(
        result.is_err(),
        "calling &mut self while shared-borrowed must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("already borrowed") || err.contains("borrow 'c' as mutable"),
        "error should mention the exclusivity conflict, got: {}",
        err
    );
}

#[test]
fn consuming_self_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Wrapper {
    value: i32
}

impl Wrapper {
    func unwrap(self) -> i32 {
        self.value
    }
}

func main() -> i32 {
    val w = Wrapper { value: 0 }
    return w.unwrap()
}
"#;
    let source_path = test.write_source("consuming_self.nr", source);
    let result = test.compile(&source_path);
    assert!(result.is_err(), "consuming self should be rejected");
    let err = result.unwrap_err();
    assert!(
        err.contains("not yet supported") || err.contains("UnsupportedSelfParam"),
        "error should mention unsupported self param, got: {}",
        err
    );
}

// ── AC5: calling a non-existent method produces a clear error ────────────────

#[test]
fn method_not_found_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point {
    x: i32,
    y: i32
}

impl Point {
    func sum(&self) -> i32 {
        self.x + self.y
    }
}

func main() -> i32 {
    val p = Point { x: 1, y: 2 }
    return p.nonexistent()
}
"#;
    let source_path = test.write_source("method_not_found.nr", source);
    let result = test.compile(&source_path);
    assert!(result.is_err(), "calling unknown method should fail");
    let err = result.unwrap_err();
    assert!(
        err.contains("nonexistent") || err.contains("MethodNotFound") || err.contains("method"),
        "error should mention the missing method, got: {}",
        err
    );
}

// ── AC6: existing struct tests are not regressed ─────────────────────────────
// (covered by the existing structs.rs test file running concurrently)

// ── impl block defined before its struct ─────────────────────────────────────

#[test]
fn impl_defined_after_struct() {
    let test = CompileTest::new();
    // Struct pre-registration ensures impl can reference it regardless of order.
    let source = r#"
impl Score {
    func doubled(&self) -> i32 {
        self.value * 2
    }
}

struct Score {
    value: i32
}

func main() -> i32 {
    val s = Score { value: 21 }
    return s.doubled()
}
"#;
    let exit_code = test
        .compile_and_run("impl_before_struct.nr", source)
        .expect("impl before struct should compile and run");
    assert_eq!(exit_code, 42, "21 * 2 should equal 42");
}
