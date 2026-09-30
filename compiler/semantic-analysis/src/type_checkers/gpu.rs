// `@gpu` form rules.
//
// `@gpu` pins a free function's body to a GPU kernel; `@gpu(fallback: true)` lets the program
// run a host copy of it where no GPU is usable. Whether a body can become a kernel is the GPU
// backend's call, made per body when the program is compiled; what is checked here is only
// the attribute's shape and the places it may not go.

use ast_types::{Attribute, Expr, Item};
use shared_types::Literal;

use crate::errors::TypeError;

use super::TypeChecker;

const GPU_ATTRIBUTE: &str = "gpu";

const FALLBACK_LABEL: &str = "fallback";

/// `@grad` and `@gpu` on one function would give it a derivative, and the derivative
/// transform only ever emits host code.
const GRAD_ATTRIBUTE: &str = "grad";

impl TypeChecker {
    pub(crate) fn check_gpu_attributes(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(func) => self.check_gpu(&func.attributes, false),
                Item::Impl(def) => {
                    for method in &def.methods {
                        self.check_gpu(&method.attributes, true);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_gpu(&mut self, attributes: &[Attribute], on_method: bool) {
        let Some(attr) = attributes
            .iter()
            .find(|attr| attr.name.name == GPU_ATTRIBUTE)
        else {
            return;
        };
        let problem = if on_method {
            "on a method is not supported yet; move the work into a free `@gpu` function"
        } else if let Some(problem) = arguments_problem(attr) {
            problem
        } else if attributes
            .iter()
            .any(|attr| attr.name.name == GRAD_ATTRIBUTE)
        {
            "cannot share a function with `@grad` yet: its derivative would run on the host"
        } else {
            return;
        };
        self.record_error(TypeError::GpuForm {
            problem: problem.to_string(),
            span: attr.span,
        });
    }
}

/// The one argument `@gpu` takes is `fallback:`, once, as a `bool` literal: the backend
/// decides at compile time whether to emit a host copy, so the value cannot wait for run time.
fn arguments_problem(attr: &Attribute) -> Option<&'static str> {
    if !attr.args.is_empty()
        || attr
            .named
            .iter()
            .any(|arg| arg.label.name != FALLBACK_LABEL)
    {
        return Some("takes only `fallback: true` or `fallback: false`");
    }
    match attr.named.as_slice() {
        [] => None,
        [arg] if matches!(arg.value, Expr::Literal(Literal::Boolean(_), _)) => None,
        [_] => Some("`fallback:` takes the literal `true` or `false`"),
        _ => Some("`fallback:` is given more than once"),
    }
}
