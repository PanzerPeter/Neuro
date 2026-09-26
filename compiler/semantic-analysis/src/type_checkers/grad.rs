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
// materialization layer writes each one's gradient slot after the call returns. A method
// follows the same rules over the parameters after its receiver, which is a constant.

use ast_types::{Attribute, FunctionDef, ImplDef, Item, MethodDef, Parameter, SelfParam};
use shared_types::Span;

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

/// What a signature rule reports against: the name a diagnostic spells, where it points,
/// and the declared parameters, the receiver excluded.
struct SignatureSite<'a> {
    name: &'a str,
    name_span: Span,
    return_span: Option<Span>,
    params: &'a [Parameter],
    generic: bool,
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
                        self.check_grad_method(def, method);
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
        // Recorded even when the signature below is refused: the call sites then check
        // against the intent, and the refusal is reported once, here.
        self.grad_functions.insert(name.clone());
        let signature = SignatureSite {
            name,
            name_span: func.name.span,
            return_span: func.return_type.as_ref().map(|ty| ty.span()),
            params: &func.params,
            generic,
        };
        self.check_grad_signature(&signature, &params, &ret);

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

    /// A `@grad` method. Without `wrt:` every tensor parameter is differentiated and the
    /// receiver is a constant, so the signature rules are a free function's, applied to
    /// the parameters after `self`. Its generated names carry the `Type__method` key,
    /// whose `__` no declared name can contain, so they cannot clash with anything the
    /// program declares.
    fn check_grad_method(&mut self, def: &ImplDef, method: &MethodDef) {
        let Some(attr) = grad_attribute(&method.attributes) else {
            return;
        };
        let form = if !attr.args.is_empty() {
            Some("with arguments")
        } else if def.trait_name.is_some() {
            Some("on a method of a trait `impl`")
        } else if !def.generics.is_empty() || !def.type_args.is_empty() {
            Some("on a method of a generic `impl`")
        } else if method.self_param.is_none() {
            Some("on an associated function")
        } else {
            None
        };
        if let Some(form) = form {
            self.record_error(TypeError::GradFormUnsupported {
                form: form.to_string(),
                span: attr.span,
            });
            return;
        }
        let type_name = &def.type_name.name;
        let name = format!("{type_name}.{}", method.name.name);
        // A signature that failed to register was already reported.
        let Some(key) = self
            .impl_methods
            .get(type_name)
            .and_then(|methods| methods.get(&method.name.name))
            .cloned()
        else {
            return;
        };
        let Some(Type::Function { params, ret }) = self.functions.get(&key).cloned() else {
            return;
        };
        self.grad_functions.insert(key);

        // A `wrt:` naming a field, or a `@model` receiver, writes gradient slots reachable
        // through the receiver after the call returns, and a receiver the call consumed
        // has nowhere to keep them. A constant receiver holds to the same form, so adding
        // a `wrt:` later never changes which receivers are legal.
        if matches!(method.self_param, Some(SelfParam::Owned)) {
            self.record_error(TypeError::GradSignature {
                function: name.clone(),
                problem: "takes `self` by value; a `@grad` method borrows its receiver, as `&self` or `&mut self`".to_string(),
                span: method.name.span,
            });
        }
        // The registered signature carries the receiver as its first parameter.
        let signature = SignatureSite {
            name: &name,
            name_span: method.name.span,
            return_span: method.return_type.as_ref().map(|ty| ty.span()),
            params: &method.params,
            generic: false,
        };
        self.check_grad_signature(&signature, params.get(1..).unwrap_or_default(), &ret);
    }

    /// The rules every `@grad` signature obeys, whatever declares it: the loss is a
    /// rank-0 `f32` tensor, and every tensor parameter is a differentiable `&mut` borrow.
    fn check_grad_signature(&mut self, signature: &SignatureSite<'_>, params: &[Type], ret: &Type) {
        let name = signature.name;
        if !is_scalar_loss(ret) {
            self.record_error(TypeError::GradSignature {
                function: name.to_string(),
                problem: format!("returns '{ret}'; a differentiated function returns its loss as a rank-0 `Tensor<f32, []>`"),
                span: signature.return_span.unwrap_or(signature.name_span),
            });
        }

        let mut differentiated = 0usize;
        for (param, ty) in signature.params.iter().zip(params.iter()) {
            if !matches!(ty.referent(), Type::Tensor { .. }) {
                continue;
            }
            differentiated += 1;
            if let Some(problem) = differentiated_param_problem(ty, signature.generic) {
                self.record_error(TypeError::GradSignature {
                    function: name.to_string(),
                    problem: format!("takes '{}' as '{ty}', which {problem}", param.name.name),
                    span: param.name.span,
                });
            }
        }
        if differentiated == 0 {
            self.record_error(TypeError::GradSignature {
                function: name.to_string(),
                problem: "has no tensor parameter to differentiate with respect to".to_string(),
                span: signature.name_span,
            });
        }
    }
}
