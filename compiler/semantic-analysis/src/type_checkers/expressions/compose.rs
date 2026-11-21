//! The function values `>>` and `|>` introduce: an instantiated generic named as a value,
//! `convert::<u8>`, and the parameter of a composition.
//!
//! A composition `f >> g` is parsed to the closure `|p| g(f(p))`, so it is checked as
//! one. What only the composition knows is where its parameter's type comes from when no
//! context gives it one: the parameter of the first stage `p` is passed to.

use std::collections::HashMap;

use ast_types::{COMPOSE_PARAM, Expr, GenericArg};
use shared_types::{Identifier, Span};

use super::TypeChecker;
use super::calls::BoundMethod;
use crate::errors::TypeError;
use crate::type_checkers::GenericFnSig;
use crate::types::{FunctionMode, Type};

impl TypeChecker {
    /// The type of `callee::<type_args>` named as a value: the generic function's, or
    /// generic associated function's, signature with every type argument substituted. A
    /// method is a value only as a bound method, an operand of `|>` or `>>`.
    pub(crate) fn check_instance(
        &mut self,
        callee: &Expr,
        type_args: &[GenericArg],
        span: Span,
    ) -> Option<Type> {
        let (name, sig) = match callee {
            Expr::Identifier(ident) => (
                ident.name.clone(),
                self.generic_funcs
                    .get(&ident.name)
                    .filter(|_| self.symbols.lookup(&ident.name).is_none())
                    .cloned(),
            ),
            Expr::Path {
                type_name, member, ..
            } if !self.generic_scope.contains(&type_name.name) => {
                let mangled = format!("{}__{}", type_name.name, member.name);
                let Some(Type::Function { params, ret, .. }) =
                    self.functions.get(&mangled).cloned()
                else {
                    // Not an associated function: the path reports what it is.
                    return self.check_expr(callee, None);
                };
                let sig = self.assoc_generic_signature(&type_name.name, &mangled, params, *ret);
                (format!("{}::{}", type_name.name, member.name), sig)
            }
            _ => {
                self.record_error(TypeError::InstanceNotAValue { span });
                return None;
            }
        };
        let Some(sig) = sig else {
            // A turbofish on a non-generic function names nothing, as on a call.
            self.record_error(TypeError::TurbofishCountMismatch {
                name,
                expected: 0,
                found: type_args.len(),
                span,
            });
            return self.check_expr(callee, None);
        };
        self.instantiate_signature(&name, &sig, type_args, span)
    }

    /// `sig` as a function type, every type argument taken from the turbofish `type_args`,
    /// which must name them all.
    fn instantiate_signature(
        &mut self,
        name: &str,
        sig: &GenericFnSig,
        type_args: &[GenericArg],
        span: Span,
    ) -> Option<Type> {
        let mut subst = HashMap::new();
        self.seed_turbofish(
            name,
            &sig.param_names,
            &sig.const_types,
            type_args,
            &mut subst,
            span,
        );
        // A turbofish that left a parameter unbound was reported by the seeding.
        if subst.len() != sig.param_names.len() {
            return None;
        }
        self.check_trait_bounds(&sig.bounds, &subst, span);
        self.check_where_predicates(&sig.where_predicates, &subst);
        let params = sig
            .params
            .iter()
            .map(|param| self.substitute_generic(param, &subst, span))
            .collect();
        let ret = self.substitute_generic(&sig.ret, &subst, span);
        Some(Type::Function {
            params,
            ret: Box::new(ret),
            mode: FunctionMode::Shared,
        })
    }

    /// The parameter type of the composition whose body is `body`: the first parameter of
    /// the stage the composition's argument goes to. `None` when that stage fixes none.
    /// Where the call in the body will say why (an undefined name, an unannotated closure),
    /// nothing is reported here; a generic stage without a turbofish, or a bound method
    /// this cannot resolve, is reported, because its call would infer from nothing.
    pub(crate) fn compose_param(&mut self, body: &Expr, span: Span) -> Option<Type> {
        let (stage, type_args) = first_stage(body)?;
        let signature = match stage {
            Expr::Closure { params, .. } => {
                let written = params.first()?.ty.as_ref()?;
                return self.resolve_type(written);
            }
            // Every error the instance has is the call's to report.
            Expr::Identifier(_) | Expr::Path { .. } if !type_args.is_empty() => {
                self.quietly(|checker| checker.check_instance(stage, type_args, span))
            }
            Expr::Identifier(ident) => match self.symbols.lookup(&ident.name) {
                Some(info) => Some(info.ty.clone()),
                None if self.generic_funcs.contains_key(&ident.name) => {
                    return self.untyped_composition(span);
                }
                None => self.functions.get(&ident.name).cloned(),
            },
            Expr::Path {
                type_name, member, ..
            } => {
                let key = format!("{}__{}", type_name.name, member.name);
                if self.generic_methods.contains_key(&key)
                    || self.instance_parts(&type_name.name).is_some()
                {
                    return self.untyped_composition(span);
                }
                self.functions.get(&key).cloned()
            }
            Expr::FieldAccess { object, field, .. } => {
                match self.bound_method_signature(object, field, type_args) {
                    Some(signature) => Some(signature),
                    None => return self.untyped_composition(span),
                }
            }
            _ => None,
        };
        match signature? {
            Type::Function { params, .. } => params.into_iter().next(),
            _ => None,
        }
    }

    /// The signature of `object.field` as a function of its arguments alone, read where
    /// the call reads it: the receiver's own `impl`, a trait object's trait, or a type
    /// parameter's bounds. A generic method is named at the instance its turbofish gives
    /// in full. A builtin method has no signature to read and is left to the call.
    fn bound_method_signature(
        &mut self,
        object: &Expr,
        field: &Identifier,
        type_args: &[GenericArg],
    ) -> Option<Type> {
        let receiver = self.quietly(|checker| checker.check_expr(object, None))?;
        let owner = match receiver.auto_deref().referent() {
            Type::Struct(name) | Type::Enum(name) | Type::Newtype(name) => name.clone(),
            Type::DynObject(trait_name) => {
                // An object-safe trait declares no generic method, so no turbofish applies.
                let sig = self.trait_member(trait_name, &field.name)?;
                return type_args.is_empty().then(|| Type::Function {
                    params: sig.params.clone(),
                    ret: Box::new(sig.ret.clone()),
                    mode: FunctionMode::Shared,
                });
            }
            Type::Generic(param) => {
                let param = param.clone();
                let method = self.quietly(|checker| {
                    checker.resolve_generic_trait_method(&param, &field.name, field.span)
                })?;
                return match method {
                    BoundMethod::Plain(params, ret) => {
                        type_args.is_empty().then_some(Type::Function {
                            params,
                            ret: Box::new(ret),
                            mode: FunctionMode::Shared,
                        })
                    }
                    BoundMethod::Generic(sig, _) if !type_args.is_empty() => {
                        self.quietly(|checker| {
                            checker.instantiate_signature(&field.name, &sig, type_args, field.span)
                        })
                    }
                    BoundMethod::Generic(..) => None,
                };
            }
            _ => return None,
        };
        let key = self.impl_methods.get(&owner)?.get(&field.name)?.clone();
        let Type::Function { params, ret, mode } = self.functions.get(&key)?.clone() else {
            return None;
        };
        let params: Vec<Type> = params.into_iter().skip(1).collect();
        let Some(own) = self.generic_methods.get(&key).cloned() else {
            return type_args
                .is_empty()
                .then_some(Type::Function { params, ret, mode });
        };
        if type_args.is_empty() {
            return None;
        }
        let sig = own.signature(params, *ret);
        self.quietly(|checker| {
            checker.instantiate_signature(&field.name, &sig, type_args, field.span)
        })
    }

    fn untyped_composition(&mut self, span: Span) -> Option<Type> {
        self.record_error(TypeError::ComposeParamUntyped { span });
        None
    }

    /// Run `read` for its result alone: the call in the composition's body checks the
    /// same thing again and reports it there.
    fn quietly<T>(&mut self, read: impl FnOnce(&mut Self) -> T) -> T {
        let reported = self.errors.len();
        let result = read(self);
        self.errors.truncate(reported);
        result
    }
}

/// The stage a composition's parameter is first passed to, with its turbofish: the
/// innermost call of the chain the parser built, the one whose argument is the parameter.
fn first_stage(body: &Expr) -> Option<(&Expr, &[GenericArg])> {
    let Expr::Call {
        func,
        type_args,
        args,
        ..
    } = body
    else {
        return None;
    };
    match args.as_slice() {
        [Expr::Identifier(param)] if param.name == COMPOSE_PARAM => Some((func, type_args)),
        [inner] => first_stage(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::errors::TypeError;
    use crate::type_checkers::tests::semantic_errors;

    const PRELUDE: &str = "struct P { x: i32 }
impl P {
    func scale(v: i32) -> i32 { v * 3 }
    func get(&self, d: i32) -> i32 { self.x + d }
}
func identity<T>(v: T) -> T { v }
func double(v: i32) -> i32 { v * 2 }
";

    fn errors_of(body: &str) -> Vec<TypeError> {
        semantic_errors(&format!("{PRELUDE}func main() -> i32 {{\n{body}\n}}"))
    }

    /// Each spelling of the first stage types the composition's parameter, and a later
    /// generic stage takes its type argument from the stage before it.
    #[test]
    fn every_first_stage_spelling_types_the_parameter() {
        for first in [
            "double",
            "P::scale",
            "identity::<i32>",
            "(|x: i32| x)",
            "p.get",
            "h",
        ] {
            let errors = errors_of(&format!(
                "val p = P {{ x: 1 }}
val h = double
val g = {first} >> identity >> (|y| y + 1)
g(2)"
            ));
            assert!(errors.is_empty(), "{first}: {errors:?}");
        }
    }

    #[test]
    fn a_generic_first_stage_needs_its_instance_named() {
        let errors = errors_of("val g = identity >> double\ng(2)");
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, TypeError::ComposeParamUntyped { .. })),
            "{errors:?}"
        );
        let typed = errors_of("val g: (i32) -> i32 = identity >> double\ng(2)");
        assert!(typed.is_empty(), "a context types the parameter: {typed:?}");
    }

    #[test]
    fn an_instance_is_a_function_value() {
        let errors = errors_of(
            "val f = identity::<i32>\nval s: (string) -> string = identity::<string>\nf(1)",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let wrong = errors_of("val f = identity::<i32>\nval s: string = f(1)\n0");
        assert!(
            wrong
                .iter()
                .any(|e| matches!(e, TypeError::Mismatch { .. })),
            "{wrong:?}"
        );
    }

    /// A closure literal called in place takes an unannotated parameter's type from its
    /// argument, which is what a closure pipeline target is.
    #[test]
    fn a_closure_called_in_place_is_typed_by_its_arguments() {
        let errors = errors_of("val a = 4 |> (|x| x * 2)\nval b = (|x, y: i64| y + 1)(a, 2)\na");
        assert!(errors.is_empty(), "{errors:?}");
    }
}
