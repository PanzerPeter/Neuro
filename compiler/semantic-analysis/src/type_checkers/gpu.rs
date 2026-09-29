// `@gpu` form rules.
//
// Bare `@gpu` pins a free function's body to a GPU kernel. Whether a body can become one
// is the GPU backend's call, made per body when the program is compiled; what is checked
// here is only the attribute's shape and the places it may not go.

use ast_types::{Attribute, Item};

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
        } else if attr
            .named
            .iter()
            .any(|arg| arg.label.name == FALLBACK_LABEL)
        {
            "`fallback:` is not supported yet; bare `@gpu` runs on a GPU or aborts"
        } else if !attr.args.is_empty() || !attr.named.is_empty() {
            "takes no arguments"
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
