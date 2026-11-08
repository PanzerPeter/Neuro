//! `.map(f)`, `.filter(p)` and `.enumerate()` called on an `Iterator` value.
//!
//! An iterator with no method of the name gets one of the prelude adapters `Map`,
//! `Filter` and `Enumerate`, built around it. The adapter is an ordinary struct value,
//! so it binds, passes, returns and drives a `for` head through its own `impl Iterator`.
//! The call consumes the receiver and stores the function, which the borrow rules see as
//! a method call holding its arguments.
//!
//! A `for` head keeps its own head forms of the three (`loop_adapters`): those fold into
//! the loop over any head shape, arrays and ranges included, and build no value.

use ast_types::Expr;
use shared_types::Span;

use super::TypeChecker;
use super::closures::closure_context;
use super::loop_adapters::{FILTER_RESULT, MAP_RESULT};
use crate::errors::TypeError;
use crate::types::Type;

const MAP_METHOD: &str = "map";
const FILTER_METHOD: &str = "filter";
const ENUMERATE_METHOD: &str = "enumerate";

/// The adapter `method` builds and how many type parameters it takes, or `None` when the
/// method is no adapter.
fn adapter_type(method: &str) -> Option<(&'static str, usize)> {
    match method {
        MAP_METHOD => Some(("Map", 3)),
        FILTER_METHOD => Some(("Filter", 2)),
        ENUMERATE_METHOD => Some(("Enumerate", 2)),
        _ => None,
    }
}

impl TypeChecker {
    /// Type `object.method(args)` as an adapter over the iterator `object`, or answer
    /// `None` when `method` is no adapter or `object` is no iterator, leaving the caller
    /// to report the missing method.
    ///
    /// The adapter is the generic struct the prelude declares. A program that declares its
    /// own `Map`, or its own `Option` or `Iterator` (which takes the prelude's adapters
    /// with it), has no adapter to build, and the call is an ordinary method lookup again.
    ///
    /// `obj_ty` is the receiver's type read through its borrows, as method dispatch reads
    /// it; a borrowed receiver is refused here, since the adapter takes ownership.
    pub(crate) fn check_iterator_adapter(
        &mut self,
        object: &Expr,
        obj_ty: &Type,
        method: &str,
        args: &[Expr],
        span: Span,
    ) -> Option<Type> {
        let (base, arity) = adapter_type(method)?;
        if self
            .generic_structs
            .get(base)
            .is_none_or(|def| def.generics.len() != arity)
        {
            return None;
        }
        let item = self.adapted_item(obj_ty.referent(), method, span)?;
        self.record_consumed_receiver(object, obj_ty, span);
        if matches!(item, Type::Unknown) {
            return Some(Type::Unknown);
        }
        let receiver = obj_ty.referent().clone();

        let expected = match method {
            ENUMERATE_METHOD => 0,
            _ => 1,
        };
        if args.len() != expected {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected,
                found: args.len(),
                span,
            });
            return Some(Type::Unknown);
        }
        let type_args = match method {
            ENUMERATE_METHOD => vec![receiver, item],
            FILTER_METHOD => {
                let borrowed = Type::Reference {
                    inner: Box::new(item.clone()),
                    mutable: false,
                };
                let ret = self.check_adapter_function(method, &args[0], borrowed, Type::Bool);
                if matches!(ret, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                vec![receiver, item]
            }
            _ => {
                let ret =
                    self.check_adapter_function(method, &args[0], item.clone(), Type::Unknown);
                if matches!(ret, Type::Unknown) {
                    return Some(Type::Unknown);
                }
                vec![receiver, item, ret]
            }
        };
        Some(self.instantiate_generic(base, &type_args, span))
    }

    /// What one step of the iterator `ty` yields, or `None` when `ty` is not one.
    fn adapted_item(&mut self, ty: &Type, method: &str, span: Span) -> Option<Type> {
        match ty {
            Type::Generic(param) => self.bound_iterator_item(param, method, span),
            _ => self.iterator_item(ty),
        }
    }

    /// Check the function an adapter stores, handed `param` per element and expected to
    /// answer `answer` (`Unknown` when any value will do). Answers its return type,
    /// `Unknown` once a diagnostic says why there is none.
    ///
    /// The function is moved into the adapter, so an `FnMut` binding is gone afterwards
    /// and an `Fn` one is copied. An `FnOnce` value is refused: the adapter calls it per
    /// element. No function-value slot is opened, so a value that must stay in its call
    /// (a `mut` parameter, say) is refused as any store of one is.
    fn check_adapter_function(
        &mut self,
        method: &str,
        arg: &Expr,
        param: Type,
        answer: Type,
    ) -> Type {
        let context = closure_context(arg, vec![param.clone()], answer.clone());
        let callee = self
            .check_expr(arg, Some(&context))
            .unwrap_or(Type::Unknown);
        self.refuse_once_per_element(&callee, method, arg.span());
        self.record_move(arg);

        let (found, ret) = match &callee {
            Type::Function { params, ret, .. } if params.len() == 1 => {
                (params[0].clone(), (**ret).clone())
            }
            Type::Unknown => return Type::Unknown,
            found => {
                self.record_error(TypeError::LoopAdapterNotCallable {
                    adapter: method.to_string(),
                    found: found.clone(),
                    span: arg.span(),
                });
                return Type::Unknown;
            }
        };
        if !matches!(param.referent(), Type::Unknown) && !self.assignable(&param, &found) {
            self.record_error(TypeError::LoopAdapterInput {
                adapter: method.to_string(),
                expected: param,
                found,
                span: arg.span(),
            });
            return Type::Unknown;
        }
        let refused = match answer {
            Type::Bool => (!matches!(ret, Type::Bool | Type::Unknown)).then_some(FILTER_RESULT),
            _ => matches!(ret, Type::Void).then_some(MAP_RESULT),
        };
        if let Some(expected) = refused {
            self.record_error(TypeError::LoopAdapterOutput {
                adapter: method.to_string(),
                expected: expected.to_string(),
                found: ret,
                span: arg.span(),
            });
            return Type::Unknown;
        }
        ret
    }
}
