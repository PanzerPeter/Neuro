// Named dynamic extents `?B`: binding at calls, annotations, constructors and struct
// literals, the naming rule for operators, the operations a dynamic axis supports, `.as_shape`
// narrowing, and where a name may and may not be written.

use super::semantic_errors;
use crate::errors::TypeError;

/// The `Option` the narrowing's result and `val ... else` read, declared here because the
/// unit-test harness type-checks bare source without the prelude.
const OPTION_DECL: &str = "enum Option<T> { Some(T), None }\n";

fn errors_with_option(body: &str) -> Vec<TypeError> {
    semantic_errors(&format!("{OPTION_DECL}{body}"))
}

fn count(errors: &[TypeError], matches: impl Fn(&TypeError) -> bool) -> usize {
    errors.iter().filter(|e| matches(e)).count()
}

fn is_axis_mismatch(e: &TypeError) -> bool {
    matches!(e, TypeError::TensorDynamicAxisMismatch { .. })
}

// --- Binding at a call -------------------------------------------------------------------

#[test]
fn a_call_binds_the_name_from_its_argument_and_substitutes_the_result() {
    let errors = semantic_errors(
        r#"
func first(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x
}

func caller(x: Tensor<f32, [?C, 4]>) -> i32 {
    val same: Tensor<f32, [?C, 4]> = first(x)
    val static_arg = Tensor::<f32, [7, 4]>::ones()
    val kept: Tensor<f32, [7, 4]> = first(static_arg)
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_call_result_keeps_the_static_extent_of_the_argument_that_binds_the_name() {
    let errors = semantic_errors(
        r#"
func first(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x
}

func caller() -> i32 {
    val seven = Tensor::<f32, [7, 4]>::ones()
    val wrong: Tensor<f32, [8, 4]> = first(seven)
    return 0
}
"#,
    );
    assert!(
        count(&errors, |e| matches!(e, TypeError::Mismatch { .. })) == 1,
        "the call's result is `[7, 4]`; got {errors:?}"
    );
}

#[test]
fn two_arguments_carrying_one_name_check_only_at_run_time() {
    let errors = semantic_errors(
        r#"
func pair(a: Tensor<f32, [?B]>, b: Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    return a
}

func caller() -> i32 {
    val seven = Tensor::<f32, [7]>::ones()
    val five = Tensor::<f32, [5]>::ones()
    val mixed: Tensor<f32, [7]> = pair(seven, five)
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a second extent is the run-time check's to report; got {errors:?}"
    );
}

#[test]
fn an_anonymous_argument_does_not_hide_the_extent_a_later_argument_gives() {
    let errors = semantic_errors(
        r#"
func pair(a: Tensor<f32, [?B]>, b: Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    return a
}

func caller(anon: Tensor<f32, [?]>) -> i32 {
    val seven = Tensor::<f32, [7]>::ones()
    val result: Tensor<f32, [7]> = pair(anon, seven)
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_call_whose_names_are_all_anonymous_returns_an_anonymous_extent() {
    let errors = semantic_errors(
        r#"
func pair(a: Tensor<f32, [?B]>, b: Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    return a
}

func caller(p: Tensor<f32, [?]>, q: Tensor<f32, [?]>) -> Tensor<f32, [?]> {
    val anon: Tensor<f32, [?]> = pair(p, q)
    return anon
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

// --- Annotations ------------------------------------------------------------------------

#[test]
fn an_annotation_binds_a_fresh_name_from_the_value_and_reads_it_as_u64() {
    let errors = semantic_errors(
        r#"
func rows(x: Tensor<f32, [?, 4]>) -> u64 {
    val y: Tensor<f32, [?B, 4]> = x
    return B
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn an_annotation_naming_a_bound_name_accepts_any_extent() {
    let errors = semantic_errors(
        r#"
func check(x: Tensor<f32, [?B]>, y: Tensor<f32, [7]>) -> i32 {
    val z: Tensor<f32, [?B]> = y
    return 0
}
"#,
    );
    assert!(
        errors.is_empty(),
        "a bound name is checked at run time; got {errors:?}"
    );
}

#[test]
fn an_annotation_bound_from_another_name_makes_the_two_one_value() {
    let errors = semantic_errors(
        r#"
func alias(x: &Tensor<f32, [?C]>) -> i32 {
    val y: &Tensor<f32, [?B]> = x
    val z = y + x
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "B is C; got {errors:?}");
}

#[test]
fn a_name_in_an_annotation_is_not_in_scope_past_its_block() {
    let errors = semantic_errors(
        r#"
func scoped(x: Tensor<f32, [?]>) -> u64 {
    if true {
        val y: Tensor<f32, [?C]> = x
    }
    return C
}
"#,
    );
    assert!(
        count(&errors, |e| matches!(
            e,
            TypeError::UndefinedVariable { .. }
        )) == 1,
        "C ends with its block; got {errors:?}"
    );
}

#[test]
fn an_assignment_is_strict_about_a_dynamic_name() {
    let errors = semantic_errors(
        r#"
func assign(x: Tensor<f32, [?B]>, anon: Tensor<f32, [?]>) -> i32 {
    mut y: Tensor<f32, [?B]> = x
    y = anon
    return 0
}
"#,
    );
    assert!(
        count(&errors, |e| matches!(e, TypeError::Mismatch { .. })) == 1,
        "an assignment expects `?B` exactly; got {errors:?}"
    );
}

// --- Constructors -----------------------------------------------------------------------

#[test]
fn a_constructor_binds_a_fresh_name_from_its_dims() {
    let errors = semantic_errors(
        r#"
func fill(n: u64) -> u64 {
    val z = Tensor::<f32, [?B, 3]>::zeros([n])
    return B
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_constructor_naming_a_bound_name_takes_no_dims() {
    let errors = semantic_errors(
        r#"
func fill(x: Tensor<f32, [?B]>) -> Tensor<f32, [?B, 3]> {
    val z = Tensor::<f32, [?B, 3]>::zeros()
    return z
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_constructor_with_too_few_dims_is_refused() {
    let errors = semantic_errors(
        r#"
func fill(n: u64) -> i32 {
    val z = Tensor::<f32, [?, ?]>::zeros([n])
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorDimsCount {
                expected: 2,
                found: 1,
                ..
            }]
        ),
        "two open axes, one entry; got {errors:?}"
    );
}

#[test]
fn a_constructor_with_dims_and_no_open_axis_is_refused() {
    let errors = semantic_errors(
        r#"
func fill(n: u64) -> i32 {
    val z = Tensor::<f32, [2, 3]>::zeros([n])
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorDimsWithoutOpenAxis { .. }]
        ),
        "a closed shape takes no dims; got {errors:?}"
    );
}

#[test]
fn a_name_written_twice_takes_one_dims_entry() {
    let errors = semantic_errors(
        r#"
func square(n: u64) -> u64 {
    val m = Tensor::<f32, [?B, ?B]>::identity([n])
    return B
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn an_identity_needs_a_square_shape_by_one_name_or_static_extents() {
    let errors = semantic_errors(
        r#"
func square(n: u64) -> i32 {
    val a = Tensor::<f32, [?, ?]>::identity([n, n])
    val b = Tensor::<f32, [?A, ?B]>::identity([n, n])
    val c = Tensor::<f32, [3, 3]>::identity()
    return 0
}
"#,
    );
    assert!(
        count(&errors, |e| matches!(
            e,
            TypeError::TensorConstructorNotApplicable { .. }
        )) == 2,
        "`[?, ?]` and `[?A, ?B]` are not square; got {errors:?}"
    );
}

#[test]
fn random_normal_takes_its_dims_beside_mean_and_std() {
    let errors = semantic_errors(
        r#"
func draw(n: u64) -> i32 {
    val w = Tensor::<f32, [?B, 2]>::random_normal(0.0, 1.0, [n])
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_literal_stays_static_and_from_refuses_a_dynamic_shape() {
    let errors = semantic_errors(
        r#"
func build() -> i32 {
    val a: Tensor<f32, [?B, 2]> = [[1.0, 2.0]]
    val b = Tensor::<f32, [?B, 2]>::from([[1.0, 2.0]])
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::TensorDynamicExtent { .. }
        )),
        2,
        "a literal and `from` are sized by their elements; got {errors:?}"
    );
}

// --- Struct literals and fields ---------------------------------------------------------

const BATCH: &str = r#"
struct Batch {
    x: Tensor<f32, [?B, 4]>,
    y: Tensor<f32, [?B]>
}
"#;

#[test]
fn a_struct_literal_binds_its_names_from_the_fields_in_order() {
    let errors = semantic_errors(&format!(
        "{BATCH}
func build(a: Tensor<f32, [?C, 4]>, b: Tensor<f32, [?C]>) -> Batch {{
    return Batch {{ x: a, y: b }}
}}
"
    ));
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_later_field_checks_the_name_an_earlier_one_bound_at_run_time() {
    let errors = semantic_errors(&format!(
        "{BATCH}
func build(a: Tensor<f32, [?C, 4]>) -> Batch {{
    val seven = Tensor::<f32, [7]>::ones()
    return Batch {{ x: a, y: seven }}
}}
"
    ));
    assert!(
        errors.is_empty(),
        "the second field is checked when the value is built; got {errors:?}"
    );
}

#[test]
fn a_field_read_is_anonymous_and_so_is_a_field_assignment() {
    let errors = semantic_errors(&format!(
        "{BATCH}
func read(bt: Batch, t: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {{
    return bt.x
}}

func write(a: Tensor<f32, [?, 4]>, c: Tensor<f32, [?, 4]>) -> i32 {{
    mut bt = Batch {{ x: a, y: Tensor::<f32, [?]>::zeros([0]) }}
    bt.x = c
    return 0
}}
"
    ));
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::ReturnTypeMismatch { .. }
        )),
        1,
        "a read is `?`, which a return of `?B` refuses; the write takes any extent; got {errors:?}"
    );
    assert_eq!(errors.len(), 1, "got {errors:?}");
}

#[test]
fn a_struct_field_static_axis_still_mismatches() {
    let errors = semantic_errors(&format!(
        "{BATCH}
func build(a: Tensor<f32, [?C, 5]>) -> Batch {{
    return Batch {{ x: a, y: a }}
}}
"
    ));
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })),
        "the second axis is 4, not 5; got {errors:?}"
    );
}

// --- `B` as a u64 value ------------------------------------------------------------------

#[test]
fn a_dynamic_name_reads_as_a_u64_in_a_loop() {
    let errors = semantic_errors(
        r#"
func total(x: Tensor<f32, [?B]>) -> u64 {
    mut sum: u64 = 0
    for i in 0..B {
        sum = sum + i
    }
    return sum
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_name_and_a_variable_of_one_identifier_are_refused() {
    let errors = semantic_errors(
        r#"
func clash(B: u64, x: Tensor<f32, [?B]>) -> i32 {
    return 0
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::DynamicNameConflict { .. }]),
        "a parameter and a dynamic name share one scope; got {errors:?}"
    );
}

#[test]
fn a_variable_named_like_an_in_scope_name_is_refused() {
    let errors = semantic_errors(
        r#"
func clash(x: Tensor<f32, [?B]>) -> i32 {
    val B = 3
    return 0
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::DynamicNameConflict { .. }]),
        "got {errors:?}"
    );
}

// --- The naming rule for operators (R8) ------------------------------------------------

#[test]
fn an_operator_joins_two_axes_of_the_same_name() {
    let errors = semantic_errors(
        r#"
func add(x: Tensor<f32, [?B, 3]>, y: Tensor<f32, [?B, 3]>) -> Tensor<f32, [?B, 3]> {
    return x + y
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn an_operator_refuses_a_literal_against_a_name_and_names_the_narrowing() {
    let errors = semantic_errors(
        r#"
func add(x: Tensor<f32, [?B]>, y: Tensor<f32, [3]>) -> i32 {
    val z = x + y
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorDynamicAxisMismatch { .. }]
        ),
        "got {errors:?}"
    );
    let message = errors[0].to_string();
    assert!(
        message.contains(".as_shape::<[...]>()"),
        "the hint names the narrowing; got {message}"
    );
}

#[test]
fn an_operator_refuses_two_different_names_an_anonymous_axis_and_a_literal_one() {
    let errors = semantic_errors(
        r#"
func mix(x: Tensor<f32, [?B]>, y: Tensor<f32, [?C]>, z: Tensor<f32, [?]>, one: Tensor<f32, [1]>) -> i32 {
    val a = x + y
    val b = x + z
    val c = x + one
    return 0
}
"#,
    );
    assert_eq!(count(&errors, is_axis_mismatch), 3, "got {errors:?}");
}

#[test]
fn a_rank_extension_is_not_a_dynamic_axis_conflict() {
    let errors = semantic_errors(
        r#"
func rows(x: Tensor<f32, [?B, 4]>, bias: Tensor<f32, [4]>) -> Tensor<f32, [?B, 4]> {
    return x + bias
}
"#,
    );
    assert!(
        errors.is_empty(),
        "the bias has no axis the name is on; got {errors:?}"
    );
}

#[test]
fn a_dynamic_axis_is_never_stretched() {
    let errors = semantic_errors(
        r#"
func stretch(x: Tensor<f32, [?B, 4]>, y: Tensor<f32, [1, 4]>) -> i32 {
    val z = x + y
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorDynamicAxisMismatch { .. }]
        ),
        "a literal 1 does not stretch a name; got {errors:?}"
    );
}

#[test]
fn a_compound_assignment_joins_the_name_of_its_target() {
    let errors = semantic_errors(
        r#"
func update(w: Tensor<f32, [?B]>, g: Tensor<f32, [?B]>) -> i32 {
    mut m: Tensor<f32, [?B]> = w
    m -= g
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_compound_assignment_refuses_another_extent_on_a_dynamic_target() {
    let errors = semantic_errors(
        r#"
func update(w: Tensor<f32, [?B]>, g: Tensor<f32, [3]>) -> i32 {
    mut m: Tensor<f32, [?B]> = w
    m -= g
    return 0
}
"#,
    );
    assert!(errors.iter().any(is_axis_mismatch), "got {errors:?}");
}

#[test]
fn a_matrix_product_contracts_a_name_only_with_itself() {
    let errors = semantic_errors(
        r#"
func product(a: Tensor<f32, [2, ?B]>, b: Tensor<f32, [?B, 4]>, c: Tensor<f32, [3, 4]>) -> i32 {
    val ok = a @ b
    val bad = a @ c
    return 0
}
"#,
    );
    assert_eq!(count(&errors, is_axis_mismatch), 1, "got {errors:?}");
}

#[test]
fn a_matrix_product_keeps_a_dynamic_row_and_column_count() {
    let errors = semantic_errors(
        r#"
func product(a: Tensor<f32, [?B, 3]>, b: Tensor<f32, [3, ?C]>) -> Tensor<f32, [?B, ?C]> {
    return a @ b
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn zip_pairs_a_dynamic_axis_only_with_its_name() {
    let errors = semantic_errors(
        r#"
func zipped(x: Tensor<f32, [?B]>, y: Tensor<f32, [?B]>, z: Tensor<f32, [?]>) -> Tensor<f32, [?B]> {
    val a = x.zip(y, |p, q| p + q)
    val b = x.zip(z, |p, q| p + q)
    return a
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::TensorZipShapeMismatch { .. }
        )),
        1,
        "`?` does not zip with `?B`; got {errors:?}"
    );
}

#[test]
fn einsum_contracts_a_letter_of_one_name_on_both_operands() {
    let errors = semantic_errors(
        r#"
func contract(a: Tensor<f32, [?, ?B]>, b: Tensor<f32, [?B, 2]>, c: Tensor<f32, [?, ?]>) -> i32 {
    val ok = einsum("ij,jk->ik", a, b)
    val bad = einsum("ij,jk->ik", a, c)
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::EinsumExtentConflict { .. }
        )),
        1,
        "the `j` of `?` and `?` is two unrelated values; got {errors:?}"
    );
}

// --- Single-operand operations (R9) ----------------------------------------------------

/// Each single-operand operation on a dynamic axis, one function apiece so that the
/// receiver each one consumes is never read again.
#[test]
fn every_single_operand_operation_keeps_the_dynamic_axis() {
    let errors = semantic_errors(
        r#"
enum Device {
    CPU,
    GPU(i32)
}

func map(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.map(|v| v * 2.0)
}

func sort(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.sort()
}

func argsort(x: Tensor<f32, [?B, 4]>) -> Tensor<i32, [?B, 4]> {
    return x.argsort()
}

func copy(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.clone()
}

func place(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.to(Device::CPU)
}

func root(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.sqrt()
}

func detach(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.detach()
}

func transpose(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [4, ?B]> {
    return x.t()
}

func permute(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [4, ?B]> {
    return x.permute([1, 0])
}

func mean(x: Tensor<f32, [?B, 4]>) -> f32 {
    return x.mean()
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn reductions_and_topk_read_a_dynamic_axis() {
    let errors = semantic_errors(
        r#"
func read(x: Tensor<f32, [?B, 4]>) -> i32 {
    val s: Tensor<f32, [4]> = x.sum(axis: 0)
    val m: Tensor<f32, [4]> = x.mean(axis: 0)
    val top = x.topk(k: 2, axis: 0)
    val lowest = x.min()
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn indexing_keeps_or_fixes_a_dynamic_axis() {
    let errors = semantic_errors(
        r#"
func index(x: Tensor<f32, [?B, 4]>) -> i32 {
    val element = x[0, 1]
    val row: Tensor<f32, [4]> = x[0, ..]
    val window: Tensor<f32, [2, 4]> = x[0..2, ..]
    val column: Tensor<f32, [?B]> = x[.., 1]
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_reversed_or_stepped_range_on_a_dynamic_axis_takes_its_static_extent() {
    let errors = semantic_errors(
        r#"
func index(x: Tensor<f32, [?B, 4]>) -> i32 {
    val reversed: Tensor<f32, [2, 4]> = x[(0..2).rev(), ..]
    val stepped: Tensor<f32, [2, 4]> = x[(0..4).step(2), ..]
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn reshape_keeps_a_dynamic_name_it_is_given_and_makes_a_minus_one_anonymous() {
    let errors = semantic_errors(
        r#"
func named(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?B, 4]> {
    return x.reshape([B, 4])
}

func inferred(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?, 4]> {
    return x.reshape([-1, 4])
}

func flat(x: Tensor<f32, [?B, 4]>) -> Tensor<f32, [?]> {
    return x.flatten()
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_reshape_of_a_dynamic_count_into_a_static_shape_is_checked_at_run_time() {
    let errors = semantic_errors(
        r#"
func reshape(x: Tensor<f32, [?B, 4]>) -> i32 {
    val static_view: Tensor<f32, [2, 2]> = x.reshape([2, 2])
    return 0
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

// --- `.as_shape` (R11) -----------------------------------------------------------------

#[test]
fn as_shape_binds_a_fresh_name_that_is_in_scope_after_it() {
    let errors = errors_with_option(
        r#"
func narrow(x: Tensor<f32, [?, 4]>) -> u64 {
    val Option::Some(labels) = x.as_shape::<[?B, 4]>() else {
        return 0
    }
    return B
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn as_shape_returns_an_option_of_the_written_shape() {
    let errors = errors_with_option(
        r#"
func narrow(x: Tensor<f32, [?, 4]>) -> i32 {
    val Option::Some(square) = x.as_shape::<[4, 4]>() else {
        return 0
    }
    val kept: Tensor<f32, [4, 4]> = square
    return 1
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn as_shape_refuses_a_borrowed_receiver() {
    let errors = errors_with_option(
        r#"
func narrow(x: &Tensor<f32, [?, 4]>) -> i32 {
    val narrowed = x.as_shape::<[?B, 4]>()
    return 0
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::TensorAsShapeBorrowed { .. }]),
        "a borrow cannot be consumed; got {errors:?}"
    );
}

#[test]
fn as_shape_refuses_a_shape_of_another_rank() {
    let errors = errors_with_option(
        r#"
func narrow(x: Tensor<f32, [?, 4]>) -> i32 {
    val narrowed = x.as_shape::<[?B]>()
    return 0
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorAsShapeRank {
                expected: 2,
                found: 1,
                ..
            }]
        ),
        "got {errors:?}"
    );
}

#[test]
fn as_shape_consumes_its_receiver() {
    let errors = errors_with_option(
        r#"
func narrow(x: Tensor<f32, [?, 4]>) -> i32 {
    val narrowed = x.as_shape::<[?B, 4]>()
    val again = x.as_shape::<[?B, 4]>()
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::UseOfMovedValue { .. })),
        "the second use reads a moved value; got {errors:?}"
    );
}

// --- Where a name may be written (R2) --------------------------------------------------

#[test]
fn a_name_at_the_top_of_a_parameter_or_a_return_is_accepted() {
    let errors = errors_with_option(
        r#"
struct Holder {
    x: Tensor<f32, [?B]>
}

func top(a: Tensor<f32, [?B]>, b: &Tensor<f32, [?B]>, c: &mut Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    return a
}

func tuple(a: Tensor<f32, [?B]>) -> (Tensor<f32, [?B]>, i32) {
    return (a, 0)
}

func option(a: Tensor<f32, [?B]>) -> Option<Tensor<f32, [?B]>> {
    return Option::Some(a)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_name_nested_in_a_parameter_type_is_refused() {
    let errors = semantic_errors(
        r#"
func nested(v: Vec<Tensor<f32, [?B]>>) -> i32 {
    return 0
}

func tuple(t: (Tensor<f32, [?B]>, i32)) -> i32 {
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::DynamicNameNotAllowed { .. }
        )),
        2,
        "got {errors:?}"
    );
}

#[test]
fn a_name_in_an_enum_payload_or_a_trait_declaration_is_refused() {
    let errors = semantic_errors(
        r#"
enum Payload {
    Data(Tensor<f32, [?B]>),
    Empty
}

trait Shape {
    func area(&self, x: Tensor<f32, [?B]>) -> i32
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::DynamicNameNotAllowed { .. }
        )),
        2,
        "got {errors:?}"
    );
}

#[test]
fn a_name_in_a_generic_argument_is_refused() {
    let errors = semantic_errors(
        r#"
struct Pair<T> {
    first: T
}

func generic(p: Pair<Tensor<f32, [?B]>>) -> i32 {
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::DynamicNameNotAllowed { .. }
        )),
        1,
        "got {errors:?}"
    );
}

#[test]
fn a_name_in_a_closure_parameter_is_refused() {
    let errors = semantic_errors(
        r#"
func apply(t: Tensor<f32, [?B]>) -> i32 {
    val f = |x: Tensor<f32, [?C]>| -> i32 { 0 }
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::DynamicNameNotAllowed { .. }
        )),
        1,
        "got {errors:?}"
    );
}

#[test]
fn a_name_in_a_grad_signature_is_refused() {
    let errors = semantic_errors(
        r#"
@grad
func loss(x: Tensor<f32, [?B]>) -> Tensor<f32, []> {
    return x.sum()
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DynamicNameNotAllowed { .. })),
        "a differentiated function is compiled for static shapes; got {errors:?}"
    );
}

#[test]
fn a_return_name_no_parameter_names_is_refused() {
    let errors = semantic_errors(
        r#"
func made() -> Tensor<f32, [?B]> {
    val z = Tensor::<f32, [?B]>::zeros([3])
    return z
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DynamicNameUnbound { .. })),
        "the extent must come from an argument; got {errors:?}"
    );
}

#[test]
fn a_return_does_not_narrow_an_anonymous_extent_to_a_name() {
    let errors = semantic_errors(
        r#"
func widen(x: Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
    val anon: Tensor<f32, [?]> = x
    return anon
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(
            e,
            TypeError::ReturnTypeMismatch { .. }
        )),
        1,
        "the annotation takes `?B` as `?`, but the return does not take `?` as `?B`; got {errors:?}"
    );
}

#[test]
fn an_anonymous_extent_returns_into_an_anonymous_result() {
    let errors = semantic_errors(
        r#"
func keep(x: Tensor<f32, [?B]>) -> Tensor<f32, [?]> {
    return x
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

// --- Compile-time agreement (R5) and generics -------------------------------------------

#[test]
fn a_static_parameter_refuses_a_dynamic_argument() {
    let errors = semantic_errors(
        r#"
func need_three(x: Tensor<f32, [3]>) -> i32 {
    return 0
}

func caller(d: Tensor<f32, [?B]>, a: Tensor<f32, [?]>) -> i32 {
    val first = need_three(d)
    val second = need_three(a)
    return 0
}
"#,
    );
    assert_eq!(
        count(&errors, |e| matches!(e, TypeError::Mismatch { .. })),
        2,
        "a run-time extent is not a literal; got {errors:?}"
    );
}

#[test]
fn a_dynamic_return_refuses_a_static_result() {
    let errors = semantic_errors(
        r#"
func shrink(x: Tensor<f32, [?]>) -> Tensor<f32, [3]> {
    return x
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::ReturnTypeMismatch { .. }]),
        "got {errors:?}"
    );
}

#[test]
fn a_generic_function_binds_its_names_per_call() {
    let errors = semantic_errors(
        r#"
func keep<T>(x: Tensor<T, [?B]>) -> Tensor<T, [?B]> {
    return x
}

func caller(t: Tensor<f32, [?C]>) -> Tensor<f32, [?C]> {
    return keep(t)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_method_call_substitutes_its_result_like_a_function_call() {
    let errors = semantic_errors(
        r#"
struct Keeper {
    scale: i32
}

impl Keeper {
    func pass(&self, x: Tensor<f32, [?B]>) -> Tensor<f32, [?B]> {
        return x
    }
}

func caller(k: Keeper, t: Tensor<f32, [?C]>) -> Tensor<f32, [?C]> {
    return k.pass(t)
}
"#,
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn a_trait_impl_method_names_no_dynamic_extent() {
    let errors = semantic_errors(
        r#"
trait Measure {
    func size(&self) -> i32
}

struct Keeper {
    scale: i32
}

impl Measure for Keeper {
    func size(&self, x: Tensor<f32, [?B]>) -> i32 {
        return 0
    }
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DynamicNameNotAllowed { .. })),
        "the trait's declaration names no extent; got {errors:?}"
    );
}

#[test]
fn a_constructor_argument_takes_its_callee_extent_without_binding_it() {
    let errors = semantic_errors(
        r#"
func take(x: Tensor<f32, [?B]>) -> i32 {
    return 0
}

func caller(n: u64) -> u64 {
    val taken = take(Tensor::zeros([n]))
    return B
}
"#,
    );
    assert!(
        matches!(errors.as_slice(), [TypeError::UndefinedVariable { .. }]),
        "the callee's `B` is not the caller's: only the return reads it, and is refused; got {errors:?}"
    );
}

#[test]
fn a_loop_variable_named_like_a_dynamic_name_is_refused() {
    let errors = semantic_errors(
        r#"
func clash(x: Tensor<f32, [?B]>) -> i32 {
    for B in 0..3 {
    }
    return 0
}
"#,
    );
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, TypeError::DynamicNameConflict { .. })),
        "a loop variable is a value binding; got {errors:?}"
    );
}

#[test]
fn an_inferred_constructor_over_a_bound_name_is_refused() {
    // A written type lowering also keeps types the constructor: the annotation and the
    // annotated binding assigned to. An unannotated binding's type is the value's, which
    // lowering has with its names erased, so a bound name there needs the turbofish.
    let errors = semantic_errors(
        r#"
func g(x: &Tensor<f32, [?B]>) -> f32 {
    val t: Tensor<f32, [?B, 2]> = Tensor::ones()
    val fresh: Tensor<f32, [?C]> = Tensor::ones(dims: [4])
    mut kept: Tensor<f32, [?B]> = x.clone()
    kept = Tensor::ones()
    mut v = x.clone()
    v = Tensor::ones()
    t.sum() + fresh.sum() + kept.sum() + v.sum()
}
"#,
    );
    assert!(
        matches!(
            errors.as_slice(),
            [TypeError::TensorCtorBoundNameInferred { name, .. }] if name == "B"
        ),
        "only the unannotated binding's assignment needs the turbofish; got {errors:?}"
    );
}
