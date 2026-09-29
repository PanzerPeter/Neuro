// `@gpu` form rules.

use super::semantic_errors;
use crate::errors::TypeError;

#[test]
fn a_bare_gpu_on_a_free_function_is_accepted() {
    let errors = semantic_errors(
        "@gpu\nfunc add(a: &Tensor<f32, [2]>, b: &Tensor<f32, [2]>) -> Tensor<f32, [2]> {\n    a + b\n}\n",
    );
    assert!(errors.is_empty(), "got {errors:?}");
}

#[test]
fn gpu_is_refused_with_arguments_beside_grad_and_on_a_method() {
    for (src, problem) in [
        (
            "@gpu(fallback: true)\nfunc id(x: f32) -> f32 {\n    x\n}\n",
            "`fallback:` is not supported yet",
        ),
        (
            "@gpu(fast)\nfunc id(x: f32) -> f32 {\n    x\n}\n",
            "takes no arguments",
        ),
        (
            "@gpu(threads: 4)\nfunc id(x: f32) -> f32 {\n    x\n}\n",
            "takes no arguments",
        ),
        (
            "@grad\n@gpu\nfunc loss(w: &mut Tensor<f32, [2]>) -> Tensor<f32, []> {\n    Tensor::scalar(w.sum())\n}\n",
            "cannot share a function with `@grad`",
        ),
        (
            "struct S { x: f32 }\nimpl S {\n    @gpu\n    func get(&self) -> f32 {\n        self.x\n    }\n}\n",
            "on a method",
        ),
    ] {
        let errors = semantic_errors(src);
        let [TypeError::GpuForm { problem: found, span }] = errors.as_slice() else {
            panic!("expected one GpuForm error for {src:?}, got {errors:?}");
        };
        assert!(found.starts_with(problem), "{found}");
        assert_eq!(span.start, src.find("@gpu").expect("attribute in source"));
    }
}
