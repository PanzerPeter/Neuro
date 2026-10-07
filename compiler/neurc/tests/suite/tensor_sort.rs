// The order-based tensor selections (Phase 2B): `.sort()`, `.argsort()` and `.topk()`,
// end to end through `neurc check` and `neurc compile`.
//
// Every runtime assertion reads the ordered values back rather than only their shape: a
// result of the right shape would pass even if the walk ordered the wrong run, since a
// column sorted as if it were a row has exactly the same type.
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
fn sort_orders_the_last_axis_by_default() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [5]> = [5, 3, 9, 1, 7]
    val s: Tensor<i32, [5]> = v.sort()
    // 1, 3, 5, 7, 9 read back as 1 + 3*2 + 5*4 + 7*8 + 9*16 = 227.
    return s[0] + s[1] * 2 + s[2] * 4 + s[3] * 8 + s[4] * 16
}
"#;
    assert_eq!(run_program("tensor_sort_default.nr", source), 227);
}

#[test]
fn descending_reverses_the_order() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = [2, 8, 4, 6]
    val d: Tensor<i32, [4]> = v.sort(descending: true)
    return d[0] * 10 + d[1] * 2 + d[2] + d[3] / 2
}
"#;
    // 8*10 + 6*2 + 4 + 1 = 97; ascending would read back as 38.
    assert_eq!(run_program("tensor_sort_desc.nr", source), 97);
}

/// The two axes of one matrix order different runs, which a shape-only assertion misses.
#[test]
fn an_axis_argument_picks_the_ordered_run() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    val rows: Tensor<i32, [2, 3]> = m.sort(axis: 1)
    val cols: Tensor<i32, [2, 3]> = m.sort(axis: 0)
    // rows = ((2,5,7),(1,4,9)); cols = ((1,2,4),(7,9,5)).
    return rows[0, 0] * 10 + rows[1, 2] + cols[0, 2] * 4 + cols[1, 1]
}
"#;
    // 2*10 + 9 + 4*4 + 9 = 54.
    assert_eq!(run_program("tensor_sort_axis.nr", source), 54);
}

#[test]
fn a_negative_axis_counts_from_the_end() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    val a: Tensor<i32, [2, 3]> = m.sort(axis: -1)
    val b: Tensor<i32, [2, 3]> = m.sort(axis: 1)
    return a[0, 0] + b[0, 0] + a[1, 2] + b[1, 2]
}
"#;
    // 2 + 2 + 9 + 9 = 22.
    assert_eq!(run_program("tensor_sort_negative_axis.nr", source), 22);
}

#[test]
fn a_dimension_name_selects_its_axis() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [height: 2, width: 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    val down: Tensor<i32, [height: 2, width: 3]> = m.sort(axis: height)
    // Column-wise: ((1,2,4),(7,9,5)).
    return down[0, 0] * 10 + down[0, 2] + down[1, 1]
}
"#;
    // 1*10 + 4 + 9 = 23.
    assert_eq!(run_program("tensor_sort_named_axis.nr", source), 23);
}

#[test]
fn argsort_reports_the_receiver_positions() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [5]> = [5, 3, 9, 1, 7]
    val order: Tensor<i32, [5]> = v.argsort()
    // The ordering is (3, 1, 0, 4, 2).
    return order[0] * 10 + order[1] * 100 + order[2] + order[3] + order[4] * 2
}
"#;
    // 30 + 100 + 0 + 4 + 4 = 138.
    assert_eq!(run_program("tensor_argsort.nr", source), 138);
}

/// An index is only useful if it reads back into the tensor it came from.
#[test]
fn an_argsort_index_indexes_the_receiver() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = [40, 10, 30, 20]
    val order: Tensor<i32, [4]> = v.argsort(descending: true)
    val best = order[0] as u64
    return v[best]
}
"#;
    assert_eq!(run_program("tensor_argsort_index.nr", source), 40);
}

#[test]
fn topk_selects_the_greatest_values_with_their_positions() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [5]> = [5, 3, 9, 1, 7]
    val (values, indices) = v.topk(k: 3)
    // values = (9, 7, 5) at indices (2, 4, 0).
    return values[0] * 10 + values[1] + indices[0] + indices[1] * 2
}
"#;
    // 90 + 7 + 2 + 8 = 107.
    assert_eq!(run_program("tensor_topk.nr", source), 107);
}

#[test]
fn topk_narrows_the_axis_it_selects_along() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    val (values, indices) = m.topk(k: 2, axis: 1)
    // values = ((7,5),(9,4)) at indices ((0,2),(1,2)).
    return values[0, 0] * 10 + values[1, 0] + indices[0, 1] + indices[1, 0]
}
"#;
    // 70 + 9 + 2 + 1 = 82.
    assert_eq!(run_program("tensor_topk_axis.nr", source), 82);
}

/// One rule is pinned for floats: `NaN` sorts to the END whatever the direction, so the
/// best candidates stay at the front of the result where top-k expects them.
#[test]
fn a_nan_sorts_to_the_end_in_both_directions() {
    let source = r#"
func main() -> i32 {
    val nan = 0.0 / 0.0
    val v: Tensor<f64, [4]> = [3.5, nan, 1.5, 2.5]
    val up: Tensor<f64, [4]> = v.sort()
    val down: Tensor<f64, [4]> = v.sort(descending: true)
    mut score: i32 = 0
    if up[3].is_nan() { score = score + 1 }
    if down[3].is_nan() { score = score + 2 }
    if up[0] == 1.5 { score = score + 4 }
    if down[0] == 3.5 { score = score + 8 }
    return score
}
"#;
    assert_eq!(run_program("tensor_sort_nan.nr", source), 15);
}

/// Equal elements keep the order they were written in, which is the only thing that makes
/// an argsort of a tensor with ties reproducible.
#[test]
fn equal_elements_keep_their_relative_order() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [5]> = [2, 1, 2, 1, 2]
    val order: Tensor<i32, [5]> = v.argsort()
    // The two 1s come first in source order (1, 3), then the three 2s (0, 2, 4).
    return order[0] + order[1] * 10 + order[2] * 100 + order[3] + order[4]
}
"#;
    // 1 + 30 + 0 + 2 + 4 = 37.
    assert_eq!(run_program("tensor_sort_stable.nr", source), 37);
}

/// A selection allocates its own result, so it READS the receiver: a borrowed tensor is
/// an acceptable receiver and the owner still has its tensor afterwards.
#[test]
fn a_selection_reads_a_borrowed_receiver() {
    let source = r#"
func smallest(t: &Tensor<i32, [4]>) -> i32 {
    val s: Tensor<i32, [4]> = t.sort()
    return s[0]
}

func main() -> i32 {
    val v: Tensor<i32, [4]> = [40, 10, 30, 20]
    val low = smallest(&v)
    return low + v.max()
}
"#;
    assert_eq!(run_program("tensor_sort_borrowed.nr", source), 50);
}

#[test]
fn a_selection_composes_with_the_shape_casts() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [7, 2, 5],
        [1, 9, 4]
    ]
    // Transposing first makes the old columns the new rows, so the same default axis
    // orders a different run.
    val t: Tensor<i32, [3, 2]> = m.t()
    val s: Tensor<i32, [3, 2]> = t.sort()
    // t = ((7,1),(2,9),(5,4)); sorted rows = ((1,7),(2,9),(4,5)).
    return s[0, 0] * 10 + s[1, 1] + s[2, 0]
}
"#;
    // 10 + 9 + 4 = 23.
    assert_eq!(run_program("tensor_sort_after_transpose.nr", source), 23);
}

#[test]
fn a_non_numeric_element_type_is_rejected() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<bool, [3]> = [true, false, true]
    val s = v.sort()
    return 0
}
"#;
    let errors = rejection("tensor_sort_bool.nr", source);
    assert!(
        errors.contains("orders a tensor's elements"),
        "the diagnostic names the element type rule: {errors}"
    );
}

#[test]
fn a_rank_zero_receiver_is_rejected() {
    let source = r#"
func main() -> i32 {
    val s: Tensor<i32, []> = Tensor::scalar(3)
    val ordered = s.sort()
    return 0
}
"#;
    let errors = rejection("tensor_sort_rank0.nr", source);
    assert!(
        errors.contains("rank-0 tensor has none"),
        "the diagnostic names the missing axis: {errors}"
    );
}

#[test]
fn a_width_wider_than_the_axis_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val (values, indices) = m.topk(k: 9)
    return 0
}
"#;
    let errors = rejection("tensor_topk_too_wide.nr", source);
    assert!(
        errors.contains("selects more elements than the sorted axis holds"),
        "the diagnostic names the axis extent: {errors}"
    );
}

/// `k`, `axis` and `descending` decide the result's shape and the comparator, both of
/// which are settled before any element exists, so none of them may be a runtime value.
#[test]
fn a_runtime_argument_is_rejected() {
    let source = r#"
func main() -> i32 {
    mut width: i32 = 2
    val m: Tensor<i32, [2, 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val (values, indices) = m.topk(k: width)
    return 0
}
"#;
    let errors = rejection("tensor_topk_runtime_k.nr", source);
    assert!(
        errors.contains("has to be a constant"),
        "the diagnostic names the constant requirement: {errors}"
    );
}

#[test]
fn a_runtime_direction_is_rejected() {
    let source = r#"
func main() -> i32 {
    mut flip = true
    val v: Tensor<i32, [3]> = [1, 2, 3]
    val s = v.sort(descending: flip)
    return 0
}
"#;
    let errors = rejection("tensor_sort_runtime_direction.nr", source);
    assert!(
        errors.contains("`descending:`") && errors.contains("has to be a constant"),
        "the diagnostic names the direction argument: {errors}"
    );
}

#[test]
fn an_unknown_dimension_name_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<i32, [height: 2, width: 3]> = [
        [1, 2, 3],
        [4, 5, 6]
    ]
    val s = m.sort(axis: depth)
    return 0
}
"#;
    let errors = rejection("tensor_sort_unknown_axis.nr", source);
    assert!(
        errors.contains("no dimension named 'depth'"),
        "the diagnostic names the missing dimension: {errors}"
    );
}

#[test]
fn a_dynamic_extent_is_rejected() {
    let source = r#"
func main() -> i32 {
    val m: Tensor<f32, [?, 3]> = Tensor::<f32, [2, 3]>::zeros()
    val s = m.sort()
    return 0
}
"#;
    let errors = rejection("tensor_sort_dynamic.nr", source);
    assert!(
        errors.contains("at compile time"),
        "the diagnostic names the dynamic extent: {errors}"
    );
}

#[test]
fn a_width_must_be_named() {
    let source = r#"
func main() -> i32 {
    val v: Tensor<i32, [4]> = [1, 2, 3, 4]
    val (values, indices) = v.topk(2)
    return 0
}
"#;
    let errors = rejection("tensor_topk_positional.nr", source);
    assert!(
        errors.contains("must be named"),
        "the diagnostic asks for the label: {errors}"
    );
}

/// An all-positional call reaches the checker unbound, so a third argument to `.sort` or
/// `.argsort` was ignored rather than counted.
#[test]
fn regression_a_surplus_positional_argument_is_rejected() {
    for method in ["sort", "argsort"] {
        let source = format!(
            "func main() -> i32 {{\n    val v: Tensor<i32, [3]> = [3, 1, 2]\n    val r = v.{method}(0, true, 5)\n    return 0\n}}\n"
        );
        let errors = rejection(&format!("tensor_{method}_surplus.nr"), &source);
        assert!(
            errors.contains("expected 2, found 3"),
            "{method}: the diagnostic counts the arguments: {errors}"
        );
    }
}

/// Runs long enough for the sort's own algorithm to matter: a host integer run of 256 or
/// more sorts by radix, and anything else by merging. Each program checks itself and
/// answers 0, or the number of the first rule it found broken: ascending, descending,
/// each argsort entry naming an element equal to the sorted one there, equal elements in
/// source order, and `.topk` agreeing with the descending sort.
const LONG_RUN_CHECKS: &str = r#"
func next(seed: i64) -> i64 {
    return (seed * 1103515245 + 12345) % 2147483648
}
"#;

#[test]
fn long_integer_runs_sort_on_every_axis() {
    let source = format!(
        "{LONG_RUN_CHECKS}{}",
        r#"
func main() -> i32 {
    mut v: Tensor<i32, [2, 700]> = Tensor::zeros()
    mut seed: i64 = 7
    for r in 0..2 {
        for i in 0..700 {
            seed = next(seed)
            v[r, i] = (((seed / 65536) % 41) as i32 - 20) * 104729
        }
    }
    v[0, 3] = 2147483647
    v[1, 5] = -2147483647
    val up: Tensor<i32, [2, 700]> = v.sort()
    val down: Tensor<i32, [2, 700]> = v.sort(descending: true)
    val order: Tensor<i32, [2, 700]> = v.argsort()
    val back: Tensor<i32, [2, 700]> = v.argsort(descending: true)
    val (top, at) = v.topk(k: 9)
    for r in 0..2 {
        for i in 1..700 {
            if up[r, i - 1] > up[r, i] { return 1 }
            if down[r, i - 1] < down[r, i] { return 2 }
            if up[r, i - 1] == up[r, i] && order[r, i - 1] >= order[r, i] { return 4 }
            if down[r, i - 1] == down[r, i] && back[r, i - 1] >= back[r, i] { return 4 }
        }
        for i in 0..700 {
            if v[r, order[r, i] as u64] != up[r, i] { return 3 }
            if v[r, back[r, i] as u64] != down[r, i] { return 3 }
        }
        for i in 0..9 {
            if top[r, i] != down[r, i] || at[r, i] != back[r, i] { return 5 }
        }
    }
    if up[0, 699] != 2147483647 || up[1, 0] != -2147483647 { return 6 }
    // Down the columns: runs of two, merged.
    val cols: Tensor<i32, [2, 700]> = v.sort(axis: 0)
    for i in 0..700 {
        if cols[0, i] > cols[1, i] { return 7 }
    }
    return 0
}
"#
    );
    assert_eq!(run_program("tensor_sort_long_i32.nr", &source), 0);
}

#[test]
fn a_long_run_along_the_first_axis_sorts_every_width() {
    let source = format!(
        "{LONG_RUN_CHECKS}{}",
        r#"
func main() -> i32 {
    mut wide: Tensor<i64, [300, 3]> = Tensor::zeros()
    mut narrow: Tensor<u8, [300, 2]> = Tensor::zeros()
    mut seed: i64 = 99
    for i in 0..300 {
        for c in 0..3 {
            seed = next(seed)
            wide[i, c] = (seed - 1073741824) * 4294967311
        }
        for c in 0..2 {
            seed = next(seed)
            narrow[i, c] = ((seed / 256) % 256) as u8
        }
    }
    val up: Tensor<i64, [300, 3]> = wide.sort(axis: 0)
    val order: Tensor<i32, [300, 3]> = wide.argsort(axis: 0)
    val down: Tensor<u8, [300, 2]> = narrow.sort(axis: 0, descending: true)
    val back: Tensor<i32, [300, 2]> = narrow.argsort(axis: 0, descending: true)
    for c in 0..3 {
        for i in 1..300 {
            if up[i - 1, c] > up[i, c] { return 1 }
        }
        for i in 0..300 {
            if wide[order[i, c] as u64, c] != up[i, c] { return 3 }
        }
    }
    for c in 0..2 {
        for i in 1..300 {
            if down[i - 1, c] < down[i, c] { return 2 }
            if down[i - 1, c] == down[i, c] && back[i - 1, c] >= back[i, c] { return 4 }
        }
        for i in 0..300 {
            if narrow[back[i, c] as u64, c] != down[i, c] { return 3 }
        }
    }
    return 0
}
"#
    );
    assert_eq!(run_program("tensor_sort_long_axis0.nr", &source), 0);
}

#[test]
fn a_long_float_run_keeps_nan_last_and_signed_zeros_in_order() {
    let source = format!(
        "{LONG_RUN_CHECKS}{}",
        r#"
func main() -> i32 {
    val nan = 0.0 / 0.0
    mut v: Tensor<f64, [1000]> = Tensor::zeros()
    mut seed: i64 = 3
    mut nans: i32 = 0
    for i in 0..1000 {
        seed = next(seed)
        val pick = (seed / 65536) % 13
        if pick == 0 {
            v[i] = nan
            nans = nans + 1
        } else if pick == 1 {
            v[i] = -0.0
        } else {
            v[i] = (pick as f64 - 7.0) * 0.5
        }
    }
    val up: Tensor<f64, [1000]> = v.sort()
    val down: Tensor<f64, [1000]> = v.sort(descending: true)
    val order: Tensor<i32, [1000]> = v.argsort()
    val back: Tensor<i32, [1000]> = v.argsort(descending: true)
    val real = 1000 - nans
    for i in 0..1000 {
        if (i >= real) != up[i].is_nan() { return 6 }
        if (i >= real) != down[i].is_nan() { return 6 }
        if i >= real {
            // The NaNs, in source order in both directions.
            if i > real && order[i - 1] >= order[i] { return 4 }
            if i > real && back[i - 1] >= back[i] { return 4 }
        } else {
            if v[order[i] as u64] != up[i] { return 3 }
            if v[back[i] as u64] != down[i] { return 3 }
            if i > 0 {
                if up[i - 1] > up[i] { return 1 }
                if down[i - 1] < down[i] { return 2 }
                if up[i - 1] == up[i] && order[i - 1] >= order[i] { return 4 }
                if down[i - 1] == down[i] && back[i - 1] >= back[i] { return 4 }
            }
        }
    }
    val (top, at) = v.topk(k: 20)
    for i in 0..20 {
        if top[i] != down[i] || at[i] != back[i] { return 5 }
    }
    return 0
}
"#
    );
    assert_eq!(run_program("tensor_sort_long_f64.nr", &source), 0);
}
