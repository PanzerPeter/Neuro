// Named dynamic extents `?B`: the scope a name lives in, the binding sites that bind it,
// the view of a value's extents that a field read or a call argument has, and the result
// type a call takes its names from.
//
// A name is `ArrayLen::Dynamic(Some(B))`. It is in scope under the symbol-table key `?B`,
// so the block scoping that governs variables governs names with no extra bookkeeping.
// `?` cannot begin a source identifier, so the key collides with no variable. A name
// bound from another name records that name as its target (`Type::Generic`), so both
// spellings denote one run-time value and an operator joins them.

use std::collections::HashMap;

use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::{ArrayLen, Type};
use shared_types::Span;

/// The extent each dynamic name of one call (or one annotation) takes from its arguments.
pub(crate) type DynamicBindings = HashMap<String, ArrayLen>;

/// The symbol-table key of the dynamic name `name`.
fn dynamic_key(name: &str) -> String {
    format!("?{name}")
}

/// Record the extents `arg` gives the dynamic names of `param`, in the order the arguments
/// are visited. A name takes the first found extent that is not an anonymous `?`, else
/// `?`: once the run-time checks pass every occurrence is the same value, so any of them
/// names the result's extent. Only a top-level tensor is read, since a parameter names
/// nothing deeper (the declaration refuses it there).
pub(crate) fn bind_dynamic_names(param: &Type, arg: &Type, bound: &mut DynamicBindings) {
    let (
        Type::Tensor {
            shape: param_shape, ..
        },
        Type::Tensor {
            shape: arg_shape, ..
        },
    ) = (param.referent(), arg.referent())
    else {
        return;
    };
    // A rank mismatch binds nothing: the argument is refused as a whole.
    if param_shape.len() != arg_shape.len() {
        return;
    }
    for (axis, found) in param_shape.iter().zip(arg_shape) {
        let ArrayLen::Dynamic(Some(name)) = &axis.extent else {
            continue;
        };
        let entry = bound
            .entry(name.clone())
            .or_insert_with(|| found.extent.clone());
        if *entry == ArrayLen::Dynamic(None) {
            *entry = found.extent.clone();
        }
    }
}

impl TypeChecker {
    /// Whether the dynamic name `name` is in scope.
    pub(crate) fn dynamic_name_in_scope(&self, name: &str) -> bool {
        self.symbols.lookup(&dynamic_key(name)).is_some()
    }

    /// The run-time extent `name` reads as: the target it was bound from, or `name` itself.
    pub(crate) fn canonical_dynamic_name(&self, name: &str) -> String {
        match self.symbols.lookup(&dynamic_key(name)).map(|info| &info.ty) {
            Some(Type::Generic(target)) => target.clone(),
            _ => name.to_string(),
        }
    }

    /// Whether a variable, parameter or constant named `name` is visible here.
    fn value_named(&self, name: &str) -> bool {
        self.symbols.lookup(name).is_some()
            || self.constants.contains_key(name)
            || self.const_scope.contains_key(name)
    }

    /// Report a value binding `name` that shadows a dynamic name or a size, shape or `const`
    /// parameter in scope, returning whether it did. Both would name the same identifier in
    /// one scope, and a value read of it could not tell them apart: lowering would read the
    /// parameter where the checker typed the local.
    pub(crate) fn refuse_reserved_binding_name(&mut self, name: &str, span: Span) -> bool {
        if self.const_scope.contains_key(name) {
            self.record_error(TypeError::BindingShadowsGenericParam {
                name: name.to_string(),
                span,
            });
            return true;
        }
        if !self.dynamic_name_in_scope(name) {
            return false;
        }
        self.record_error(TypeError::DynamicNameConflict {
            name: name.to_string(),
            span,
        });
        true
    }

    /// Bring the fresh dynamic name `name` into the current scope. `found` is the extent
    /// its binding site gives it: a name found in another name's extent becomes an alias
    /// of that name, anything else leaves it a value of its own. Returns `false` when a
    /// variable or constant already holds the identifier, which is reported.
    pub(crate) fn introduce_dynamic_name(
        &mut self,
        name: &str,
        found: Option<&ArrayLen>,
        span: Span,
    ) -> bool {
        if self.value_named(name) {
            self.record_error(TypeError::DynamicNameConflict {
                name: name.to_string(),
                span,
            });
            return false;
        }
        let ty = match found {
            Some(ArrayLen::Dynamic(Some(target))) => {
                Type::Generic(self.canonical_dynamic_name(target))
            }
            _ => Type::U64,
        };
        if self.dynamic_name_in_scope(name) {
            return true;
        }
        self.symbols.define(dynamic_key(name), ty, false).is_ok()
    }

    /// Bring every name `ty` writes that is not yet in scope into the current scope, each
    /// bound to the extent `found` gives it, then read `ty` through the scope. The binding
    /// site of a `val` or `mut` annotation is this.
    pub(crate) fn bind_annotation_names(
        &mut self,
        ty: &Type,
        found: Option<&Type>,
        span: Span,
    ) -> Type {
        let mut bound = DynamicBindings::new();
        if let Some(found) = found {
            bind_dynamic_names(ty, found, &mut bound);
        }
        for name in self.dynamic_names_of(ty, span) {
            if self.dynamic_name_in_scope(&name) {
                continue;
            }
            self.introduce_dynamic_name(&name, bound.get(&name), span);
        }
        self.canonicalize_dynamic(ty, span)
    }

    /// `ty` with every dynamic extent rewritten by `rewrite`, through tuples, references,
    /// arrays, collections and the instances of generic structs and enums (`Option<..>`).
    pub(crate) fn map_dynamic_extents(
        &mut self,
        ty: &Type,
        span: Span,
        rewrite: &mut dyn FnMut(&ArrayLen) -> ArrayLen,
    ) -> Type {
        match ty {
            Type::Tensor { element, shape } => Type::Tensor {
                element: element.clone(),
                shape: shape
                    .iter()
                    .map(|axis| axis.with_extent(rewrite(&axis.extent)))
                    .collect(),
            },
            Type::Reference { inner, mutable } => Type::Reference {
                inner: Box::new(self.map_dynamic_extents(inner, span, rewrite)),
                mutable: *mutable,
            },
            Type::Tuple(elements) => {
                let mut mapped = Vec::with_capacity(elements.len());
                for element in elements {
                    mapped.push(self.map_dynamic_extents(element, span, rewrite));
                }
                Type::Tuple(mapped)
            }
            Type::Array { element, size } => Type::Array {
                element: Box::new(self.map_dynamic_extents(element, span, rewrite)),
                size: size.clone(),
            },
            Type::Slice(element) => {
                Type::Slice(Box::new(self.map_dynamic_extents(element, span, rewrite)))
            }
            Type::Collection { kind, args } => {
                let mut mapped = Vec::with_capacity(args.len());
                for arg in args {
                    mapped.push(self.map_dynamic_extents(arg, span, rewrite));
                }
                Type::Collection {
                    kind: *kind,
                    args: mapped,
                }
            }
            Type::Struct(name) | Type::Enum(name) => {
                let Some((base, args)) = self.instance_parts(name) else {
                    return ty.clone();
                };
                let mut mapped = Vec::with_capacity(args.len());
                for arg in &args {
                    mapped.push(self.map_dynamic_extents(arg, span, rewrite));
                }
                if mapped == args {
                    return ty.clone();
                }
                self.instantiate_generic(&base, &mapped, span)
            }
            other => other.clone(),
        }
    }

    /// Every dynamic name `ty` writes, in first-occurrence order.
    pub(crate) fn dynamic_names_of(&mut self, ty: &Type, span: Span) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        self.map_dynamic_extents(ty, span, &mut |extent| {
            if let ArrayLen::Dynamic(Some(name)) = extent
                && !names.contains(name)
            {
                names.push(name.clone());
            }
            extent.clone()
        });
        names
    }

    /// `ty` read through the scope: each name bound from another name becomes that name.
    pub(crate) fn canonicalize_dynamic(&mut self, ty: &Type, span: Span) -> Type {
        let mut targets: HashMap<String, String> = HashMap::new();
        for name in self.dynamic_names_of(ty, span) {
            let target = self.canonical_dynamic_name(&name);
            targets.insert(name, target);
        }
        if targets.iter().all(|(name, target)| name == target) {
            return ty.clone();
        }
        self.map_dynamic_extents(ty, span, &mut |extent| match extent {
            ArrayLen::Dynamic(Some(name)) => {
                ArrayLen::Dynamic(Some(targets.get(name).cloned().unwrap_or(name.clone())))
            }
            other => other.clone(),
        })
    }

    /// `ty` with every dynamic name made anonymous. A parameter, an annotation, a struct
    /// field's type, and a field read or write all see the extent of a value this way: the
    /// name a binding site binds is the run-time check's, not the type's.
    pub(crate) fn anonymize_dynamic(&mut self, ty: &Type, span: Span) -> Type {
        self.map_dynamic_extents(ty, span, &mut |extent| match extent {
            ArrayLen::Dynamic(Some(_)) => ArrayLen::Dynamic(None),
            other => other.clone(),
        })
    }

    /// `ty` with the names a call bound replaced by their extents. A name no argument bound
    /// (its argument was refused) becomes an anonymous `?`, so the callee's name never
    /// leaks into the caller's scope.
    ///
    /// Lowering re-types a call's result only through tensors, tuples and borrows; inside any
    /// other type (an `Option`, a struct instance) the instance it builds keeps `?`. A static
    /// extent bound there becomes `?` here too, so the two agree on the result's type.
    pub(crate) fn substitute_dynamic(
        &mut self,
        ty: &Type,
        bound: &DynamicBindings,
        span: Span,
    ) -> Type {
        match ty {
            Type::Tensor { .. } => self.map_dynamic_extents(ty, span, &mut |extent| match extent {
                ArrayLen::Dynamic(Some(name)) => {
                    bound.get(name).cloned().unwrap_or(ArrayLen::Dynamic(None))
                }
                other => other.clone(),
            }),
            Type::Reference { inner, mutable } => Type::Reference {
                inner: Box::new(self.substitute_dynamic(inner, bound, span)),
                mutable: *mutable,
            },
            Type::Tuple(elements) => {
                let mut mapped = Vec::with_capacity(elements.len());
                for element in elements {
                    mapped.push(self.substitute_dynamic(element, bound, span));
                }
                Type::Tuple(mapped)
            }
            other => self.map_dynamic_extents(other, span, &mut |extent| match extent {
                ArrayLen::Dynamic(Some(name)) => match bound.get(name) {
                    Some(found @ ArrayLen::Dynamic(_)) => found.clone(),
                    _ => ArrayLen::Dynamic(None),
                },
                other => other.clone(),
            }),
        }
    }

    /// Refuse a return type that names a dynamic extent no parameter names, since the
    /// extent it returns would have no argument to come from. Reports each such name once.
    pub(crate) fn refuse_unbound_return_names(&mut self, params: &[Type], ret: &Type, span: Span) {
        let mut in_params: Vec<String> = Vec::new();
        for param in params {
            for name in self.dynamic_names_of(param, span) {
                in_params.push(name);
            }
        }
        for name in self.dynamic_names_of(ret, span) {
            if !in_params.contains(&name) {
                self.record_error(TypeError::DynamicNameUnbound { name, span });
            }
        }
    }
}
