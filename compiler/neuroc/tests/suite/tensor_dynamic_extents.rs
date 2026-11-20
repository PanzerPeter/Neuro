// Named dynamic extents end to end: `?B` bound at calls, annotations, constructors and struct
// literals, read as a `u64`, carried through every tensor operation, and narrowed back with
// `.as_shape`. A dynamic extent is not monomorphized: one compiled body reads it from the
// tensor's DLPack shape, so these tests drive bodies that run at several extents.

use crate::compile_harness::{CompileTest, compile_tagged, rejected, run};
use std::process::Command;

/// The panic text of a program that aborts, after checking it did abort.
fn panic_text(source: &str, tag: &str) -> String {
    let exe = compile_tagged(source, &format!("dyn_extent_{tag}"), "0");
    let output = Command::new(&exe).output().expect("run executable");
    assert!(
        output.status.code().is_none_or(|code| code != 0),
        "{tag}: expected a panic, got {:?}",
        output.status
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_call_binds_the_name_and_the_body_reads_it_as_a_u64() {
    let source = r#"
func rows(x: &Tensor<f32, [?B, 3]>) -> u64 {
    B
}

func main() -> i32 {
    val a = Tensor::<f32, [4, 3]>::zeros()
    val b = Tensor::<f32, [7, 3]>::ones()
    return (rows(&a) * 10 + rows(&b)) as i32
}
"#;
    assert_eq!(run("dyn_call_binds.nr", source), 47);
}

#[test]
fn two_arguments_sharing_a_name_combine_with_no_check_in_the_body() {
    let source = r#"
func dot(x: &Tensor<f32, [?B]>, y: &Tensor<f32, [?B]>) -> f32 {
    (x * y).sum()
}

func main() -> i32 {
    val x: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val y: Tensor<f32, [3]> = [4.0, 5.0, 6.0]
    val z: Tensor<f32, [2]> = [10.0, 1.0]
    return (dot(&x, &y) + dot(&z, &z)) as i32
}
"#;
    // 4 + 10 + 18 = 32, and 100 + 1 = 101.
    assert_eq!(run("dyn_shared_name.nr", source), 133);
}

#[test]
fn a_call_whose_arguments_disagree_panics_before_the_body_naming_both_values() {
    let source = r#"
func dot(x: &Tensor<f32, [?B]>, y: &Tensor<f32, [?B]>) -> f32 {
    println("the body ran")
    (x * y).sum()
}

func main() -> i32 {
    val x = Tensor::<f32, [3]>::ones()
    val y = Tensor::<f32, [4]>::ones()
    return dot(&x, &y) as i32
}
"#;
    let text = panic_text(source, "call_mismatch");
    assert!(
        text.contains("dynamic extent `B` is 3 from `x` but 4 from `y`"),
        "{text}"
    );
}

#[test]
fn a_constructor_sizes_from_dims_and_a_bound_name_needs_none() {
    let source = r#"
func total(n: u64) -> f32 {
    val a = Tensor::<f32, [?N, 2]>::ones(dims: [n])
    val b = Tensor::<f32, [?N, 2]>::ones()
    (a + b).sum()
}

func main() -> i32 {
    return (total(5) + total(1)) as i32
}
"#;
    // 2 * n * 2 at n = 5 and n = 1.
    assert_eq!(run("dyn_constructor.nr", source), 24);
}

#[test]
fn an_annotation_binds_from_the_value_and_checks_once_bound() {
    let source = r#"
func make(n: u64) -> Tensor<f32, [?, 2]> {
    Tensor::<f32, [?, 2]>::ones(dims: [n])
}

func main() -> i32 {
    val x: Tensor<f32, [?B, 2]> = make(6)
    val y: Tensor<f32, [?B, 2]> = make(6)
    return ((&x + &y).sum() as u64 + B) as i32
}
"#;
    // 6 * 2 * 2 = 24, plus B = 6.
    assert_eq!(run("dyn_annotation.nr", source), 30);

    let mismatch = r#"
func make(n: u64) -> Tensor<f32, [?, 2]> {
    Tensor::<f32, [?, 2]>::ones(dims: [n])
}

func main() -> i32 {
    val x: Tensor<f32, [?B, 2]> = make(6)
    val y: Tensor<f32, [?B, 2]> = make(5)
    return 0
}
"#;
    let text = panic_text(mismatch, "annotation_mismatch");
    assert!(
        text.contains("dynamic extent `B` is 6 but `y` has 5"),
        "{text}"
    );
}

#[test]
fn a_struct_literal_checks_its_fields_against_each_other() {
    let source = r#"
struct Batch {
    x: Tensor<f32, [?B, 2]>,
    y: Tensor<f32, [?B]>,
}

func make(n: u64) -> Batch {
    Batch {
        x: Tensor::<f32, [?, 2]>::ones(dims: [n]),
        y: Tensor::<f32, [?]>::ones(dims: [n]),
    }
}

func size(batch: &Batch) -> f32 {
    batch.x.sum() + batch.y.sum()
}

func main() -> i32 {
    val small = make(2)
    val large = make(5)
    return (size(&small) * 10.0 + size(&large)) as i32
}
"#;
    // 2 * 2 + 2 = 6, and 5 * 2 + 5 = 15.
    assert_eq!(run("dyn_struct.nr", source), 75);

    let mismatch = r#"
struct Batch {
    x: Tensor<f32, [?B, 2]>,
    y: Tensor<f32, [?B]>,
}

func main() -> i32 {
    val batch = Batch {
        x: Tensor::<f32, [?, 2]>::ones(dims: [3]),
        y: Tensor::<f32, [?]>::ones(dims: [4]),
    }
    return 0
}
"#;
    let text = panic_text(mismatch, "struct_mismatch");
    assert!(
        text.contains("dynamic extent `B` of `Batch` is 3 from `x` but 4 from `y`"),
        "{text}"
    );
}

#[test]
fn as_shape_narrows_to_a_static_shape_or_returns_none() {
    let source = r#"
func corner(x: Tensor<i32, [?, 3]>) -> i32 {
    val Some(t) = x.as_shape::<[2, 3]>() else {
        return -1
    }
    t[1, 2]
}

func widen(t: Tensor<i32, [2, 3]>) -> Tensor<i32, [?, 3]> {
    t
}

func main() -> i32 {
    val fits: Tensor<i32, [2, 3]> = [[1, 2, 3], [4, 5, 6]]
    val wrong = Tensor::<i32, [?, 3]>::zeros(dims: [4])
    return corner(widen(fits)) * 10 + corner(wrong)
}
"#;
    assert_eq!(run("dyn_as_shape.nr", source), 59);
}

#[test]
fn as_shape_introduces_a_fresh_name_for_the_rest_of_the_block() {
    let source = r#"
func load(n: u64) -> Tensor<f32, [?, 4]> {
    Tensor::<f32, [?, 4]>::ones(dims: [n])
}

func main() -> i32 {
    val Some(x) = load(3).as_shape::<[?B, 4]>() else {
        return -1
    }
    val Some(y) = load(3).as_shape::<[?B, 4]>() else {
        return -2
    }
    val Some(z) = load(2).as_shape::<[?B, 4]>() else {
        return ((&x + &y).sum() as u64 + B) as i32
    }
    return 0
}
"#;
    // x and y agree on B = 3; z has 2 rows, so the third narrowing fails: 24 + 3.
    assert_eq!(run("dyn_as_shape_fresh.nr", source), 27);
}

#[test]
fn matmul_and_reductions_run_over_a_dynamic_batch() {
    let source = r#"
func forward(x: &Tensor<f32, [?B, 2]>, w: &Tensor<f32, [2, 3]>) -> Tensor<f32, [?B, 3]> {
    x @ w
}

func main() -> i32 {
    val w: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val x = Tensor::<f32, [?, 2]>::ones(dims: [4])
    val y = forward(&x, &w)
    val row_sums = y.sum(axis: 1)
    return (y.sum() + row_sums.mean() + y.max()) as i32
}
"#;
    // Every row is [5, 7, 9]: sum 4 * 21 = 84, row mean 21, max 9.
    assert_eq!(run("dyn_matmul.nr", source), 114);
}

#[test]
fn a_dynamic_contracted_axis_uses_its_run_time_extent() {
    let source = r#"
func gram(x: &Tensor<f32, [2, ?K]>, y: &Tensor<f32, [?K, 2]>) -> Tensor<f32, [2, 2]> {
    x @ y
}

func main() -> i32 {
    val x = Tensor::<f32, [2, ?]>::ones(dims: [5])
    val y = Tensor::<f32, [?, 2]>::ones(dims: [5])
    return gram(&x, &y).sum() as i32
}
"#;
    assert_eq!(run("dyn_contract.nr", source), 20);
}

#[test]
fn slices_and_reshapes_keep_or_compute_the_run_time_extent() {
    let source = r#"
func columns(x: &Tensor<f32, [?B, 4]>) -> f32 {
    val second = x[.., 1]
    val head = x[0..2, ..]
    second.sum() + head.sum()
}

func pairs(x: Tensor<f32, [?B, 4]>) -> u64 {
    val p = x.reshape([-1, 2])
    p.sum() as u64
}

func main() -> i32 {
    val x = Tensor::<f32, [?, 4]>::ones(dims: [3])
    val y = Tensor::<f32, [?, 4]>::ones(dims: [3])
    return (columns(&x) + pairs(y) as f32) as i32
}
"#;
    // 3 + 8, then 12 elements summed.
    assert_eq!(run("dyn_slice_reshape.nr", source), 23);
}

#[test]
fn a_slice_past_a_dynamic_extent_panics() {
    let source = r#"
func head(x: &Tensor<f32, [?B, 4]>) -> f32 {
    x[0..3, ..].sum()
}

func main() -> i32 {
    val x = Tensor::<f32, [?, 4]>::ones(dims: [2])
    return head(&x) as i32
}
"#;
    let text = panic_text(source, "slice_bound");
    assert!(text.contains("tensor index out of bounds"), "{text}");
}

#[test]
fn a_reshape_whose_count_does_not_fit_panics() {
    let source = r#"
func square(x: Tensor<f32, [?B, 4]>) -> f32 {
    x.reshape([2, 2]).sum()
}

func main() -> i32 {
    val x = Tensor::<f32, [?, 4]>::ones(dims: [3])
    return square(x) as i32
}
"#;
    let text = panic_text(source, "reshape_count");
    assert!(text.contains("tensor reshape: 12 elements"), "{text}");
}

#[test]
fn clone_sort_map_and_element_access_on_a_dynamic_axis() {
    let source = r#"
func run(n: u64) -> i32 {
    mut x = Tensor::<i32, [?]>::zeros(dims: [n])
    mut i: u64 = 0
    while i < n {
        x[i] = (n - i) as i32
        i += 1
    }
    val copy = x.clone()
    val sorted = copy.sort()
    val doubled = sorted.map(|v: i32| -> i32 { v * 2 })
    doubled[0] * 100 + doubled[n - 1]
}

func main() -> i32 {
    return run(4)
}
"#;
    // x = [4, 3, 2, 1], sorted [1, 2, 3, 4], doubled [2, 4, 6, 8].
    assert_eq!(run("dyn_clone_sort.nr", source), 208);
}

#[test]
fn einsum_binds_a_dynamic_letter_across_operands() {
    let source = r#"
func outer_sum(a: &Tensor<f32, [?N]>, b: &Tensor<f32, [?N, 3]>) -> Tensor<f32, [3]> {
    einsum("n,nk->k", a, b)
}

func main() -> i32 {
    val a = Tensor::<f32, [?]>::ones(dims: [4])
    val b = Tensor::<f32, [?, 3]>::ones(dims: [4])
    return outer_sum(&a, &b).sum() as i32
}
"#;
    assert_eq!(run("dyn_einsum.nr", source), 12);
}

/// The feature's core computation in several equivalent forms, each computed by hand:
/// the sum of `x * 2 + x` over `n` rows of ones is `3 * n * 2`.
#[test]
fn equivalent_forms_agree() {
    let source = r#"
func tail(x: &Tensor<f32, [?B, 2]>) -> f32 {
    (x * 2.0 + x).sum()
}

func bound(x: &Tensor<f32, [?B, 2]>) -> f32 {
    val y = x * 2.0 + x
    val total = y.sum()
    return total
}

func branch(x: &Tensor<f32, [?B, 2]>, flag: bool) -> f32 {
    if flag {
        match B {
            0 => 0.0,
            _ => (x * 2.0 + x).sum(),
        }
    } else {
        -1.0
    }
}

func apply(x: &Tensor<f32, [?, 2]>, f: (&Tensor<f32, [?, 2]>) -> f32) -> f32 {
    f(x)
}

func narrowed(x: Tensor<f32, [?, 2]>) -> f32 {
    val Some(s) = x.as_shape::<[5, 2]>() else {
        return -1.0
    }
    (&s * 2.0 + &s).sum()
}

func main() -> i32 {
    val x = Tensor::<f32, [?, 2]>::ones(dims: [5])
    val a = tail(&x)
    val b = bound(&x)
    val c = branch(&x, true)
    val d = apply(&x, |t: &Tensor<f32, [?, 2]>| -> f32 { tail(t) })
    val e = narrowed(x.clone())
    if a == 30.0 && b == 30.0 && c == 30.0 && d == 30.0 && e == 30.0 {
        return 30
    }
    return 1
}
"#;
    assert_eq!(run("dyn_equivalent.nr", source), 30);
}

/// Allocation, narrowing (both outcomes), reshapes and slices in a loop, under the leak
/// check, so a lost shape block or buffer shows up.
#[test]
fn dynamic_tensors_release_everything_they_allocate() {
    let source = r#"
func step(n: u64) -> f32 {
    val x = Tensor::<f32, [?B, 4]>::ones(dims: [n])
    val y = (&x + &x).reshape([-1, 2])
    val head = x[0..1, ..]
    val copy = x.clone()
    val narrowed = copy.as_shape::<[3, 4]>()
    val extra = match narrowed {
        Some(t) => t.sum(),
        None => 0.0,
    }
    y.sum() + head.sum() + extra
}

func main() -> i32 {
    mut total: f32 = 0.0
    mut n: u64 = 1
    while n < 6 {
        total += step(n)
        n += 1
    }
    return total as i32
}
"#;
    // y sums to 8n over n = 1..5 (120), head 4 each (20), extra 12 at n = 3.
    assert_eq!(
        CompileTest::new().run_leak_checked("dyn_leaks.nr", source),
        152
    );
}

#[test]
fn the_naming_rule_refuses_axes_it_cannot_prove_equal() {
    let anonymous = r#"
func f(x: &Tensor<f32, [?]>, y: &Tensor<f32, [?]>) -> f32 {
    (x + y).sum()
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_rule_anonymous.nr", anonymous);
    assert!(errors.contains("as_shape"), "{errors}");

    let two_names = r#"
func f(x: &Tensor<f32, [?A]>, y: &Tensor<f32, [?B]>) -> f32 {
    (x + y).sum()
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_rule_names.nr", two_names);
    assert!(errors.contains("?A") && errors.contains("?B"), "{errors}");

    let literal = r#"
func f(x: &Tensor<f32, [?B, 2]>, y: &Tensor<f32, [3, 2]>) -> f32 {
    (x + y).sum()
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_rule_literal.nr", literal);
    assert!(errors.contains("as_shape"), "{errors}");
}

#[test]
fn misplaced_names_and_constructor_arguments_are_compile_errors() {
    let unbound_return = r#"
func f() -> Tensor<f32, [?B]> {
    Tensor::<f32, [?]>::zeros(dims: [2])
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_unbound_return.nr", unbound_return);
    assert!(errors.contains("?B"), "{errors}");

    let nested = r#"
func f(xs: &Vec<Tensor<f32, [?B]>>) -> i32 { return 0 }
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_nested_name.nr", nested);
    assert!(errors.contains("?B"), "{errors}");

    let dims_count = r#"
func main() -> i32 {
    val t = Tensor::<f32, [?, ?]>::zeros(dims: [2])
    return 0
}
"#;
    let errors = rejected("dyn_dims_count.nr", dims_count);
    assert!(errors.contains("dims"), "{errors}");

    let no_open_axis = r#"
func main() -> i32 {
    val t = Tensor::<f32, [2, 3]>::zeros(dims: [2])
    return 0
}
"#;
    let errors = rejected("dyn_dims_static.nr", no_open_axis);
    assert!(errors.contains("dims"), "{errors}");

    let borrowed = r#"
func f(x: &Tensor<f32, [?, 3]>) -> i32 {
    val narrowed = x.as_shape::<[2, 3]>()
    return 0
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_as_shape_borrowed.nr", borrowed);
    assert!(errors.contains("as_shape"), "{errors}");

    let rank = r#"
func f(x: Tensor<f32, [?, 3]>) -> i32 {
    val narrowed = x.as_shape::<[6]>()
    return 0
}
func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_as_shape_rank.nr", rank);
    assert!(errors.contains("as_shape"), "{errors}");
}

/// A static empty run is a compile error; a dynamic one is refused when it runs, rather
/// than reading an element that is not there.
#[test]
fn a_reduction_over_an_empty_dynamic_run_panics() {
    let source = r#"
func biggest(x: &Tensor<f32, [?B]>) -> f32 {
    x.max()
}

func main() -> i32 {
    val empty = Tensor::<f32, [?]>::zeros(dims: [0])
    return biggest(&empty) as i32
}
"#;
    let text = panic_text(source, "empty_run");
    assert!(text.contains("`.max()` reduces over no elements"), "{text}");
}

/// A closure reads a name in scope however the body spells it: as a value, in a
/// constructor's turbofish, or in a `val` annotation. A name spelled only in a type was not
/// captured, and the closure body read a local it did not have.
#[test]
fn a_closure_captures_a_name_it_reads_only_through_a_type() {
    let source = r#"
func main() -> i32 {
    val x = Tensor::<f32, [?]>::ones(dims: [3])
    val Some(y) = x.as_shape::<[?N]>() else { return 1 }
    val built = || -> f32 { Tensor::<f32, [?N]>::ones().sum() }
    val annotated = || -> f32 {
        val t: Tensor<f32, [?N, 2]> = Tensor::<f32, [?N, 2]>::ones()
        t.sum()
    }
    val read = || -> u64 { N }
    return built() as i32 * 100 + annotated() as i32 * 10 + read() as i32 + y.sum() as i32 - 300
}
"#;
    // N is 3: 3 * 100 + 6 * 10 + 3 + 3, less 300 to fit an exit code.
    assert_eq!(
        CompileTest::new().run_leak_checked("dyn_closure_type_read.nr", source),
        66
    );
}

/// A constructor without a turbofish takes its shape from the expected type, and lowering
/// sees that type with its names erased, so a bound name there is refused with the
/// spelling that works.
#[test]
fn an_inferred_constructor_over_a_bound_name_is_refused() {
    let source = r#"
func g(x: &Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    Tensor::ones()
}

func main() -> i32 { return 0 }
"#;
    let errors = rejected("dyn_ctor_inferred.nr", source);
    assert!(errors.contains("Tensor::<f32, [?B]>::ones()"), "{errors}");
}

/// A call's result names the caller's extent through a generic impl's method as through a
/// free function. The instance method had no extent signature, so lowering typed the result
/// `?` where the checker had typed it static, and the sum that followed failed to lower.
#[test]
fn a_generic_impl_method_result_takes_the_argument_extent() {
    let source = r#"
struct Wrap<T> { v: T }

impl<T> Wrap<T> {
    func double(&self, x: &Tensor<f32, [?B]>) -> Tensor<f32, [?B]> { x + x }
}

func main() -> i32 {
    val w = Wrap { v: 1 }
    val y = Tensor::<f32, [3]>::ones()
    val z = w.double(&y)
    val s: Tensor<f32, [3]> = &z + &y
    return (s.sum() + z[2]) as i32
}
"#;
    // Each element of s is 2 + 1, three of them, plus z[2] = 2.
    assert_eq!(run("dyn_generic_impl_result.nr", source), 11);
}

/// Inside an `Option` the result keeps its dynamic axis even when the argument is static,
/// as the instance lowering builds does; a dynamic argument's name carries through.
#[test]
fn an_option_result_keeps_a_dynamic_axis_for_a_static_argument() {
    let wrapped = r#"
func twice(x: &Tensor<f32, [?B]>) -> Option<Tensor<f32, [?B]>> { Some(x + x) }

func main() -> i32 {
    val y = Tensor::<f32, [3]>::ones()
    val Some(z) = twice(&y) else { return 1 }
    val s: Tensor<f32, [3]> = &z + &y
    return s.sum() as i32
}
"#;
    let errors = rejected("dyn_option_static.nr", wrapped);
    assert!(errors.contains("as_shape"), "{errors}");

    let named = r#"
func twice(x: &Tensor<f32, [?B]>) -> Option<Tensor<f32, [?B]>> { Some(x + x) }

func main() -> i32 {
    val x = Tensor::<f32, [?]>::ones(dims: [3])
    val Some(y) = x.as_shape::<[?N]>() else { return 1 }
    val Some(z) = twice(&y) else { return 1 }
    return (&z + &y).sum() as i32
}
"#;
    assert_eq!(run("dyn_option_named.nr", named), 9);
}
