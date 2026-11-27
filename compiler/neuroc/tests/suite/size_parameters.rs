// End-to-end tests for size parameters declared by bare name.
//
// A bare generic name used as an array length or a tensor extent is a size parameter, a
// `u64` read like any value in the body. A struct or enum decides its own from its fields,
// and an `impl` header from the type it passes the name to. A `const NAME: T` parameter is
// a value that sizes nothing, so writing one in a size position is an error.
use crate::compile_harness::{assert_check_error, run, run_program};

#[test]
fn a_struct_sized_by_a_bare_name_takes_a_generic_impl() {
    let source = r#"
struct Ring<T, CAP> {
    data: [T; CAP],
    count: u64,
}

impl<T: Copy, CAP> Ring<T, CAP> {
    func spare(&self) -> u64 {
        CAP - self.count
    }

    func newest(&self) -> T {
        self.data[self.count - 1]
    }
}

func main() -> i32 {
    val small: Ring<i32, 4> = Ring { data: [5, 6, 7, 8], count: 1 }
    val large = Ring { data: [1, 2, 3, 4, 5, 6, 7], count: 3 }
    return (small.spare() * 10 + large.spare()) as i32 + small.newest() + large.newest()
}
"#;
    // 3 * 10 + 4 + 5 + 3
    assert_eq!(run("ring.nr", source), 42);
}

#[test]
fn a_struct_and_an_enum_sized_by_tensor_extents() {
    let source = r#"
struct Dense<IN, OUT> {
    w: Tensor<f32, [IN, OUT]>,
    b: Tensor<f32, [OUT]>,
}

impl<IN, OUT> Dense<IN, OUT> {
    func parameters(&self) -> u64 {
        IN * OUT + OUT
    }

    func last_bias(&self) -> f32 {
        self.b[OUT - 1]
    }
}

enum Slot<N> {
    Empty,
    Full(Tensor<i32, [N]>),
}

impl<N> Slot<N> {
    func last(self) -> i32 {
        match self {
            Slot::Full(t) => t[N - 1],
            Slot::Empty => -1,
        }
    }
}

func main() -> i32 {
    val d: Dense<3, 2> = Dense { w: Tensor::zeros(), b: Tensor::ones() }
    val full: Slot<3> = Slot::Full([4, 5, 6])
    val none: Slot<2> = Slot::Empty
    return d.parameters() as i32 + d.last_bias() as i32 + full.last() + none.last()
}
"#;
    // 8 + 1 + 6 - 1
    assert_eq!(run("dense.nr", source), 14);
}

#[test]
fn an_array_length_makes_a_function_parameter_a_size() {
    let source = r#"
func length<T, N>(a: [T; N]) -> u64 {
    N
}

func main() -> i32 {
    val a: [i32; 3] = [1, 2, 3]
    val b: [bool; 5] = [true, false, true, false, true]
    return (length(a) * 10 + length(b)) as i32
}
"#;
    assert_eq!(run("length.nr", source), 35);
}

#[test]
fn an_extent_reads_as_u64() {
    let source = r#"
func width<N>(t: &Tensor<i32, [N]>) -> u64 {
    val n: u64 = N
    n
}

func main() -> i32 {
    val t: Tensor<i32, [6]> = Tensor::zeros()
    return width(&t) as i32
}
"#;
    assert_eq!(run("u64.nr", source), 6);
    assert_check_error(
        "u32.nr",
        "func width<N>(t: &Tensor<i32, [N]>) -> u32 { N }\nfunc main() -> i32 { 0 }\n",
        "expected u32, found u64",
    );
}

#[test]
fn an_impl_of_a_sized_type_from_another_module() {
    let files = [
        (
            "main.nr",
            "import ./ring::{Ring}\n\
             trait Room { func room(&self) -> u64 }\n\
             impl<CAP> Room for Ring<CAP> { func room(&self) -> u64 { CAP - self.used } }\n\
             func main() -> i32 {\n    \
                 val r: Ring<5> = Ring { data: [0, 0, 0, 0, 0], used: 2 }\n    \
                 return r.room() as i32\n\
             }\n",
        ),
        (
            "ring.nr",
            "export struct Ring<CAP> {\n    export data: [i32; CAP],\n    export used: u64,\n}\n",
        ),
    ];
    assert_eq!(run_program(&files, "main.nr"), Ok(3));
}

#[test]
fn a_const_parameter_keeps_sizing_nothing() {
    let source = r#"
func scaled<N, const K: u32>(a: [i32; N]) -> u64 {
    N * (K as u64)
}

func main() -> i32 {
    return scaled::<4, 3>([1, 2, 3, 4]) as i32
}
"#;
    assert_eq!(run("scaled.nr", source), 12);
}

#[test]
fn a_const_parameter_in_a_size_position_is_refused_with_the_bare_form() {
    for (name, source, what) in [
        (
            "array.nr",
            "func f<const N: u32>(a: [i32; N]) -> i32 { 0 }",
            "'N' sizes an array",
        ),
        (
            "tensor.nr",
            "func f<const N: u64>(t: &Tensor<f32, [N]>) -> i32 { 0 }",
            "'N' sizes a tensor axis",
        ),
        (
            "body.nr",
            "func f<const N: u64>() -> f32 {\n    val t = Tensor::<f32, [N]>::zeros()\n    t.sum()\n}",
            "'N' sizes a tensor axis",
        ),
        (
            "field.nr",
            "struct B<const CAP: u32> { d: [i32; CAP] }",
            "'CAP' sizes an array",
        ),
        (
            "impl.nr",
            "struct B<CAP> { d: [i32; CAP] }\nimpl<const CAP: u64> B<CAP> { func c(&self) -> u64 { CAP } }",
            "'CAP' sizes the type's extent",
        ),
    ] {
        let source = format!("{source}\nfunc main() -> i32 {{ 0 }}\n");
        assert_check_error(name, &source, what);
        let hint = if name == "impl.nr" || name == "field.nr" {
            "`<CAP>`"
        } else {
            "`<N>`"
        };
        assert_check_error(name, &source, hint);
    }
}

/// BUG-171: a size, shape or `const` parameter is a value and an extent at once, so a
/// binding of its name would split `N` from `[T; N]` in one body. Every binding form is
/// refused, naming the parameter.
#[test]
fn a_binding_named_like_a_size_or_const_parameter_is_refused() {
    for (name, source, param) in [
        (
            "val.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    val N = 100u64\n    N\n}",
            "N",
        ),
        (
            "const.nr",
            "func s<const N: u32>() -> u32 {\n    val N = 100u32\n    N\n}",
            "N",
        ),
        (
            "mut.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    mut N = 1u64\n    N\n}",
            "N",
        ),
        (
            "tuple.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    val (N, b) = (1u64, 2u64)\n    N + b\n}",
            "N",
        ),
        ("param.nr", "func s<const N: u32>(N: u32) -> u32 { N }", "N"),
        (
            "for.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    mut t = 0u64\n    for N in 0..3 { t += 1 }\n    t\n}",
            "N",
        ),
        (
            "closure.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    val f = |N: u64| -> u64 { N }\n    f(1)\n}",
            "N",
        ),
        (
            "match.nr",
            "func s<N>(a: [i32; N]) -> u64 {\n    match Some(1u64) {\n        Some(N) => N,\n        None => 0,\n    }\n}",
            "N",
        ),
        (
            "shape.nr",
            "func s<S>(t: &Tensor<f32, [2, ..S]>) -> f32 {\n    val S = 1.0\n    S\n}",
            "S",
        ),
        (
            "method.nr",
            "struct B<CAP> { d: [i32; CAP] }\nimpl<CAP> B<CAP> {\n    func c(&self) -> u64 {\n        val CAP = 9u64\n        CAP\n    }\n}",
            "CAP",
        ),
    ] {
        let source = format!("{source}\nfunc main() -> i32 {{ 0 }}\n");
        assert_check_error(
            name,
            &source,
            &format!("`{param}` is a generic parameter here and cannot also name a binding"),
        );
    }
}

#[test]
fn a_misspelled_size_is_not_a_new_parameter() {
    assert_check_error(
        "typo.nr",
        "struct B<CAP> { d: [i32; CAPP] }\nfunc main() -> i32 { 0 }\n",
        "array length 'CAPP' is not a known constant",
    );
}

/// One computation, `CAP - count` on a `Ring<4>` holding one element, written six ways.
/// By hand: 4 - 1 = 3 each, 18 in all; the same at `Ring<9>` holding two gives 7 each,
/// 42 in all.
#[test]
fn equivalent_forms_of_a_size_read_agree() {
    let source = r#"
struct Ring<CAP> {
    data: [i32; CAP],
    count: u64,
}

func minus(a: u64, b: u64) -> u64 {
    a - b
}

func apply<T>(f: (T) -> T, x: T) -> T {
    f(x)
}

impl<CAP> Ring<CAP> {
    func tail(&self) -> u64 {
        CAP - self.count
    }

    func bound(&self) -> u64 {
        val left = CAP - self.count
        return left
    }

    func nested(&self) -> u64 {
        if self.count > 0 {
            match self.count {
                0 => 0,
                _ => CAP - self.count,
            }
        } else {
            CAP
        }
    }

    func closure(&self) -> u64 {
        val f = |used: u64| CAP - used
        f(self.count)
    }

    func argument(&self) -> u64 {
        minus(CAP, self.count)
    }

    func generic(&self) -> u64 {
        apply(|used: u64| CAP - used, self.count)
    }

    func all(&self) -> u64 {
        self.tail() + self.bound() + self.nested() + self.closure() + self.argument()
            + self.generic()
    }
}

func main() -> i32 {
    val small: Ring<4> = Ring { data: [0, 0, 0, 0], count: 1 }
    val large: Ring<9> = Ring { data: [0, 0, 0, 0, 0, 0, 0, 0, 0], count: 2 }
    println("{small.all()} {large.all()}")
    return (small.all() + large.all()) as i32
}
"#;
    assert_eq!(run("forms.nr", source), 60);
}
