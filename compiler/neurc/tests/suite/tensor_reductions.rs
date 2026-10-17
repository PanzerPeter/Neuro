// Tensor reductions (Phase 2B): `.sum()`, `.mean()`, `.max()` and `.min()`, whole-tensor
// and along one axis, end to end through `neurc check` and `neurc compile`.
//
// Every runtime assertion reads the reduced values back, because a result of the right
// SHAPE would pass even if the traversal gathered the wrong run: a column sum computed
// over rows has the same type as the column sum.
//
// An exit code is one byte, so each expected value is kept below 256.

use crate::compile_harness::CompileTest;

/// Compile and run `source`, returning its exit code.
fn run_program(name: &str, source: &str) -> i32 {
    CompileTest::new()
        .compile_and_run(name, source)
        .unwrap_or_else(|e| panic!("{name} should compile and run: {e}"))
}

/// The diagnostics from a program that must not check.
fn rejection(name: &str, source: &str) -> String {
    CompileTest::new()
        .check(name, source)
        .expect_err(&format!("{name} should be rejected"))
}

#[test]
fn a_whole_tensor_reduction_folds_every_element() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    // 21 + 6 * 10 + 1 * 100 = 181
    return m.sum() + m.max() * 10 + m.min() * 100
}
"#;
    assert_eq!(run_program("tensor_reduce_whole.nr", source), 181);
}

/// The two axes of the same matrix reduce over different runs, which is what a
/// shape-only assertion would miss.
#[test]
fn an_axis_reduction_gathers_that_axis_run() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val rows: Tensor<i32, [2]> = m.sum(axis: 1)
    val cols: Tensor<i32, [3]> = m.sum(axis: 0)
    // rows = (6, 15), cols = (5, 7, 9).
    return rows[0] * 10 + rows[1] + cols[0] + cols[2] * 10
}
"#;
    assert_eq!(run_program("tensor_reduce_axis.nr", source), 170);
}

#[test]
fn a_reduction_of_a_rank_three_tensor_keeps_the_surviving_axes() {
    let source = r#"
func main() -> i32 {
    val t: Tensor<i32, [2, 2, 2]> = [
        [[1, 2], [3, 4]],
        [[5, 6], [7, 8]]
    ]
    val middle: Tensor<i32, [2, 2]> = t.sum(axis: 1)
    // middle = ((1+3, 2+4), (5+7, 6+8)) = ((4, 6), (12, 14)).
    return middle[0, 0] * 10 + middle[0, 1] + middle[1, 0] + middle[1, 1]
}
"#;
    assert_eq!(run_program("tensor_reduce_rank3.nr", source), 72);
}

#[test]
fn a_mean_divides_by_the_reduced_run_length() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<f64, [2, 3]> = [
        [1.0, 2.0, 3.0],
        [7.0, 8.0, 9.0]
    ]
    val all: f64 = m.mean()
    val rows: Tensor<f64, [2]> = m.mean(axis: 1)
    // all = 5.0, rows = (2.0, 8.0).
    return (all as i32) * 10 + (rows[0] as i32) + (rows[1] as i32)
}
"#;
    assert_eq!(run_program("tensor_reduce_mean.nr", source), 60);
}

/// A negative axis counts from the end, and a dimension name selects the same axis its
/// position does.
#[test]
fn an_axis_may_be_named_or_counted_from_the_end() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [height: 2, width: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val by_name: Tensor<i32, [width: 3]> = m.max(axis: height)
    val by_index: Tensor<i32, [height: 2]> = m.max(axis: -1)
    // by_name = (4, 5, 6), by_index = (3, 6).
    return by_name[2] * 10 + by_index[0] * 5 + by_index[1]
}
"#;
    assert_eq!(run_program("tensor_reduce_named_axis.nr", source), 81);
}

/// A reduction reads its receiver: the tensor is still there afterwards, and a borrow is
/// a legal receiver, which is what lets a weight be summarised without moving it.
#[test]
fn a_reduction_leaves_its_receiver_alive() {
    let source = r#"
func spread(t: &Tensor<i32, [4]>) -> i32 {
    t.max() - t.min()
}

func main() -> i32 {
    val v: Tensor<i32, [4]> = [4, 9, 2, 7]
    val range: i32 = spread(&v)
    val total: i32 = v.sum()
    return range * 10 + total
}
"#;
    assert_eq!(run_program("tensor_reduce_borrowed.nr", source), 92);
}

/// Reducing a vector along its only axis yields the rank-0 tensor. Indexing cannot read
/// one back — a rank-0 index has no argument to give — so the value is reached by
/// reducing it again, over the empty product of extents that is its single element.
#[test]
fn reducing_a_vector_yields_a_rank_zero_tensor() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = [4, 9, 2, 7]
    val folded: Tensor<i32, []> = v.sum(axis: 0)
    return folded.sum()
}
"#;
    assert_eq!(run_program("tensor_reduce_rank0.nr", source), 22);
}

/// `.max()` / `.min()` fold with the sorting comparator, where a `NaN` is the worst
/// element in either direction, so the answer is the extreme non-`NaN` wherever the `NaN`
/// sits. Before, a leading `NaN` was carried out while a later one was skipped.
#[test]
fn a_nan_never_wins_a_max_or_a_min() {
    let source = r#"
func main() -> i32 {
    val z = 0.0f32
    val nan = z / z
    val a: Tensor<f32, [4]> = [nan, 1.0, 3.0, 2.0]
    val b: Tensor<f32, [4]> = [1.0, 3.0, nan, 2.0]
    val c: Tensor<f32, [3]> = [2.0, nan, 1.0]
    val d: Tensor<f32, [2]> = [nan, nan]
    val m: Tensor<f32, [2, 2]> = [[nan, 5.0], [4.0, nan]]
    mut total = 0
    total += a.clone().max() as i32
    total += b.max() as i32
    total += a.min() as i32
    total += c.min() as i32
    // Only an all-NaN run has no candidate, and then the answer is NaN.
    if d.max().is_nan() { total += 10 }
    val r = m.max(axis: 0)
    total += (r[0] as i32) * 20 + (r[1] as i32)
    return total
}
"#;
    assert_eq!(run_program("tensor_reduce_nan.nr", source), 103);
}

/// A float run longer than 4096 folds in lanes: lane `l` takes run positions `l`, `l + 4096`,
/// ..., then the lanes add in order. Left to right, every `1.0` after `2^24` would round
/// away; in lanes, each lane's small total survives, so the answer shows which order ran. A
/// run of exactly 4096 still folds left to right.
#[test]
fn a_float_run_longer_than_the_lanes_folds_in_lane_order() {
    let source = r#"
func main() -> i32 {
    mut long = Tensor::<f32, [2, 5000]>::ones()
    long[0, 0] = 16777216.0f32
    long[1, 0] = 16777216.0f32
    mut short = Tensor::<f32, [4096]>::ones()
    short[0] = 16777216.0f32
    if short.sum() != 16777216.0f32 { return 1 }
    if long.sum() != 33561656.0f32 { return 2 }
    val means = long.mean(axis: 1)
    if means[1] != 3355.8046875f32 { return 3 }
    val rows = long.sum(axis: 1)
    if rows[0] != rows[1] { return 4 }
    // 2^24 + 1808, where left to right would leave 2^24.
    return ((rows[0] - 16777216.0f32) / 8.0f32) as i32
}
"#;
    assert_eq!(run_program("tensor_reduce_lanes.nr", source), 226);
}

/// The lane order written out in scalar code: `reference` folds the first `n` elements of a
/// run copied into `run`, `serial` folds them left to right. `same` tells `-0.0` from `0.0`.
const LANE_MODEL: &str = r#"
func next(seed: i64) -> i64 {
    return (seed * 1103515245 + 12345) % 2147483648
}

// Terms near 1e8 beside small ones, so where each addition happens decides the last bits.
func sum_value(seed: i64) -> f32 {
    val pick = (seed / 65536) % 4
    val small = ((seed / 16) % 1000) as f32 * 0.001f32
    if pick == 0 { return 100000000.0f32 + small * 64.0f32 }
    if pick == 1 { return -100000000.0f32 }
    return small + 1.0f32
}

// Negative numbers, a NaN now and then, and zeros of both signs as the largest, so which
// zero a `.max()` keeps shows the order it met them in.
func peak_value(seed: i64, nan: f32) -> f32 {
    val pick = (seed / 65536) % 32
    if pick == 0 { return 0.0f32 }
    if pick == 1 { return -0.0f32 }
    if pick == 2 { return nan }
    return -(((seed / 16) % 1000) as f32) - 1.0f32
}

func same(a: f32, b: f32) -> bool {
    if a.is_nan() { return b.is_nan() }
    return a == b && 1.0f32 / a == 1.0f32 / b
}

func precedes(a: f32, b: f32, descending: bool) -> bool {
    if a.is_nan() { return false }
    if b.is_nan() { return true }
    if descending { return a > b }
    return a < b
}

// One fold step: op 0 adds, 1 keeps the larger, 2 the smaller.
func fold(op: i32, acc: f32, value: f32) -> f32 {
    if op == 0 { return acc + value }
    if precedes(value, acc, op == 1) { return value }
    return acc
}

func serial(run: &Tensor<f32, [RUN]>, n: i64, op: i32) -> f32 {
    mut acc = run[0]
    if op == 0 { acc = -0.0f32 }
    mut i: i64 = 0
    while i < n {
        acc = fold(op, acc, run[i])
        i += 1
    }
    return acc
}

func reference(run: &Tensor<f32, [RUN]>, n: i64, op: i32) -> f32 {
    if n <= 4096 { return serial(run, n, op) }
    mut lanes = Tensor::<f32, [4096]>::zeros()
    mut l: i64 = 0
    while l < 4096 {
        mut p = run[l]
        mut k: i64 = l + 4096
        while k < n {
            p = fold(op, p, run[k])
            k += 4096
        }
        lanes[l] = p
        l += 1
    }
    val folded = lanes_run(&lanes)
    return serial(&folded, 4096, op)
}

func lanes_run(lanes: &Tensor<f32, [4096]>) -> Tensor<f32, [RUN]> {
    mut out = Tensor::<f32, [RUN]>::zeros()
    mut l = 0
    while l < 4096 {
        out[l] = lanes[l]
        l += 1
    }
    return out
}
"#;

/// `.sum()`, `.mean()`, `.max()` and `.min()` over runs below, at and above `REDUCE_LANES`
/// (a multiple of it and not, and runs of several lane rows past the second), whole-tensor
/// (rank 1 to 3), along the last axis, an outer one and a middle one, each matching the lane order bit for bit. Each sum of two lane rows
/// or more also differs from a left-to-right fold, so a fold in another order would show.
#[test]
fn every_reduction_keeps_the_lane_order_bits() {
    let cases: Vec<(Vec<usize>, Option<usize>)> = [4095, 4096, 4097, 8192, 9001, 12288, 20000]
        .into_iter()
        .flat_map(|n| {
            [
                (vec![n], None),
                (vec![2, n], Some(1)),
                (vec![n, 3], Some(0)),
            ]
        })
        .chain([
            (vec![3, 3001], None),
            (vec![2, 4096], None),
            (vec![2, 5000, 3], Some(1)),
            (vec![3, 4, 4100], None),
        ])
        .collect();
    let longest = cases
        .iter()
        .map(|(shape, axis)| axis.map_or(shape.iter().product(), |axis| shape[axis]))
        .max()
        .unwrap_or(0);
    let mut body = String::new();
    for (case, (shape, axis)) in cases.iter().enumerate() {
        let dims = shape
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let all = |name: &str| {
            (0..shape.len())
                .map(|d| format!("{name}{d}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let loops = |name: &str, skip: Option<usize>| {
            (0..shape.len())
                .filter(|&d| Some(d) != skip)
                .map(|d| format!("for {name}{d} in 0..{} {{ ", shape[d]))
                .collect::<String>()
        };
        let close = |skip: Option<usize>| {
            "} ".repeat((0..shape.len()).filter(|&d| Some(d) != skip).count())
        };
        body.push_str(&format!(
            "    mut a{case} = Tensor::<f32, [{dims}]>::zeros()\n    mut b{case} = Tensor::<f32, [{dims}]>::zeros()\n    mut c{case} = Tensor::<f32, [{dims}]>::zeros()\n    {}seed = next(seed)\n a{case}[{i}] = sum_value(seed)\n seed = next(seed)\n val w = peak_value(seed, nan)\n b{case}[{i}] = w\n c{case}[{i}] = -w\n {}\n",
            loops("i", None),
            close(None),
            i = all("i"),
        ));
        let (call, kept, n) = match axis {
            None => (
                String::new(),
                String::new(),
                shape.iter().product::<usize>(),
            ),
            Some(axis) => (
                format!("axis: {axis}"),
                (0..shape.len())
                    .filter(|d| d != axis)
                    .map(|d| format!("j{d}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                shape[*axis],
            ),
        };
        let at = |value: &str| match axis {
            None => value.to_string(),
            Some(_) => format!("{value}[{kept}]"),
        };
        body.push_str(&format!(
            "    val s{case} = a{case}.sum({call})\n    val m{case} = a{case}.mean({call})\n    val x{case} = b{case}.max({call})\n    val y{case} = c{case}.min({call})\n"
        ));
        // Copy each run into `run` in run order, then check its four results.
        let (outer, copy, end) = match axis {
            None => (
                String::new(),
                format!(
                    "mut t{case} = 0\n {}ra[t{case}] = a{case}[{i}]\n rb[t{case}] = b{case}[{i}]\n rc[t{case}] = c{case}[{i}]\n t{case} += 1\n {}\n",
                    loops("i", None),
                    close(None),
                    i = all("i"),
                ),
                String::new(),
            ),
            Some(axis) => {
                let index = (0..shape.len())
                    .map(|d| {
                        if d == *axis {
                            "t".to_string()
                        } else {
                            format!("j{d}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    loops("j", Some(*axis)),
                    format!(
                        "for t in 0..{n} {{ ra[t] = a{case}[{index}]\n rb[t] = b{case}[{index}]\n rc[t] = c{case}[{index}] }}\n"
                    ),
                    close(Some(*axis)),
                )
            }
        };
        let code = case * 5;
        let sensitive = match n >= 2 * 4096 {
            true => format!(
                "if same(reference(&ra, {n}, 0), serial(&ra, {n}, 0)) {{ return {} }}\n",
                code + 5
            ),
            false => String::new(),
        };
        body.push_str(&format!(
            "    {outer}{copy} if !same({s}, reference(&ra, {n}, 0)) {{ return {} }}\n if !same({m}, reference(&ra, {n}, 0) / {n}.0f32) {{ return {} }}\n if !same({x}, reference(&rb, {n}, 1)) {{ return {} }}\n if !same({y}, reference(&rc, {n}, 2)) {{ return {} }}\n {sensitive}{end}\n",
            code + 1,
            code + 2,
            code + 3,
            code + 4,
            s = at(&format!("s{case}")),
            m = at(&format!("m{case}")),
            x = at(&format!("x{case}")),
            y = at(&format!("y{case}")),
        ));
    }
    let source = format!(
        "{}\nfunc main() -> i32 {{\n    val z = 0.0f32\n    val nan = z / z\n    mut seed: i64 = 11\n    mut ra = Tensor::<f32, [RUN]>::zeros()\n    mut rb = Tensor::<f32, [RUN]>::zeros()\n    mut rc = Tensor::<f32, [RUN]>::zeros()\n{body}    return 0\n}}\n",
        LANE_MODEL
    )
    .replace("RUN", &longest.to_string());
    assert_eq!(run_program("tensor_reduce_lane_bits.nr", &source), 0);
}

#[test]
fn a_non_numeric_element_is_rejected() {
    let source = r#"
func main() -> i32 {
    val b: Tensor<bool, [2]> = [true, false]
    val x: bool = b.max()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bool.nr", source);
    assert!(
        errors.contains("integer or float element type"),
        "a bool tensor should be rejected: {errors}"
    );
}

#[test]
fn an_integer_mean_is_rejected() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [2]> = [1, 2]
    val x: i32 = v.mean()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_int_mean.nr", source);
    assert!(
        errors.contains("no rounding rule"),
        "an integer mean should be rejected: {errors}"
    );
}

#[test]
fn a_reduction_over_no_elements_is_rejected() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [0]> = []
    val x: i32 = v.min()
    return 0
}
"#;
    let errors = rejection("tensor_reduce_empty.nr", source);
    assert!(
        errors.contains("reduces over no elements"),
        "an empty reduction should be rejected: {errors}"
    );
}

#[test]
fn an_axis_outside_the_rank_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 2]> = [[1, 2], [3, 4]]
    val x: Tensor<i32, [2]> = m.sum(axis: 3)
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bad_axis.nr", source);
    assert!(
        errors.contains("out of range"),
        "an out-of-range axis should be rejected: {errors}"
    );
}

#[test]
fn an_unknown_dimension_name_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [rows: 2, cols: 2]> = [[1, 2], [3, 4]]
    val x: Tensor<i32, [2]> = m.sum(axis: depth)
    return 0
}
"#;
    let errors = rejection("tensor_reduce_bad_name.nr", source);
    assert!(
        errors.contains("no dimension named 'depth'"),
        "an unknown dimension should be rejected: {errors}"
    );
}
