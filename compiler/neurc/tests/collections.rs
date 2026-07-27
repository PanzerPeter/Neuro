// End-to-end tests for the standard collections `Vec<T>`, `HashMap<K, V>`, and
// `BTreeMap<K, V>`: construction, the method surface, `Option`-returning readers,
// growth and rehashing under load, ownership (move + scope-exit free), and the
// `OrderedF32` key wrapper that gives an ordered map a total order over floats.
mod common;
use common::CompileTest;

use std::process::Command;

#[test]
fn vec_push_len_and_iteration() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(10)
    v.push(20)
    v.push(30)
    mut total: i32 = 0
    for x in v {
        total = total + x
    }
    total + v.len() as i32
}
"#;
    let exit = test
        .compile_and_run("vec_push.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 63);
}

#[test]
fn vec_index_read_and_write() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v.push(2)
    v[1] = 40
    v[0] + v[1]
}
"#;
    let exit = test
        .compile_and_run("vec_index.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 41);
}

#[test]
fn vec_pop_yields_some_then_none() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(7)
    mut acc: i32 = 0
    match v.pop() {
        Option::Some(x) => { acc = acc + x }
        Option::None => { acc = acc - 1 }
    }
    match v.pop() {
        Option::Some(x) => { acc = acc + x }
        Option::None => { acc = acc + 100 }
    }
    acc + v.len() as i32
}
"#;
    let exit = test
        .compile_and_run("vec_pop.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 107);
}

#[test]
fn vec_get_is_the_checked_reader() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(5)
    mut acc: i32 = 0
    match v.get(0u64) {
        Option::Some(x) => { acc = acc + x }
        Option::None => { acc = acc - 100 }
    }
    match v.get(9u64) {
        Option::Some(x) => { acc = acc - 100 }
        Option::None => { acc = acc + 3 }
    }
    acc
}
"#;
    let exit = test
        .compile_and_run("vec_get.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 8);
}

#[test]
fn vec_grows_past_its_initial_capacity() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    mut i: i32 = 0
    while i < 500 {
        v.push(i)
        i = i + 1
    }
    mut bad: i32 = 0
    mut j: i32 = 0
    while j < 500 {
        if v[j] != j { bad = bad + 1 }
        j = j + 1
    }
    bad + (v.len() as i32 - 500)
}
"#;
    let exit = test
        .compile_and_run("vec_growth.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn vec_clear_empties_without_losing_the_buffer() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v.push(2)
    v.clear()
    v.push(9)
    v.len() as i32 * 10 + v[0]
}
"#;
    let exit = test
        .compile_and_run("vec_clear.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 19);
}

#[test]
fn vec_index_out_of_bounds_panics() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(1)
    v[3]
}
"#;
    let source_path = test.write_source("vec_oob.nr", source);
    let exe = test.compile(&source_path).expect("compilation failed");
    let output = Command::new(&exe).output().expect("failed to run program");
    assert!(
        !output.status.success(),
        "an out-of-bounds Vec index must abort"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Vec index out of bounds"),
        "expected the bounds panic diagnostic, got: {stderr}"
    );
}

#[test]
fn hashmap_insert_get_and_overwrite() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut m: HashMap<string, i32> = HashMap::new()
    m.insert("alpha", 1)
    m.insert("beta", 2)
    m.insert("beta", 20)
    mut acc: i32 = 0
    match m.get("alpha") {
        Option::Some(v) => { acc = acc + v }
        Option::None => { acc = acc - 100 }
    }
    match m.get("beta") {
        Option::Some(v) => { acc = acc + v }
        Option::None => { acc = acc - 100 }
    }
    match m.get("missing") {
        Option::Some(v) => { acc = acc - 100 }
        Option::None => { acc = acc + 5 }
    }
    acc + m.len() as i32
}
"#;
    let exit = test
        .compile_and_run("hashmap_basic.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 28);
}

#[test]
fn hashmap_contains_and_remove() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    m.insert(1, 10)
    m.insert(2, 20)
    mut acc: i32 = 0
    if m.contains_key(1) { acc = acc + 1 }
    if m.contains_key(9) { acc = acc + 100 }
    if m.remove(1) { acc = acc + 2 }
    if m.remove(1) { acc = acc + 100 }
    if m.contains_key(1) { acc = acc + 100 }
    acc + m.len() as i32
}
"#;
    let exit = test
        .compile_and_run("hashmap_remove.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 4);
}

#[test]
fn hashmap_survives_growth_and_tombstone_churn() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut m: HashMap<i32, i32> = HashMap::new()
    mut i: i32 = 0
    while i < 200 {
        m.insert(i, i * 3)
        i = i + 1
    }
    mut j: i32 = 0
    while j < 200 {
        if j % 2 == 0 {
            if !m.remove(j) { return 91 }
        }
        j = j + 1
    }
    mut k: i32 = 0
    while k < 200 {
        if k % 2 == 0 { m.insert(k, k * 3) }
        k = k + 1
    }
    if m.len() != 200u64 { return 92 }
    mut bad: i32 = 0
    mut q: i32 = 0
    while q < 200 {
        match m.get(q) {
            Option::Some(v) => { if v != q * 3 { bad = bad + 1 } }
            Option::None => { bad = bad + 1 }
        }
        q = q + 1
    }
    bad
}
"#;
    let exit = test
        .compile_and_run("hashmap_churn.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn btreemap_keys_come_back_in_ascending_order() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: BTreeMap<i32, i32> = BTreeMap::new()
    t.insert(50, 5)
    t.insert(10, 1)
    t.insert(90, 9)
    t.insert(30, 3)
    val ks: Vec<i32> = t.keys()
    mut ordered: i32 = 1
    mut prev: i32 = -1
    for k in ks {
        if k < prev { ordered = 0 }
        prev = k
    }
    ordered * 100 + prev
}
"#;
    let exit = test
        .compile_and_run("btreemap_order.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 190);
}

#[test]
fn btreemap_lookup_removal_and_shifting() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: BTreeMap<i32, i32> = BTreeMap::new()
    mut i: i32 = 0
    while i < 64 {
        t.insert(63 - i, i)
        i = i + 1
    }
    if !t.remove(0) { return 91 }
    if !t.remove(63) { return 92 }
    mut bad: i32 = 0
    mut j: i32 = 1
    while j < 63 {
        match t.get(j) {
            Option::Some(v) => { if v != 63 - j { bad = bad + 1 } }
            Option::None => { bad = bad + 1 }
        }
        j = j + 1
    }
    bad + (t.len() as i32 - 62)
}
"#;
    let exit = test
        .compile_and_run("btreemap_remove.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 0);
}

#[test]
fn btreemap_accepts_ordered_float_wrapper_keys() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut scores: BTreeMap<OrderedF32, i32> = BTreeMap::new()
    scores.insert(OrderedF32::new(0.75f32), 3)
    scores.insert(OrderedF32::new(0.10f32), 1)
    scores.insert(OrderedF32::new(0.50f32), 2)
    mut acc: i32 = 0
    match scores.get(OrderedF32::new(0.50f32)) {
        Option::Some(v) => { acc = acc + v }
        Option::None => { acc = acc - 100 }
    }
    val ks: Vec<OrderedF32> = scores.keys()
    mut ordered: i32 = 1
    mut prev: f32 = -1.0f32
    for k in ks {
        if k.value < prev { ordered = 0 }
        prev = k.value
    }
    acc * ordered + scores.len() as i32
}
"#;
    let exit = test
        .compile_and_run("btreemap_ordered_f32.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 5);
}

#[test]
fn hashmap_accepts_struct_keys_with_the_required_impls() {
    let test = CompileTest::new();
    let source = r#"
@derive(Copy, Clone)
struct Point {
    x: i32,
    y: i32
}

impl PartialEq for Point {
    func eq(&self, rhs: &Point) -> bool {
        self.x == rhs.x && self.y == rhs.y
    }
    func ne(&self, rhs: &Point) -> bool {
        self.x != rhs.x || self.y != rhs.y
    }
}

impl Hashable for Point {
    func hash(&self) -> u64 {
        (self.x * 31 + self.y) as u64
    }
}

func main() -> i32 {
    mut grid: HashMap<Point, i32> = HashMap::new()
    val here = Point { x: 1, y: 2 }
    val there = Point { x: 3, y: 4 }
    val nowhere = Point { x: 9, y: 9 }
    grid.insert(here, 12)
    grid.insert(there, 34)
    mut acc: i32 = 0
    match grid.get(there) {
        Option::Some(v) => { acc = acc + v }
        Option::None => { acc = acc - 100 }
    }
    match grid.get(nowhere) {
        Option::Some(v) => { acc = acc - 100 }
        Option::None => { acc = acc + 1 }
    }
    acc
}
"#;
    let exit = test
        .compile_and_run("hashmap_struct_key.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 35);
}

#[test]
fn collection_moves_on_assignment() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut a: Vec<i32> = Vec::new()
    a.push(1)
    val b: Vec<i32> = a
    a.len() as i32
}
"#;
    let source_path = test.write_source("vec_move.nr", source);
    let error = test
        .compile(&source_path)
        .expect_err("using a moved collection must be rejected");
    assert!(
        error.contains("use of moved value 'a'"),
        "expected a move diagnostic, got: {error}"
    );
}

#[test]
fn float_map_key_is_rejected_with_the_wrapper_hint() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut m: BTreeMap<f32, i32> = BTreeMap::new()
    0
}
"#;
    let source_path = test.write_source("float_key.nr", source);
    let error = test
        .compile(&source_path)
        .expect_err("a raw float key must be rejected");
    assert!(
        error.contains("OrderedF32"),
        "expected the wrapper hint, got: {error}"
    );
}

#[test]
fn collection_new_requires_an_annotated_target() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v = Vec::new()
    0
}
"#;
    let source_path = test.write_source("vec_infer.nr", source);
    let error = test
        .compile(&source_path)
        .expect_err("an unannotated Vec::new() must be rejected");
    assert!(
        error.contains("cannot infer the element type"),
        "expected an inference diagnostic, got: {error}"
    );
}

#[test]
fn vec_of_strings_holds_its_elements() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut names: Vec<string> = Vec::new()
    names.push("alpha")
    names.push("beta")
    mut total: i32 = 0
    for n in names {
        total = total + n.len() as i32
    }
    total
}
"#;
    let exit = test
        .compile_and_run("vec_strings.nr", source)
        .expect("compile/run failed");
    assert_eq!(exit, 9);
}
