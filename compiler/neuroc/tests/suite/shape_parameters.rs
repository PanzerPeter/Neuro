// End-to-end tests for shape parameters.
//
// A bare generic name used as a whole tensor shape (`Tensor<T, S>`) or spliced into one
// (`[..S, N]`) is a shape parameter: it stands for any number of extents, is bound by
// monomorphization, and is read in a body through `S.rank()` and `S.count()`. A size
// parameter, extent or shape, may be passed on to another generic call or type.
use crate::compile_harness::{assert_check_error, stdout_of};

#[test]
fn one_template_serves_every_rank() {
    let source = r#"
func shift<T: Float, S>(x: Tensor<T, S>, by: T) -> Tensor<T, S> {
    &x * by + &x
}

func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m: Tensor<f64, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val c = Tensor::<f32, [2, 2, 2]>::ones()
    println("{shift(v, 1.0).sum()} {shift(m, 2.0).sum()} {shift(c, 0.5).sum()}")
    return 0
}
"#;
    // 2 * 6, 3 * 21, 1.5 * 8
    assert_eq!(stdout_of("ranks.nr", source), "12.0 63.0 12.0\n");
}

#[test]
fn rank_and_count_read_the_bound_shape() {
    let source = r#"
func describe<S>(x: &Tensor<f32, S>) -> u64 {
    S.count() * 10 + S.rank()
}

func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m = Tensor::<f32, [2, 3]>::zeros()
    val c = Tensor::<f32, [2, 2, 3]>::zeros()
    val s = Tensor::<f32, []>::scalar(1.0)
    println("{describe(&v)} {describe(&m)} {describe(&c)} {describe(&s)}")
    return 0
}
"#;
    assert_eq!(stdout_of("rank.nr", source), "31 62 123 10\n");
}

#[test]
fn a_splice_takes_the_leading_axes_and_broadcasts_a_trailing_bias() {
    let source = r#"
func add_bias<S, N>(x: Tensor<f32, [..S, N]>, b: &Tensor<f32, [N]>) -> Tensor<f32, [..S, N]> {
    x + b
}

func leading<S, N>(x: &Tensor<f32, [..S, N]>) -> u64 {
    S.count() * 100 + N
}

func main() -> i32 {
    val b: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val row = add_bias(Tensor::<f32, [3]>::zeros(), &b)
    val batch = add_bias(Tensor::<f32, [2, 2, 3]>::ones(), &b)
    println("{row.sum()} {batch.sum()} {leading(&row)} {leading(&batch)}")
    return 0
}
"#;
    // 6, 4 rows of (2 + 3 + 4), S = [] counts 1, S = [2, 2] counts 4
    assert_eq!(stdout_of("bias.nr", source), "6.0 36.0 103 403\n");
}

#[test]
fn a_shape_parameter_is_passed_on_by_inference_and_by_turbofish() {
    let source = r#"
func rank_of<S>(x: &Tensor<f32, S>) -> u64 {
    S.rank()
}

func both<S>(x: &Tensor<f32, S>) -> u64 {
    rank_of(x) * 10 + rank_of::<S>(x)
}

func last_rank<S, N>(x: &Tensor<f32, [..S, N]>) -> u64 {
    rank_of(x)
}

func zeros_of<S>() -> Tensor<f32, S> {
    Tensor::<f32, S>::zeros()
}

func main() -> i32 {
    val m = Tensor::<f32, [2, 3]>::ones()
    val z = zeros_of::<[4, 2]>()
    println("{both(&m)} {last_rank(&m)} {z.sum()} {both(&z)}")
    return 0
}
"#;
    assert_eq!(stdout_of("pass_shape.nr", source), "22 2 0.0 22\n");
}

/// BUG-027: an extent parameter passed on, by inference, by turbofish, and through a type.
#[test]
fn an_extent_parameter_is_passed_on_to_a_call_and_a_type() {
    let source = r#"
func sum_n<N>(t: &Tensor<i32, [N]>) -> i32 {
    mut total = 0
    for i in 0..N { total = total + t[i] }
    return total
}

func delegate<N>(t: &Tensor<i32, [N]>) -> i32 {
    return sum_n(t) + sum_n::<N>(t)
}

struct Ring<T, CAP> {
    data: [T; CAP],
}

impl<T: Copy, CAP> Ring<T, CAP> {
    func copy(&self) -> Ring<T, CAP> {
        Ring { data: self.data }
    }

    func same(&self) -> Self {
        Ring { data: self.data }
    }
}

func capacity<CAP>(r: &Ring<i32, CAP>) -> u64 {
    CAP
}

func sum_arr<N>(a: &[i32; N]) -> i32 {
    mut total = 0
    for x in a { total = total + *x }
    return total
}

func delegate_arr<M>(a: &[i32; M]) -> i32 {
    sum_arr(a) + sum_arr::<M>(a)
}

func main() -> i32 {
    val v: Tensor<i32, [3]> = [1, 2, 3]
    val w: Tensor<i32, [2]> = [10, 20]
    val r = Ring { data: [4, 5, 6, 7] }
    val arr = [1, 1, 1, 1, 1]
    println("{delegate(&v)} {delegate(&w)} {capacity(&r.copy())} {capacity(&r.same())} {delegate_arr(&arr)}")
    return 0
}
"#;
    // Two instances of `delegate`, so a wrong extent cannot pass unnoticed.
    assert_eq!(stdout_of("pass_extent.nr", source), "12 60 4 4 10\n");
}

#[test]
fn a_type_and_its_impl_take_a_shape_parameter() {
    let source = r#"
struct Layer<S> {
    w: Tensor<f32, S>,
}

impl<S> Layer<S> {
    func apply(&self, x: &Tensor<f32, S>) -> Tensor<f32, S> {
        &self.w * x
    }

    func cells(&self) -> u64 {
        S.count()
    }

    func doubled(&self) -> Layer<S> {
        Layer { w: &self.w + &self.w }
    }
}

func cells_of<S>(l: &Layer<S>) -> u64 {
    l.cells()
}

func main() -> i32 {
    val x: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val l = Layer { w: Tensor::<f32, [2, 3]>::ones() * 2.0 }
    val small: Layer<[3]> = Layer { w: [1.0, 1.0, 1.0] }
    println("{l.apply(&x).sum()} {l.doubled().apply(&x).sum()} {cells_of(&l)} {small.cells()}")
    return 0
}
"#;
    assert_eq!(stdout_of("layer.nr", source), "42.0 84.0 6 3\n");
}

/// `Layer<[N]>` reads as a slice type to the parser, so the one-name shape is the case
/// that has to be recognized from the extent in scope.
#[test]
fn a_shape_argument_may_be_built_from_extent_parameters() {
    let source = r#"
struct Layer<S> {
    w: Tensor<f32, S>,
}

func wrap<N>(x: Tensor<f32, [N]>) -> Layer<[N]> {
    Layer { w: x }
}

func wrap_rows<N>(x: Tensor<f32, [N, 2]>) -> Layer<[N, 2]> {
    Layer { w: x }
}

func main() -> i32 {
    val l = wrap(Tensor::<f32, [3]>::ones())
    val m = wrap_rows(Tensor::<f32, [3, 2]>::ones())
    println("{l.w.sum()} {m.w.sum()}")
    return 0
}
"#;
    assert_eq!(stdout_of("built.nr", source), "3.0 6.0\n");
}

#[test]
fn an_enum_payload_takes_a_shape_parameter() {
    let source = r#"
enum Slot<S> {
    Empty,
    Full(Tensor<f32, S>),
}

func cells<S>(s: &Slot<S>) -> u64 {
    match s {
        Slot::Empty => 0,
        Slot::Full(_) => S.count(),
    }
}

func fill<S>(t: Tensor<f32, S>) -> Slot<S> {
    Slot::Full(t)
}

func main() -> i32 {
    val a = fill(Tensor::<f32, [2, 3]>::ones())
    val e: Slot<[4]> = Slot::Empty
    val total = match a {
        Slot::Full(t) => t.sum(),
        Slot::Empty => 0.0,
    }
    println("{cells(&fill(Tensor::<f32, [5]>::zeros()))} {cells(&e)} {total}")
    return 0
}
"#;
    assert_eq!(stdout_of("slot.nr", source), "5 0 6.0\n");
}

/// One shape-generic computation, `3x - 1` summed, in seven forms at two ranks. Every
/// form must agree with the hand value: 3 * 6 - 3 = 15 for `[1, 2, 3]`, and
/// 3 * 21 - 6 = 57 for the 2x3 matrix of 1 to 6.
#[test]
fn a_shape_generic_result_agrees_across_equivalent_forms() {
    let source = r#"
func affine<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    x * 3.0 - 1.0
}

func tail<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    affine(x)
}

func bound<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    val y = affine(x)
    return y
}

func branch<S>(x: &Tensor<f32, S>, pick: i32) -> Tensor<f32, S> {
    match pick {
        0 => if S.rank() > 0 { affine(x) } else { x.clone() },
        _ => x.clone(),
    }
}

func through<S>(x: &Tensor<f32, S>) -> Tensor<f32, S> {
    tail::<S>(x)
}

struct Holder<S> {
    t: Tensor<f32, S>,
}

impl<S> Holder<S> {
    func affine(&self) -> Tensor<f32, S> {
        affine(&self.t)
    }
}

func total<S>(t: Tensor<f32, S>) -> Tensor<f32, S> {
    t
}

func main() -> i32 {
    val v: Tensor<f32, [3]> = [1.0, 2.0, 3.0]
    val m: Tensor<f32, [2, 3]> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
    val f = |x: &Tensor<f32, [3]>| affine(x)
    println("{tail(&v).sum()} {bound(&v).sum()} {branch(&v, 0).sum()} {through(&v).sum()} {f(&v).sum()} {total(affine(&v)).sum()} {Holder { t: v.clone() }.affine().sum()}")
    println("{tail(&m).sum()} {bound(&m).sum()} {branch(&m, 0).sum()} {through(&m).sum()} {total(affine(&m)).sum()} {Holder { t: m.clone() }.affine().sum()}")
    return 0
}
"#;
    assert_eq!(
        stdout_of("forms.nr", source),
        "15.0 15.0 15.0 15.0 15.0 15.0 15.0\n57.0 57.0 57.0 57.0 57.0 57.0\n"
    );
}

#[test]
fn a_parameter_shape_splices_at_most_once() {
    assert_check_error(
        "twice.nr",
        "func f<S, R>(x: Tensor<f32, [..S, ..R]>) -> u64 { 0 }\nfunc main() -> i32 { 0 }\n",
        "splices 2 shape parameters",
    );
}

#[test]
fn an_operation_that_needs_a_rank_is_refused_on_a_splice() {
    for (body, operation) in [
        ("x[0]", "an index"),
        ("x.sum()", "`.sum`"),
        ("x.item()", "`.item`"),
    ] {
        let source = format!(
            "func f<S>(x: &Tensor<f32, S>) -> f32 {{ {body} }}\nfunc main() -> i32 {{ 0 }}\n"
        );
        assert_check_error("rank.nr", &source, &format!("{operation} needs the rank"));
    }
    assert_check_error(
        "transpose.nr",
        "func f<S>(x: Tensor<f32, [..S, 2, 2]>) -> Tensor<f32, [..S, 2, 2]> { x.t() }\n\
         func main() -> i32 { 0 }\n",
        "`.t` needs the rank",
    );
}

#[test]
fn a_splice_broadcasts_only_against_itself() {
    assert_check_error(
        "broadcast.nr",
        "func f<S>(x: Tensor<f32, S>, y: Tensor<f32, [3]>) -> Tensor<f32, S> { x + y }\n\
         func main() -> i32 { 0 }\n",
        "do not broadcast",
    );
}

#[test]
fn a_shape_parameter_is_no_value_and_no_extent() {
    assert_check_error(
        "value.nr",
        "func f<S>(x: &Tensor<f32, S>) -> u64 {\n    val s = S\n    0\n}\nfunc main() -> i32 { 0 }\n",
        "shape parameter 'S' is not a value",
    );
    assert_check_error(
        "kind.nr",
        "func f<S>(x: &Tensor<f32, S>, y: &Tensor<f32, [S]>) -> u64 { 0 }\nfunc main() -> i32 { 0 }\n",
        "'S' is a shape parameter, so a shape writes it as `..S`",
    );
    assert_check_error(
        "unknown.nr",
        "func f(x: &Tensor<f32, [..Q]>) -> u64 { 0 }\nfunc main() -> i32 { 0 }\n",
        "shape parameter 'Q' is not declared",
    );
}

#[test]
fn a_dynamic_axis_binds_no_shape_parameter() {
    let text = crate::compile_harness::rejected(
        "dynamic.nr",
        "func f<S>(x: &Tensor<f32, [..S, 2]>) -> u64 { S.rank() }\n\
         func main() -> i32 {\n\
             val d: Tensor<f32, [?, 2]> = Tensor::<f32, [3, 2]>::zeros()\n\
             return f(&d) as i32\n\
         }\n",
    );
    assert!(text.contains("type mismatch"), "{text}");
    assert!(!text.contains("cannot be inferred"), "{text}");
}

#[test]
fn a_turbofish_shape_goes_to_a_shape_parameter_only() {
    assert_check_error(
        "turbofish.nr",
        "func f<S>(x: &Tensor<f32, S>) -> u64 { S.rank() }\n\
         func main() -> i32 {\n\
             val m = Tensor::<f32, [2]>::zeros()\n\
             return f::<f32>(&m) as i32\n\
         }\n",
        "a shape argument was expected",
    );
}
