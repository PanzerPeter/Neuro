// `@grad` signature rules.
//
// A `@grad` function lowers to a pure derivative sibling, `__f__rev`, returning the loss
// and a generated `GradsOf_f` bundle with one owned gradient per differentiated
// parameter. What that pair looks like is fixed entirely by the signature, so the
// signature is what is checked here; which operations the body may use is the transform's
// own rule set, and the transform reports a construct it cannot differentiate itself.
//
// `order: 2` asks for second derivatives as well (`.hessian()`), `order: 1` is the default
// spelled out, and no other order has an accessor. A method takes first derivatives only.
//
// `wrt: [...]` picks what is differentiated: a parameter by name, or
// a tensor reached from a method's receiver through exported fields and literal array
// positions, `self.encoder.w` or `self.heads[1]`. Without it every tensor parameter is
// differentiated and a method's receiver is a constant. Whatever is differentiated is held
// to one rule: `.backward()` writes its gradient slot after the call returns, so a
// parameter is borrowed `&mut`, and a path through the receiver needs `&mut self`.
// Everything else is a constant and may be passed however the function wants.

use ast_types::{
    Attribute, AttributeNamedArg, Expr, FunctionDef, ImplDef, Item, MethodDef, Parameter, SelfParam,
};
use shared_types::{Literal, Span};

use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};

use super::TypeChecker;

/// The attribute that asks for a derivative.
const GRAD_ATTRIBUTE: &str = "grad";

/// The generated bundle's name prefix. Duplicated in `hir-lowering`, which emits the
/// struct; the two slices must agree, and a clash is caught here, where it has a span.
const BUNDLE_PREFIX: &str = "GradsOf_";

/// The argument selecting what is differentiated.
const WRT_LABEL: &str = "wrt";

/// The argument selecting how many derivatives `.backward()` fills.
const ORDER_LABEL: &str = "order";

/// The orders with an accessor: `.grad()` reads the first, `.hessian()` the second.
const FIRST_ORDER: i128 = 1;
const SECOND_ORDER: i128 = 2;

/// The receiver's name, the root of every field path a method's `wrt:` may list.
const RECEIVER: &str = "self";

/// Which of a `@grad` function's arguments its `.backward()` writes a gradient slot through,
/// and so holds borrowed from the call to the `.backward()`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct GradSelection {
    /// One flag per declared parameter, the receiver excluded.
    pub(crate) params: Vec<bool>,
    /// Whether a `wrt:` path reaches a tensor through the receiver.
    pub(crate) receiver: bool,
}

/// A `@grad` attribute's arguments.
struct GradArguments<'a> {
    wrt: Option<&'a AttributeNamedArg>,
    /// The `order: 2` argument, when the attribute asks for second derivatives.
    second_order: Option<&'a AttributeNamedArg>,
}

/// The receiver a method's `wrt:` paths start from.
struct Receiver<'a> {
    type_name: &'a str,
    self_param: Option<&'a SelfParam>,
}

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

/// Why a differentiated parameter cannot be differentiated, or `None` when it can.
fn differentiated_param_problem(ty: &Type, generic: bool) -> Option<&'static str> {
    let Type::Reference {
        inner,
        mutable: true,
    } = ty
    else {
        return Some("must be borrowed `&mut`, because it is differentiated and its gradient is written after the call returns");
    };
    differentiated_tensor_problem(inner, generic)
}

/// Why a tensor cannot be differentiated, or `None` when it can.
///
/// A shape parameter is an extent every instance fixes, so a generic function's template
/// may name one; only a dynamic `?` axis has no static shape to give a gradient buffer.
fn differentiated_tensor_problem(ty: &Type, generic: bool) -> Option<&'static str> {
    let Type::Tensor { element, shape } = ty else {
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
        let Some(GradArguments { wrt, .. }) = self.grad_arguments(attr) else {
            return;
        };
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
        let signature = SignatureSite {
            name,
            name_span: func.name.span,
            return_span: func.return_type.as_ref().map(|ty| ty.span()),
            params: &func.params,
            generic,
        };
        // Recorded even when the signature is refused: the call sites then check against
        // the intent, and the refusal is reported once, here.
        let selection = self.check_grad_signature(&signature, &params, &ret, wrt, None);
        self.grad_functions.insert(name.clone(), selection);

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

    /// A `@grad` method. Its signature rules are a free function's, applied to the
    /// parameters after `self`, and its `wrt:` may also list tensors reached through the
    /// receiver. Its generated names carry the `Type__method` key, whose `__` no declared
    /// name can contain, so they cannot clash with anything the program declares.
    fn check_grad_method(&mut self, def: &ImplDef, method: &MethodDef) {
        let Some(attr) = grad_attribute(&method.attributes) else {
            return;
        };
        let Some(GradArguments { wrt, second_order }) = self.grad_arguments(attr) else {
            return;
        };
        // The second derivative is taken over the first derivative's body, a function of
        // the parameters alone; a method's `wrt:` fields would have to be threaded through
        // it as well.
        if let Some(order) = second_order {
            self.record_error(TypeError::GradFormUnsupported {
                form: "with `order: 2` on a method".to_string(),
                span: order.label.span,
            });
            return;
        }
        let form = if def.trait_name.is_some() {
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

        // A `wrt:` naming a field, or a `@model` receiver, writes gradient slots reachable
        // through the receiver after the call returns, and a receiver the call consumed
        // has nowhere to keep them. A constant receiver holds to the same form, so adding
        // a `wrt:` never changes which receivers are legal.
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
        let receiver = Receiver {
            type_name,
            self_param: method.self_param.as_ref(),
        };
        let selection = self.check_grad_signature(
            &signature,
            params.get(1..).unwrap_or_default(),
            &ret,
            wrt,
            Some(&receiver),
        );
        self.grad_functions.insert(key, selection);
    }

    /// The arguments of `attr`, or `None` when it takes one `@grad` does not, or an
    /// `order:` no accessor reads, which is reported here.
    fn grad_arguments<'a>(&mut self, attr: &'a Attribute) -> Option<GradArguments<'a>> {
        if let Some(arg) = attr.args.first() {
            self.record_error(TypeError::GradFormUnsupported {
                form: format!("with the bare argument '{}'", arg.name),
                span: arg.span,
            });
            return None;
        }
        let (mut wrt, mut order) = (None, None);
        for arg in &attr.named {
            let seen = match arg.label.name.as_str() {
                WRT_LABEL => &mut wrt,
                ORDER_LABEL => &mut order,
                other => {
                    self.record_error(TypeError::GradFormUnsupported {
                        form: format!("with `{other}:`"),
                        span: arg.label.span,
                    });
                    return None;
                }
            };
            if seen.is_some() {
                self.record_error(TypeError::GradFormUnsupported {
                    form: format!("with `{}:` given twice", arg.label.name),
                    span: arg.label.span,
                });
                return None;
            }
            *seen = Some(arg);
        }
        let second_order = match order {
            None => None,
            Some(arg) => match &arg.value {
                Expr::Literal(Literal::Integer(FIRST_ORDER, _), _) => None,
                Expr::Literal(Literal::Integer(SECOND_ORDER, _), _) => Some(arg),
                other => {
                    let found = match other {
                        Expr::Literal(Literal::Integer(value, _), _) => format!("order {value}"),
                        _ => "an order that is not an integer literal".to_string(),
                    };
                    self.record_error(TypeError::GradOrderUnsupported {
                        found,
                        span: other.span(),
                    });
                    return None;
                }
            },
        };
        Some(GradArguments { wrt, second_order })
    }

    /// The rules every `@grad` signature obeys, whatever declares it: the loss is a
    /// rank-0 `f32` tensor, and everything differentiated is a differentiable tensor that
    /// `.backward()` can write a gradient slot through. Returns what is differentiated.
    fn check_grad_signature(
        &mut self,
        signature: &SignatureSite<'_>,
        params: &[Type],
        ret: &Type,
        wrt: Option<&AttributeNamedArg>,
        receiver: Option<&Receiver<'_>>,
    ) -> GradSelection {
        let name = signature.name;
        if !is_scalar_loss(ret) {
            self.record_error(TypeError::GradSignature {
                function: name.to_string(),
                problem: format!("returns '{ret}'; a differentiated function returns its loss as a rank-0 `Tensor<f32, []>`"),
                span: signature.return_span.unwrap_or(signature.name_span),
            });
        }

        let selection = match wrt {
            None => GradSelection {
                params: params
                    .iter()
                    .map(|ty| matches!(ty.referent(), Type::Tensor { .. }))
                    .collect(),
                receiver: false,
            },
            Some(wrt) => match self.wrt_selection(signature, params, wrt, receiver) {
                Some(selection) => selection,
                None => return GradSelection::default(),
            },
        };

        for ((param, ty), _) in signature
            .params
            .iter()
            .zip(params.iter())
            .zip(&selection.params)
            .filter(|(_, selected)| **selected)
        {
            if let Some(problem) = differentiated_param_problem(ty, signature.generic) {
                self.record_error(TypeError::GradSignature {
                    function: name.to_string(),
                    problem: format!("takes '{}' as '{ty}', which {problem}", param.name.name),
                    span: param.name.span,
                });
            }
        }
        if wrt.is_none() && !selection.params.contains(&true) {
            self.record_error(TypeError::GradSignature {
                function: name.to_string(),
                problem: "has no tensor parameter to differentiate with respect to".to_string(),
                span: signature.name_span,
            });
        }
        selection
    }

    /// What a `wrt:` list selects, or `None` when it is not a list at all. Every entry it
    /// cannot select is reported with its own span.
    fn wrt_selection(
        &mut self,
        signature: &SignatureSite<'_>,
        params: &[Type],
        wrt: &AttributeNamedArg,
        receiver: Option<&Receiver<'_>>,
    ) -> Option<GradSelection> {
        let name = signature.name;
        let report = |this: &mut Self, problem: String, span: Span| {
            this.record_error(TypeError::GradSignature {
                function: name.to_string(),
                problem,
                span,
            });
        };
        let Expr::ArrayLiteral { elements, .. } = &wrt.value else {
            report(
                self,
                "gives `wrt:` something other than a list; write `wrt: [w, self.field]`"
                    .to_string(),
                wrt.value.span(),
            );
            return None;
        };
        if elements.is_empty() {
            report(
                self,
                "lists nothing in `wrt:`, so there is nothing to differentiate".to_string(),
                wrt.value.span(),
            );
            return None;
        }

        let mut selection = GradSelection {
            params: vec![false; params.len()],
            receiver: false,
        };
        let mut paths: Vec<String> = Vec::new();
        for entry in elements {
            let span = entry.span();
            if let Expr::Identifier(ident) = entry {
                if ident.name != RECEIVER {
                    let problem = self.select_param(signature, params, &ident.name, &mut selection);
                    if let Some(problem) = problem {
                        report(self, problem, span);
                    }
                    continue;
                }
            }
            let Some(path) = field_path_text(entry) else {
                report(self, "lists an entry in `wrt:` that is neither a parameter name nor a field path rooted at `self`".to_string(), span);
                continue;
            };
            let problem = match receiver {
                None => Some(format!("lists '{path}' in `wrt:`, but only a method's `wrt:` may name a field of `self`")),
                Some(receiver) => self.select_field(&path, entry, receiver, signature.generic),
            };
            if let Some(problem) = problem {
                report(self, problem, span);
            } else if paths.contains(&path) {
                report(self, format!("lists '{path}' in `wrt:` twice"), span);
            } else {
                paths.push(path);
                selection.receiver = true;
            }
        }
        Some(selection)
    }

    /// Select the parameter `name`, or say why it cannot be.
    fn select_param(
        &self,
        signature: &SignatureSite<'_>,
        params: &[Type],
        name: &str,
        selection: &mut GradSelection,
    ) -> Option<String> {
        let Some(position) = signature.params.iter().position(|p| p.name.name == name) else {
            return Some(format!(
                "lists '{name}' in `wrt:`, which is not one of its parameters"
            ));
        };
        if selection.params[position] {
            return Some(format!("lists '{name}' in `wrt:` twice"));
        }
        let ty = params.get(position)?;
        if !matches!(ty.referent(), Type::Tensor { .. }) {
            return Some(format!(
                "lists '{name}' in `wrt:`, which is a '{ty}'; only a tensor has a gradient"
            ));
        }
        selection.params[position] = true;
        None
    }

    /// Why the field path `entry` (rendered as `path`) cannot be differentiated through
    /// `receiver`, or `None` when it can.
    fn select_field(
        &self,
        path: &str,
        entry: &Expr,
        receiver: &Receiver<'_>,
        generic: bool,
    ) -> Option<String> {
        let leaf = match self.field_path_type(entry, receiver.type_name) {
            Ok(leaf) => leaf,
            Err(problem) => return Some(format!("lists '{path}' in `wrt:`, which {problem}")),
        };
        if !matches!(leaf, Type::Tensor { .. }) {
            return Some(format!(
                "lists '{path}' in `wrt:`, which is a '{leaf}'; only a tensor has a gradient"
            ));
        }
        if let Some(problem) = differentiated_tensor_problem(&leaf, generic) {
            return Some(format!("lists '{path}' in `wrt:`, which {problem}"));
        }
        if !matches!(receiver.self_param, Some(SelfParam::RefMut)) {
            return Some(format!("lists '{path}' in `wrt:`, reached through the receiver, so it must take `&mut self`: `.backward()` writes that field's gradient slot after the call returns"));
        }
        None
    }

    /// The type a `wrt:` field path reaches from a receiver of type `type_name`, or why it
    /// reaches nothing. A private field is refused wherever the path is written, because
    /// naming one would tie the annotation to a layout the type does not promise.
    fn field_path_type(&self, path: &Expr, type_name: &str) -> Result<Type, String> {
        match path {
            Expr::Identifier(ident) if ident.name == RECEIVER => {
                Ok(Type::Struct(type_name.to_string()))
            }
            Expr::FieldAccess { object, field, .. } => {
                let object = self.field_path_type(object, type_name)?;
                let Type::Struct(owner) = &object else {
                    return Err(format!(
                        "reads field '{}' of a '{object}', which has no fields",
                        field.name
                    ));
                };
                let Some((_, ty)) = self
                    .struct_defs
                    .get(owner)
                    .and_then(|fields| fields.iter().find(|(name, _)| *name == field.name))
                else {
                    return Err(format!(
                        "names a field '{}' that '{owner}' does not have",
                        field.name
                    ));
                };
                if self
                    .private_fields
                    .get(owner)
                    .is_some_and(|private| private.contains(&field.name))
                {
                    return Err(format!("reaches through the private field '{owner}.{}'; `wrt:` may name only exported fields", field.name));
                }
                Ok(ty.clone())
            }
            Expr::Index { object, index, .. } => {
                let object = self.field_path_type(object, type_name)?;
                let Type::Array {
                    element,
                    size: ArrayLen::Fixed(len),
                } = &object
                else {
                    return Err(format!(
                        "indexes a '{object}', which is not a fixed-length array"
                    ));
                };
                match literal_position(index) {
                    Some(position) if position < *len => Ok((**element).clone()),
                    Some(position) => {
                        Err(format!("reads position {position} of an array of {len}"))
                    }
                    None => Err(
                        "indexes an array at a position that is not an integer literal".to_string(),
                    ),
                }
            }
            _ => Err("is not a field path rooted at `self`".to_string()),
        }
    }
}

/// `entry` as a field path is written, `self.a.b[1]`, when it has that shape.
fn field_path_text(entry: &Expr) -> Option<String> {
    match entry {
        Expr::Identifier(ident) if ident.name == RECEIVER => Some(ident.name.clone()),
        Expr::FieldAccess { object, field, .. } => {
            Some(format!("{}.{}", field_path_text(object)?, field.name))
        }
        Expr::Index { object, index, .. } => Some(format!(
            "{}[{}]",
            field_path_text(object)?,
            literal_position(index)?
        )),
        _ => None,
    }
}

/// The value of an integer literal array position.
fn literal_position(index: &Expr) -> Option<usize> {
    match index {
        Expr::Literal(Literal::Integer(value, _), _) => usize::try_from(*value).ok(),
        _ => None,
    }
}
