// Generic tensor element types and the `Numeric` / `Integer` / `Float` bounds: a body
// checked once against its bound, lowered per instance at the concrete scalar type.
use crate::compile_harness::{assert_check_error, compile_rejected, stdout_of};

#[test]
fn numeric_bounds_give_scalar_arithmetic_per_instance() {
    let source = r#"
func mix<T: Numeric>(a: T, b: T) -> T { a + b * b - a / b }
func less<T: Numeric>(a: T, b: T) -> bool { a < b && -a != b }
func bits<T: Integer>(a: T, b: T) -> T { (a % b) ^ (a & b) | ~b }
func lerp<T: Float>(a: T, b: T, t: T) -> T { a + (b - a) * t }
func inc<T: Integer>(x: T) -> T { x + 1 }
func twice<T: Float>(x: T) -> T { x * 2.0 }
func copies<T: Numeric>(v: T) -> T { val a = v
    a + v }
func soft<T: Float>(x: T) -> T { (x.exp() + T::one()).log() }
func wraps<T: Integer>(x: T) -> T { x.wrapping_add(x) }
func half<T: Float>() -> T { T::from_f64(0.5) }
func main() -> i32 {
    println("{mix(3, 2)} {mix(1.5f32, 0.5f32)} {less(1i8, 2i8)} {less(2.0, 1.0)}")
    println("{bits(12u8, 10u8)} {lerp(0.0, 10.0, 0.25)} {lerp(1.0f32, 3.0f32, 0.5f32)}")
    println("{inc(41u8)} {inc(-1i64)} {twice(1.5f32)} {twice(0.25f16)} {copies(7)}")
    println("{soft(0.0)} {wraps(200u8)} {half::<bf16>()} {(0.0f32 / 0.0f32).is_nan()}")
    0
}
"#;
    assert_eq!(
        stdout_of("scalars.nr", source),
        "6 -1.25 true false\n255 2.5 2.0\n42 0 3.0 0.5 14\n0.6931471805599453 144 0.5 true\n"
    );
}

#[test]
fn a_tensor_element_may_be_a_type_parameter() {
    let source = r#"
func first<T>(t: Tensor<T, [3]>) -> T { t[0] }
func total<T: Numeric>(t: &Tensor<T, [3]>) -> T { t.sum() }
func scale<T: Float>(t: Tensor<T, [2]>, k: T) -> Tensor<T, [2]> { t * k }
func filled<T: Numeric>() -> Tensor<T, [3]> { Tensor::<T, [3]>::ones() }
func mm<T: Numeric>(a: Tensor<T, [2, 2]>, b: Tensor<T, [2, 2]>) -> Tensor<T, [2, 2]> { a @ b }
func grow<T: Numeric>(t: &mut Tensor<T, [2]>) { *t += T::one() }
func soft<T: Float>(t: &Tensor<T, [2]>) -> Tensor<T, [2]> { t.exp() }
func avg<T: Float>(t: &Tensor<T, [4]>) -> T { t.mean() }
struct Pair<T> { t: Tensor<T, [2]> }
impl<T: Numeric> Pair<T> {
    func sum(&self) -> T { self.t.sum() }
}
func main() -> i32 {
    val flags = Tensor::<bool, [3]>::from([false, true, true])
    val ints = Tensor::<u16, [3]>::from([3, 4, 5])
    val s = scale(t: Tensor::<f32, [2]>::from([1.0, 2.0]), k: 3.0)
    val m = mm(Tensor::<i32, [2, 2]>::from([[1, 2], [3, 4]]), Tensor::<i32, [2, 2]>::from([[0, 1], [1, 0]]))
    mut g = Tensor::<f64, [2]>::from([0.0, 1.0])
    grow(&mut g)
    val e = soft(&g)
    val p = Pair { t: Tensor::<i8, [2]>::from([-3, 5]) }
    val h = Tensor::<bf16, [4]>::from([1.0, 2.0, 3.0, 6.0])
    println("{first(flags)} {total(&ints)} {s[1]} {filled::<f64>()[2]} {m[1, 0]}")
    println("{g[1]} {e[0]} {p.sum()} {avg(&h)}")
    0
}
"#;
    assert_eq!(
        stdout_of("tensors.nr", source),
        "false 12 6.0 1.0 4\n2.0 2.718281828459045 2 3.0\n"
    );
}

// One computation, `sum(t * t) + 1`, written six ways and run at an integer and a float
// instance. By hand: [1, 2, 3] gives 14 + 1 = 15, [0.5, 1.5, 2.0] gives 6.5 + 1 = 7.5.
#[test]
fn equivalent_forms_of_a_generic_reduction_agree() {
    let source = r#"
func tail<T: Numeric>(t: &Tensor<T, [3]>) -> T { (t * t).sum() + T::one() }
func bound<T: Numeric>(t: &Tensor<T, [3]>) -> T {
    val sq = t * t
    val s = sq.sum()
    return s + T::one()
}
func arm<T: Numeric>(t: &Tensor<T, [3]>, k: i32) -> T {
    match k {
        0 => T::zero(),
        _ => if k > 0 { (t * t).sum() + T::one() } else { T::zero() }
    }
}
func closure<T: Numeric>(t: &Tensor<T, [3]>) -> T {
    val add_one = |x: T| x + T::one()
    add_one((t * t).sum())
}
func plus_one<T: Numeric>(x: T) -> T { x + T::one() }
func argument<T: Numeric>(t: &Tensor<T, [3]>) -> T { plus_one((t * t).sum()) }
struct Holder<T> { t: Tensor<T, [3]> }
impl<T: Numeric> Holder<T> {
    func method(&self) -> T { tail(&self.t) }
}
func all<T: Numeric>(t: Tensor<T, [3]>, want: T) -> bool {
    val forms = [tail(&t), bound(&t), arm(&t, 1), closure(&t), argument(&t)]
    val h = Holder { t: t }
    mut ok = h.method() == want
    for f in forms { ok = ok && f == want }
    ok
}
func main() -> i32 {
    val i = all(Tensor::<i32, [3]>::from([1, 2, 3]), 15)
    val f = all(Tensor::<f32, [3]>::from([0.5, 1.5, 2.0]), 7.5)
    println("{i} {f}")
    0
}
"#;
    assert_eq!(stdout_of("forms.nr", source), "true true\n");
}

#[test]
fn an_associated_function_is_reached_through_a_bound() {
    let source = r#"
trait Make { func make(x: i32) -> Self }
trait Read: Make { func read(&self) -> i32 }
@derive(Copy, Clone)
struct P { v: i32 }
impl Make for P { func make(x: i32) -> P { P { v: x + 1 } } }
impl Read for P { func read(&self) -> i32 { self.v } }
struct Holder<T> { x: T }
impl<T: Read> Holder<T> { func build() -> Holder<T> { Holder { x: T::make(9) } } }
func direct<T: Make>() -> T { T::make(20) }
func through_super<T: Read>() -> i32 { T::make(4).read() }
func main() -> i32 {
    val h: Holder<P> = Holder::build::<P>()
    println("{direct::<P>().v} {through_super::<P>()} {h.x.v} {P::make(0).v}")
    0
}
"#;
    assert_eq!(stdout_of("assoc.nr", source), "21 5 10 1\n");
}

#[test]
fn an_operation_the_bound_does_not_promise_is_refused() {
    let cases = [
        (
            "func f<T>(a: T, b: T) -> T { a + b }",
            "cannot apply binary operator +",
        ),
        (
            "func f<T: Numeric>(a: T, b: T) -> T { a % b }",
            "cannot apply binary operator %",
        ),
        (
            "func f<T: Numeric>(a: T) -> T { a + 1 }",
            "expected T, found i32",
        ),
        (
            "func f<T: Numeric>(a: T) -> T { a * 2.0 }",
            "expected T, found f64",
        ),
        (
            "func f<T: Integer>(a: T) -> T { a + 200 }",
            "integer literal 200 out of range for type T",
        ),
        (
            "func f<T: Float>(a: T) -> T { a * 70000.0 }",
            "float literal out of range for type T",
        ),
        (
            "func f<T: Numeric>(x: T) -> T { x.exp() }",
            "type 'T' has no method 'exp'",
        ),
        (
            "func f<T: Float>(x: T) -> T { x.wrapping_add(x) }",
            "type 'T' has no method 'wrapping_add'",
        ),
        (
            "func f<T>() -> T { T::zero() }",
            "'T' has no associated function 'zero'",
        ),
        (
            "func f<T>(t: Tensor<T, [2]>) -> T { t.sum() }",
            "`.sum()` reduces a tensor's elements",
        ),
        (
            "func f<T: Numeric>(t: &Tensor<T, [2]>) -> T { t.mean() }",
            "`.mean()` averages T elements",
        ),
    ];
    for (body, expected) in cases {
        let source = format!("{body}\nfunc main() -> i32 {{ 0 }}\n");
        assert_check_error("refused.nr", &source, expected);
    }
}

#[test]
fn a_type_argument_outside_the_family_is_refused() {
    assert_check_error(
        "bool.nr",
        "func f<T: Float>(a: T) -> T { a }\nfunc main() -> i32 { f(true)\n 0 }\n",
        "type argument `bool` for 'T' does not implement required trait 'Float'",
    );
    assert_check_error(
        "string.nr",
        "func f<T: Numeric>(a: T) -> T { a }\nfunc main() -> i32 { val s = f(\"x\")\n 0 }\n",
        "does not implement required trait 'Numeric'",
    );
    assert_check_error(
        "impl.nr",
        "struct S { v: i32 }\nimpl Numeric for S { func zero() -> S { S { v: 0 } } func one() -> S { S { v: 1 } } }\nfunc main() -> i32 { 0 }\n",
        "`Numeric` is implemented by the compiler for the numeric scalars only",
    );
}

#[test]
fn a_non_scalar_tensor_element_is_refused_per_instance() {
    assert_check_error(
        "signature.nr",
        "func mk<T>(x: T) -> Tensor<T, [1]> { Tensor::<T, [1]>::from([x]) }\nfunc main() -> i32 { val t = mk(\"hi\")\n 0 }\n",
        "tensor element type string is not a numeric scalar",
    );
    let text = compile_rejected(
        "body.nr",
        "func mk<T>(x: T) -> T { val t = Tensor::<T, [1]>::from([x])\n t[0] }\nfunc main() -> i32 { val t = mk(\"hi\")\n 0 }\n",
    );
    assert!(
        text.contains("this instance makes it `string`"),
        "unexpected diagnostic: {text}"
    );
}

// A trait whose function takes no receiver is not object-safe, so it gets no vtable; the
// backend used to build one and emit a call with a receiver the function does not take.
#[test]
fn a_trait_with_an_associated_function_compiles() {
    let source = r#"
trait Make { func make(x: i32) -> Self }
struct P { v: i32 }
impl Make for P { func make(x: i32) -> P { P { v: x * 2 } } }
func main() -> i32 {
    println("{P::make(21).v}")
    0
}
"#;
    assert_eq!(stdout_of("receiverless.nr", source), "42\n");
}

// `.to_checked`, `.parse` and `std::testing::is_close` take a type parameter where a bound
// says what it is.
#[test]
fn conversions_and_closeness_take_a_bounded_parameter() {
    let source = r#"
import std::testing::{is_close}
func narrow<T: Integer>(x: f64) -> Option<T> { x.to_checked::<T>() }
func widen<F: Float, I: Integer>(x: F) -> Option<I> { x.to_checked::<I>() }
func read<T: Numeric>(s: &string) -> Result<T, ParseError> { s.parse::<T>() }
func main() -> i32 {
    val p = match narrow::<u8>(300.0) { Option::Some(_) => 1, Option::None => 0 }
    val q = match narrow::<i16>(-12.7) { Option::Some(v) => v as i32, Option::None => 0 }
    val r = match widen::<f32, u8>(7.9f32) { Option::Some(v) => v as i32, Option::None => 0 }
    val t = match read::<f32>("2.5") { Result::Ok(v) => v, Result::Err(_) => 0.0f32 }
    val u = match read::<u8>("256") { Result::Ok(_) => 1, Result::Err(_) => 0 }
    val near = is_close(0.1f32 + 0.2f32, 0.3f32, 1e-6f32)
    val far = is_close(1.0f16, 1.01f16, 0.001f16)
    println("{p} {q} {r} {t} {u} {near} {far}")
    0
}
"#;
    assert_eq!(
        stdout_of("conversions.nr", source),
        "0 -12 7 2.5 0 true false\n"
    );
}
