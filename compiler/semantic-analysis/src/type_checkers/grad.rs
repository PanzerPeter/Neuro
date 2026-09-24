// `@grad` signature rules.
//
// A `@grad` function lowers to a pure derivative sibling, `__f__rev`, returning the loss
// and a generated `GradsOf_f` bundle with one owned gradient per differentiated
// parameter. What that pair looks like is fixed entirely by the signature, so the
// signature is what is checked here; which operations the body may use is the transform's
// own rule set, and the transform reports a construct it cannot differentiate itself.
//
// Without `wrt:` (a later item) every tensor parameter is differentiated, so every tensor
// parameter is held to the differentiated-parameter rule: borrowed `&mut`, because the
// materialization layer writes each one's gradient slot after the call returns.

use ast_types::{Attribute, FunctionDef, Item};

use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};

use super::TypeChecker;

/// The attribute that asks for a derivative.
const GRAD_ATTRIBUTE: &str = "grad";

/// The generated bundle's name prefix. Duplicated in `hir-lowering`, which emits the
/// struct; the two slices must agree, and a clash is caught here, where it has a span.
const BUNDLE_PREFIX: &str = "GradsOf_";

fn grad_attribute(attributes: &[Attribute]) -> Option<&Attribute> {
    attributes
        .iter()
        .find(|attr| attr.name.name == GRAD_ATTRIBUTE)
}

/// Whether `ty` is the rank-0 `f32` tensor a differentiated function returns. The reverse
/// pass seeds with `1.0` at that type, which is why no seed parameter exists.
fn is_scalar_loss(ty: &Type) -> bool {
    matches!(ty, Type::Tensor { element, shape } if **element == Type::F32 && shape.is_empty())
}

/// Why a tensor parameter cannot be differentiated, or `None` when it can.
///
/// A shape parameter is an extent every instance fixes, so a generic function's template
/// may name one; only a dynamic `?` axis has no static shape to give a gradient buffer.
fn differentiated_param_problem(ty: &Type, generic: bool) -> Option<&'static str> {
    let Type::Reference {
        inner,
        mutable: true,
    } = ty
    else {
        return Some("must be borrowed `&mut`, because every tensor parameter is differentiated and its gradient is written after the call returns");
    };
    let Type::Tensor { element, shape } = &**inner else {
        return None;
    };
    if !element.is_float() {
        return Some(
            "has an element type other than `f32` or `f64`, the two that have a derivative",
        );
    }
    if shape.iter().any(|axis| match axis.extent {
        ArrayLen::Fixed(_) => false,
        ArrayLen::Param(_) => !generic,
        _ => true,
    }) {
        return Some("has an extent that is not a literal; a gradient buffer is shaped like its parameter and needs a static shape");
    }
    None
}

impl TypeChecker {
    /// Check every `@grad` annotation against the forms the derivative transform accepts.
    ///
    /// Runs after signature registration, so a function's parameter and return types are
    /// already resolved and every generated name can be tested against the whole program.
    pub(crate) fn check_grad_attributes(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(func) => self.check_grad_function(func),
                Item::Impl(def) => {
                    for method in &def.methods {
                        if let Some(attr) = grad_attribute(&method.attributes) {
                            self.record_error(TypeError::GradFormUnsupported {
                                form: "on a method".to_string(),
                                span: attr.span,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_grad_function(&mut self, func: &FunctionDef) {
        let Some(attr) = grad_attribute(&func.attributes) else {
            return;
        };
        if !attr.args.is_empty() {
            self.record_error(TypeError::GradFormUnsupported {
                form: "with arguments".to_string(),
                span: attr.span,
            });
            return;
        }
        // A generic template is checked once, with its parameters abstract; the
        // derivative itself is derived per instance, where every extent is concrete.
        let generic = !func.generics.is_empty();
        // A signature that failed to register was already reported.
        let (params, ret) = if generic {
            let Some(sig) = self.generic_funcs.get(&func.name.name).cloned() else {
                return;
            };
            (sig.params, sig.ret)
        } else {
            let Some(Type::Function { params, ret }) = self.functions.get(&func.name.name).cloned()
            else {
                return;
            };
            (params, *ret)
        };
        let name = &func.name.name;

        if !is_scalar_loss(&ret) {
            self.record_error(TypeError::GradSignature {
                function: name.clone(),
                problem: format!("returns '{ret}'; a differentiated function returns its loss as a rank-0 `Tensor<f32, []>`"),
                span: func
                    .return_type
                    .as_ref()
                    .map_or(func.name.span, |ty| ty.span()),
            });
        }

        let mut differentiated = 0usize;
        for (param, ty) in func.params.iter().zip(params.iter()) {
            if !matches!(ty.referent(), Type::Tensor { .. }) {
                continue;
            }
            differentiated += 1;
            if let Some(problem) = differentiated_param_problem(ty, generic) {
                self.record_error(TypeError::GradSignature {
                    function: name.clone(),
                    problem: format!("takes '{}' as '{ty}', which {problem}", param.name.name),
                    span: param.name.span,
                });
            }
        }
        if differentiated == 0 {
            self.record_error(TypeError::GradSignature {
                function: name.clone(),
                problem: "has no tensor parameter to differentiate with respect to".to_string(),
                span: func.name.span,
            });
        }

        // `__f__rev` needs no such test: declared names may not contain `__` at all.
        let bundle = format!("{BUNDLE_PREFIX}{name}");
        if self.struct_defs.contains_key(&bundle) || self.generic_structs.contains_key(&bundle) {
            self.record_error(TypeError::GradGeneratedNameTaken {
                function: name.clone(),
                generated: bundle,
                span: func.name.span,
            });
        }
    }
}
