// Assignment to a place: a field, an element, a tensor coordinate, or a referent,
// with or without a compound operator.
//
// The forms below were one missing parse form between them until the place
// expression landed, so these tests are as much about the shapes reaching the
// checker at all as about what they compute.
use crate::compile_harness::CompileTest;

#[test]
fn test_bug_025_compound_assignment_to_a_field() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32, y: i32 }

impl Point {
    func translate(&mut self, dx: i32, dy: i32) {
        self.x += dx
        self.y += dy
    }
}

func main() -> i32 {
    mut q = Point { x: 3, y: 4 }
    q.translate(1, 2)
    return q.x * 10 + q.y
}
"#;
    let exit_code = test
        .compile_and_run("bug_025_field_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 46);
}

#[test]
fn test_compound_assignment_to_a_field_of_a_binding() {
    let test = CompileTest::new();
    let source = r#"
struct Counter { hits: i32 }

func main() -> i32 {
    mut c = Counter { hits: 10 }
    c.hits += 5
    c.hits *= 2
    return c.hits
}
"#;
    let exit_code = test
        .compile_and_run("field_compound_binding.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 30);
}

#[test]
fn test_assignment_to_a_nested_field() {
    let test = CompileTest::new();
    let source = r#"
struct Inner { v: i32 }
struct Outer { i: Inner }

func main() -> i32 {
    mut o = Outer { i: Inner { v: 1 } }
    o.i.v = 5
    o.i.v += 3
    return o.i.v
}
"#;
    let exit_code = test
        .compile_and_run("nested_field_assign.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 8);
}

#[test]
fn test_compound_assignment_to_an_array_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut arr = [1, 2, 3]
    arr[0] += 5
    arr[2] -= 1
    return arr[0] * 10 + arr[2]
}
"#;
    let exit_code = test
        .compile_and_run("array_element_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 62);
}

#[test]
fn test_compound_assignment_to_an_array_inside_a_nested_struct() {
    let test = CompileTest::new();
    let source = r#"
struct Row { cells: [i32; 3] }
struct Grid { row: Row }

func main() -> i32 {
    mut g = Grid { row: Row { cells: [1, 2, 3] } }
    g.row.cells[2] += 40
    return g.row.cells[2]
}
"#;
    let exit_code = test
        .compile_and_run("nested_array_element_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 43);
}

#[test]
fn test_compound_assignment_to_a_vec_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<i32> = Vec::new()
    v.push(10)
    v.push(20)
    v[0] += 7
    v[1] /= 2
    return v[0] + v[1]
}
"#;
    let exit_code = test
        .compile_and_run("vec_element_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 27);
}

#[test]
fn test_compound_assignment_to_a_slice_element() {
    let test = CompileTest::new();
    let source = r#"
func bump(xs: &mut [i32]) {
    xs[1] += 100
}

func main() -> i32 {
    mut arr = [1, 2, 3]
    bump(&mut arr)
    return arr[1]
}
"#;
    let exit_code = test
        .compile_and_run("slice_element_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 102);
}

#[test]
fn test_compound_assignment_through_a_mutable_reference() {
    let test = CompileTest::new();
    let source = r#"
func bump(r: &mut i32) {
    *r += 5
}

func main() -> i32 {
    mut x = 1
    bump(&mut x)
    return x
}
"#;
    let exit_code = test
        .compile_and_run("deref_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 6);
}

#[test]
fn test_assignment_to_a_tensor_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    t[0, 1] = 9
    return t[0, 1] + t[1, 0]
}
"#;
    let exit_code = test
        .compile_and_run("tensor_element_assign.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 12);
}

#[test]
fn test_compound_assignment_to_a_tensor_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    t[1, 2] += 10
    return t[1, 2]
}
"#;
    let exit_code = test
        .compile_and_run("tensor_element_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 16);
}

#[test]
fn test_in_place_tensor_update_of_a_struct_field() {
    let test = CompileTest::new();
    let source = r#"
struct Net { w: Tensor<i32, [2, 2]> }

func main() -> i32 {
    mut n = Net { w: [[1, 2], [3, 4]] }
    val g: Tensor<i32, [2, 2]> = [[10, 10], [10, 10]]
    n.w += g
    return n.w[1, 1]
}
"#;
    let exit_code = test
        .compile_and_run("tensor_field_compound.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 14);
}

#[test]
fn test_writing_a_tensor_element_in_a_loop() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: Tensor<i32, [3, 3]> = [[0, 0, 0], [0, 0, 0], [0, 0, 0]]
    for i in 0..3 {
        for j in 0..3 {
            t[i, j] = i * 3 + j
        }
    }
    return t[2, 2] + t[0, 1]
}
"#;
    let exit_code = test
        .compile_and_run("tensor_element_loop.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 9);
}

#[test]
fn test_a_non_place_target_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func f() -> i32 { return 1 }

func main() -> i32 {
    f() = 2
    return 0
}
"#;
    let error = test
        .check("non_place_target.nr", source)
        .expect_err("a call result is not a place");
    assert!(error.contains("must be a place"), "got: {error}");
}

#[test]
fn test_a_field_of_an_immutable_binding_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
struct Point { x: i32 }

func main() -> i32 {
    val p = Point { x: 1 }
    p.x += 1
    return p.x
}
"#;
    let error = test
        .check("immutable_field_target.nr", source)
        .expect_err("an immutable binding's field is not assignable");
    assert!(error.contains("immutable"), "got: {error}");
}

#[test]
fn test_an_element_of_an_immutable_array_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    val arr = [1, 2, 3]
    arr[0] += 1
    return arr[0]
}
"#;
    let error = test
        .check("immutable_element_target.nr", source)
        .expect_err("an immutable binding's element is not assignable");
    assert!(error.contains("immutable"), "got: {error}");
}

#[test]
fn test_a_tensor_slice_target_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut t: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    t[0, ..] = 5
    return 0
}
"#;
    let error = test
        .check("tensor_slice_target.nr", source)
        .expect_err("a slice is a fresh tensor, not storage");
    assert!(error.contains("tensor slice"), "got: {error}");
}

#[test]
fn test_assignment_to_a_field_of_an_array_element() {
    let test = CompileTest::new();
    let source = r#"
struct Cell { load: i32 }

func main() -> i32 {
    mut cells = [Cell { load: 1 }, Cell { load: 2 }]
    cells[0].load += 10
    return cells[0].load
}
"#;
    let exit_code = test
        .compile_and_run("field_of_element.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 11);
}

#[test]
fn test_assignment_to_a_nested_array_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut grid = [[1, 2], [3, 4]]
    grid[0][1] = 9
    grid[1][0] += 30
    return grid[0][1] + grid[1][0]
}
"#;
    let exit_code = test
        .compile_and_run("nested_array_element.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 42);
}

#[test]
fn test_assignment_to_a_tensor_element_held_in_a_field() {
    let test = CompileTest::new();
    let source = r#"
struct Board { t: Tensor<i32, [2, 2]> }

func main() -> i32 {
    mut b = Board { t: [[1, 2], [3, 4]] }
    b.t[1, 0] = 30
    b.t[0, 0] += 1
    return b.t[1, 0] + b.t[0, 0]
}
"#;
    let exit_code = test
        .compile_and_run("tensor_element_in_field.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 32);
}

#[test]
fn test_an_element_of_a_temporary_array_is_rejected() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut v: Vec<[i32; 2]> = Vec::new()
    v.push([1, 2])
    v[0][1] = 9
    return v[0][1]
}
"#;
    let error = test
        .compile_and_run("temporary_array_target.nr", source)
        .expect_err("a collection element read out by value has no storage to write into");
    assert!(error.contains("not a temporary"), "got: {error}");
}

#[test]
fn test_bug_036_mutating_a_collection_held_in_a_field() {
    let test = CompileTest::new();
    let source = r#"
struct Registry { open: Vec<i32> }

func main() -> i32 {
    mut registry = Registry { open: Vec::new() }
    registry.open.push(1)
    registry.open.push(2)
    return registry.open.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("bug_036_field_receiver.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 2);
}

#[test]
fn test_bug_036_mutating_a_collection_held_in_a_tuple_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut pair: (Vec<i32>, i32) = (Vec::new(), 0)
    pair.0.push(1)
    pair.0.push(2)
    return pair.0.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("bug_036_tuple_receiver.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 2);
}

#[test]
fn test_bug_036_mutating_a_collection_held_in_an_array_element() {
    let test = CompileTest::new();
    let source = r#"
func main() -> i32 {
    mut vs: [Vec<i32>; 2] = [Vec::new(), Vec::new()]
    vs[0].push(1)
    vs[0].push(2)
    vs[1].push(9)
    return vs[0].len() as i32 * 10 + vs[1].len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("bug_036_element_receiver.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 21);
}

#[test]
fn test_reading_back_what_a_held_collection_was_given() {
    let test = CompileTest::new();
    let source = r#"
struct Inner { v: Vec<i32> }
struct Outer { i: Inner }

func main() -> i32 {
    mut o = Outer { i: Inner { v: Vec::new() } }
    o.i.v.push(7)
    o.i.v.push(8)
    return o.i.v[0] + o.i.v[1] + o.i.v.len() as i32
}
"#;
    let exit_code = test
        .compile_and_run("held_collection_roundtrip.nr", source)
        .expect("compilation failed");
    assert_eq!(exit_code, 17);
}
