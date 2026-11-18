// End-to-end tests for shape checks per instantiation.
//
// A shape-generic body is checked once, before its extents have values, so a reshape, a
// flatten or a reduction over extent parameters is typed symbolically (`[N, 2]` flattens to
// `[2 * N]`, and a shape may write `D / H`). Whether the element counts agree, and whether a
// reduced run is empty, is answered at each instance and reported at the call that
// supplied the extents.
use crate::compile_harness::{assert_check_error, rejected, stdout_of};

/// The peaks of each head: `x` split into `H` heads of `D / H` and the maximum of each
/// head kept. For `0..12` as `[2, 6]` in three heads the peaks sum to
/// 1 + 3 + 5 + 7 + 9 + 11 = 36, and for one row of two heads to 5 + 11 = 16.
const PEAKS: &str = r#"
func tail<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    x.reshape([B, H, D / H]).max(axis: -1)
}

func bound<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    val heads = x.reshape([B, H, -1])
    val peaks = heads.max(axis: -1)
    return peaks
}

func branch<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    match B > 0 {
        true => if H > 0 { x.reshape([B, H, D / H]).max(axis: -1) } else { Tensor::<i32, [B, H]>::zeros() },
        false => Tensor::<i32, [B, H]>::zeros(),
    }
}

func through<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    val peak = |t: Tensor<i32, [B, H, D / H]>| t.max(axis: -1)
    peak(x.reshape([B, H, D / H]))
}

func keep<M>(t: Tensor<i32, [M]>) -> Tensor<i32, [M]> {
    t
}

func argument<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    keep(x.reshape([B, H, D / H]).max(axis: -1).flatten()).reshape([B, H])
}

func flat_first<B, D, H>(x: Tensor<i32, [B, D]>) -> Tensor<i32, [B, H]> {
    x.flatten().reshape([B * H, -1]).max(axis: -1).reshape([B, -1])
}

func all<B, D, H>(x: &Tensor<i32, [B, D]>, heads: &Tensor<i32, [H]>) {
    println("{tail::<B, D, H>(x.clone()).sum()} {bound::<B, D, H>(x.clone()).sum()} {branch::<B, D, H>(x.clone()).sum()} {through::<B, D, H>(x.clone()).sum()} {argument::<B, D, H>(x.clone()).sum()} {flat_first::<B, D, H>(x.clone()).sum()}")
}

func main() -> i32 {
    val x: Tensor<i32, [2, 6]> = [[0, 1, 2, 3, 4, 5], [6, 7, 8, 9, 10, 11]]
    all(&x, &Tensor::<i32, [3]>::zeros())
    val y: Tensor<i32, [1, 12]> = [[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]]
    all(&y, &Tensor::<i32, [2]>::zeros())
    return 0
}
"#;

#[test]
fn a_shape_generic_reshape_agrees_in_every_form() {
    assert_eq!(
        stdout_of("peaks.nr", PEAKS),
        "36 36 36 36 36 36\n16 16 16 16 16 16\n"
    );
}

#[test]
fn a_flatten_types_as_the_product_of_its_extents() {
    let source = r#"
func twice<N>(t: Tensor<f32, [N, 2]>) -> Tensor<f32, [2 * N]> {
    t.flatten()
}

func cells<S>(x: Tensor<f32, S>) -> Tensor<f32, [S.count()]> {
    x.flatten()
}

func rows<S, N>(x: Tensor<f32, [..S, N]>) -> Tensor<f32, [S.count(), N]> {
    x.reshape([-1, N])
}

func main() -> i32 {
    println("{twice(Tensor::<f32, [4, 2]>::ones()).sum()} {cells(Tensor::<f32, [2, 3, 2]>::ones()).sum()}")
    val r = rows(Tensor::<f32, [2, 2, 3]>::ones())
    println("{r.sum(axis: 0).sum()}")
    return 0
}
"#;
    assert_eq!(stdout_of("flat.nr", source), "8.0 12.0\n12.0\n");
}

#[test]
fn a_reduction_over_extent_parameters_and_splices_compiles() {
    let source = r#"
func last_max<S, N>(x: &Tensor<f32, [..S, N]>) -> Tensor<f32, S> {
    x.max(axis: -1)
}

func everything<S>(x: &Tensor<f32, S>) -> f32 {
    x.sum()
}

func row_sums<N, K>(t: &Tensor<f32, [N, K]>) -> Tensor<f32, [N]> {
    t.sum(axis: 1)
}

func main() -> i32 {
    val m: Tensor<f32, [2, 3]> = [[1.0, 5.0, 2.0], [7.0, 3.0, 4.0]]
    println("{last_max(&m).sum()} {everything(&m)} {row_sums(&m).max()}")
    return 0
}
"#;
    assert_eq!(stdout_of("reduce.nr", source), "12.0 22.0 14.0\n");
}

#[test]
fn a_traversal_and_a_contraction_take_a_shape_generic_tensor() {
    let source = r#"
func doubled<N>(t: &Tensor<i32, [N]>) -> Tensor<i32, [N]> {
    t.map(|x: i32| x * 2)
}

func summed<S>(a: &Tensor<f32, S>, b: &Tensor<f32, S>) -> Tensor<f32, S> {
    a.zip(b, |x: f32, y: f32| x + y)
}

func product<M, K, N>(a: &Tensor<f32, [M, K]>, b: &Tensor<f32, [K, N]>) -> Tensor<f32, [M, N]> {
    einsum("ij,jk->ik", a, b)
}

func main() -> i32 {
    val v: Tensor<i32, [3]> = [1, 2, 3]
    val m = Tensor::<f32, [2, 3]>::ones()
    val n = Tensor::<f32, [3, 4]>::ones()
    println("{doubled(&v).sum()} {summed(&m, &m).sum()} {product(&m, &n).sum()}")
    return 0
}
"#;
    assert_eq!(stdout_of("family.nr", source), "12 12.0 24.0\n");
}

#[test]
fn a_count_mismatch_is_reported_at_the_call_that_supplied_the_extents() {
    let text = rejected(
        "mismatch.nr",
        r#"
func heads<B, D, H>(x: Tensor<f32, [B, D]>) -> Tensor<f32, [B, H, D / H]> {
    x.reshape([B, H, D / H])
}

func outer<D>(x: Tensor<f32, [1, D]>) -> f32 {
    heads::<1, D, 4>(x).sum()
}

func main() -> i32 {
    val x = Tensor::<f32, [1, 6]>::ones()
    println("{outer(x)}")
    return 0
}
"#,
    );
    assert!(
        text.contains("would hold 4 elements at this instance but the receiver holds 6"),
        "{text}"
    );
    assert!(
        text.contains("mismatch.nr:12:"),
        "the call in `main` is the location: {text}"
    );
    assert!(text.contains("`heads` with B = 1, D = 6, H = 4"), "{text}");
}

#[test]
fn a_where_predicate_moves_the_error_to_the_signature() {
    assert_check_error(
        "where.nr",
        r#"
func heads<B, D, H>(x: Tensor<f32, [B, D]>) -> Tensor<f32, [B, H, D / H]>
where D % H == 0
{
    x.reshape([B, H, D / H])
}

func main() -> i32 {
    val h = heads::<2, 6, 4>(Tensor::<f32, [2, 6]>::ones())
    return 0
}
"#,
        "`where` predicate is not satisfied",
    );
}

#[test]
fn an_inferred_extent_that_does_not_divide_is_reported_per_instance() {
    assert_check_error(
        "indivisible.nr",
        r#"
func split<N>(t: Tensor<f32, [N]>) -> f32 {
    t.reshape([4, -1]).sum()
}

func main() -> i32 {
    println("{split(Tensor::<f32, [6]>::ones())}")
    return 0
}
"#,
        "`-1` cannot be inferred at this instance",
    );
}

#[test]
fn an_empty_run_is_reported_per_instance() {
    let text = rejected(
        "empty.nr",
        r#"
func peak<N>(t: &Tensor<f32, [N]>) -> f32 {
    t.max()
}

func main() -> i32 {
    println("{peak(&Tensor::<f32, [0]>::zeros())}")
    return 0
}
"#,
    );
    assert!(
        text.contains("reduces over no elements at this instance"),
        "{text}"
    );
    assert!(text.contains("`peak` with N = 0"), "{text}");
}

#[test]
fn an_extent_with_no_value_at_an_instance_is_reported_at_the_call() {
    let text = rejected(
        "negative.nr",
        r#"
func shorter<N>(t: Tensor<f32, [N]>) -> Tensor<f32, [N - 1]> {
    Tensor::<f32, [N - 1]>::zeros()
}

func main() -> i32 {
    val s = shorter(Tensor::<f32, [0]>::zeros())
    return 0
}
"#,
    );
    assert!(
        text.contains("the extent `N - 1` has no value at this instance"),
        "{text}"
    );
    assert!(text.contains("negative.nr:7:"), "{text}");
}

#[test]
fn extent_expressions_compare_by_normal_form() {
    assert_check_error(
        "form.nr",
        "func f<N>(t: Tensor<f32, [N, 2]>) -> Tensor<f32, [N * 3]> { t.flatten() }\n\
         func main() -> i32 { 0 }\n",
        "Tensor<f32, [2 * N]>",
    );
}

#[test]
fn an_extent_expression_binds_nothing_and_is_checked_once_bound() {
    let source = r#"
func pair<N>(a: &Tensor<f32, [N + 1]>, b: &Tensor<f32, [N]>) -> f32 {
    a.sum() + b.sum()
}

func main() -> i32 {
    println("{pair(&Tensor::<f32, [5]>::ones(), &Tensor::<f32, [4]>::ones())}")
    return 0
}
"#;
    assert_eq!(stdout_of("pair.nr", source), "9.0\n");
    assert_check_error(
        "pair_bad.nr",
        &source.replace("[5]>::ones()", "[7]>::ones()"),
        "expected &Tensor<f32, [5]>, found &Tensor<f32, [7]>",
    );
}

#[test]
fn a_reduction_along_an_axis_a_splice_moves_is_refused() {
    assert_check_error(
        "axis.nr",
        "func f<S, N>(x: &Tensor<f32, [..S, N]>) -> u64 { val r = x.sum(axis: 0)\n 0 }\n\
         func main() -> i32 { 0 }\n",
        "`.sum` along axis 0 needs the rank",
    );
}

#[test]
fn an_extent_admits_arithmetic_and_a_shape_count_only() {
    assert_check_error(
        "call.nr",
        "func f<N>(t: Tensor<f32, [g(N)]>) -> u64 { 0 }\nfunc main() -> i32 { 0 }\n",
        "expected ']' to close a tensor shape",
    );
    assert_check_error(
        "rank.nr",
        "func f<S>(t: Tensor<f32, [S.rank()]>) -> u64 { 0 }\nfunc main() -> i32 { 0 }\n",
        "`.count()` after a shape parameter in an extent",
    );
}
