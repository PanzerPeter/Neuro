// The call-site naming rules every declaration in the program imposes.

use std::collections::HashMap;

use ast_types::{GenericArg, GenericParam, ImplDef, Item, Parameter, TraitDef, Type};

/// What one parameter accepts at a call site.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParamBinding {
    /// The name a caller may write, or `None` when the `_` label suppresses it.
    pub(crate) name: Option<String>,
    /// The parameter's own name, kept even when no caller may write it: a `_`-labelled
    /// parameter is exactly the one a caller is most likely to try to name, and saying
    /// so beats "no parameter named that".
    pub(crate) internal: String,
    /// Whether omitting that name is an error (the `external internal:` form).
    pub(crate) required: bool,
    /// The parameter's declared type, when restating it at a call site is meaningful —
    /// see [`annotatable_type`]. It annotates a temporary the hoisting rewrite binds an
    /// argument to, so the argument is still typed by its parameter rather than by
    /// itself. `None` leaves the temporary to inference, which is always sound.
    pub(crate) ty: Option<Type>,
}

/// A callee's parameters in declaration order — the order arguments are bound into.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Signature {
    pub(crate) params: Vec<ParamBinding>,
}

impl Signature {
    fn from_params(params: &[Parameter], generics: &[GenericParam]) -> Self {
        Signature {
            params: params
                .iter()
                .map(|p| ParamBinding {
                    name: p.label.call_site_name(&p.name).map(str::to_string),
                    internal: p.name.name.clone(),
                    required: p.label.is_required(),
                    ty: annotatable_type(&p.ty, generics),
                })
                .collect(),
        }
    }

    /// The same signature with every parameter type dropped.
    ///
    /// A method is matched by name across every `impl` and `trait` declaring it, and two
    /// declarations that agree on their parameter *names* need not agree on their types.
    /// Keeping the types would either make such a pair disagree — rejecting a named
    /// argument that binds correctly today — or annotate a temporary with the wrong
    /// type, so a method signature carries none.
    fn without_types(mut self) -> Self {
        for param in &mut self.params {
            param.ty = None;
        }
        self
    }

    /// Whether any parameter must be named, which is what forces an all-positional
    /// call to be checked rather than passed through untouched.
    pub(crate) fn has_required_label(&self) -> bool {
        self.params.iter().any(|p| p.required)
    }

    /// The declaration index of the parameter a caller names `label`.
    pub(crate) fn position_of(&self, label: &str) -> Option<usize> {
        self.params
            .iter()
            .position(|p| p.name.as_deref() == Some(label))
    }

    /// Whether `label` names a parameter whose `_` label makes it positional-only.
    pub(crate) fn is_suppressed(&self, label: &str) -> bool {
        self.params
            .iter()
            .any(|p| p.name.is_none() && p.internal == label)
    }
}

/// What is known about an instance method reached by name alone.
///
/// A receiver's type is not available before type checking, so a method call is matched
/// on the method name across every `impl` and `trait` in the program. That is exact
/// while one set of parameter names answers to the name, and deliberately gives up
/// otherwise rather than guessing at a receiver.
#[derive(Debug, Clone, PartialEq)]
enum MethodEntry {
    Agreed(Signature),
    Conflicting,
}

/// Every call-site naming rule the program declares, indexed the three ways a call can
/// name its callee.
#[derive(Debug, Default)]
pub(crate) struct SignatureTable {
    functions: HashMap<String, Signature>,
    assoc: HashMap<(String, String), Signature>,
    methods: HashMap<String, MethodEntry>,
}

/// What a lookup found for a call site.
pub(crate) enum Lookup<'a> {
    /// The callee's parameters are known.
    Known(&'a Signature),
    /// Nothing in the program declares parameter names under this callee — a closure,
    /// a builtin, an enum variant, a newtype constructor.
    Unknown,
    /// Several methods answer to the name with different parameter names.
    Ambiguous,
}

impl SignatureTable {
    pub(crate) fn build(items: &[Item]) -> Self {
        let mut table = SignatureTable::default();
        table.collect(items);
        table
    }

    fn collect(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(def) => {
                    self.functions.insert(
                        def.name.name.clone(),
                        Signature::from_params(&def.params, &def.generics),
                    );
                }
                Item::Impl(def) => self.collect_impl(def),
                Item::Trait(def) => self.collect_trait(def),
                // Inline blocks are lifted into the flat item list before this pass runs;
                // descending anyway keeps the table correct for any caller that has not.
                Item::Module(def) => self.collect(&def.items),
                _ => {}
            }
        }
    }

    fn collect_impl(&mut self, def: &ImplDef) {
        for method in &def.methods {
            let sig = Signature::from_params(&method.params, &def.generics);
            match method.self_param {
                None => {
                    self.assoc
                        .insert((def.type_name.name.clone(), method.name.name.clone()), sig);
                }
                Some(_) => self.record_method(&method.name.name, sig.without_types()),
            }
        }
    }

    fn collect_trait(&mut self, def: &TraitDef) {
        for method in &def.methods {
            if method.self_param.is_some() {
                self.record_method(
                    &method.name.name,
                    Signature::from_params(&method.params, &[]).without_types(),
                );
            }
        }
    }

    fn record_method(&mut self, name: &str, sig: Signature) {
        match self.methods.get(name) {
            None => {
                self.methods
                    .insert(name.to_string(), MethodEntry::Agreed(sig));
            }
            Some(MethodEntry::Agreed(existing)) if *existing == sig => {}
            Some(_) => {
                self.methods
                    .insert(name.to_string(), MethodEntry::Conflicting);
            }
        }
    }

    pub(crate) fn function(&self, name: &str) -> Lookup<'_> {
        match self.functions.get(name) {
            Some(sig) => Lookup::Known(sig),
            None => Lookup::Unknown,
        }
    }

    pub(crate) fn assoc_function(&self, type_name: &str, member: &str) -> Lookup<'_> {
        match self.assoc.get(&(type_name.to_string(), member.to_string())) {
            Some(sig) => Lookup::Known(sig),
            None => Lookup::Unknown,
        }
    }

    pub(crate) fn method(&self, name: &str) -> Lookup<'_> {
        match self.methods.get(name) {
            Some(MethodEntry::Agreed(sig)) => Lookup::Known(sig),
            Some(MethodEntry::Conflicting) => Lookup::Ambiguous,
            None => Lookup::Unknown,
        }
    }
}

/// A parameter's declared type, or `None` when restating it at a call site would not mean
/// the same thing there.
///
/// A type parameter of the callee (`T`), `Self`, and an `impl Trait` bound all name
/// something that exists only inside the declaration. A `dyn Trait` and a function type
/// are dropped too: an argument reaches them through a coercion the argument position
/// applies and a binding does not.
fn annotatable_type(ty: &Type, generics: &[GenericParam]) -> Option<Type> {
    is_annotatable(ty, generics).then(|| ty.clone())
}

fn is_annotatable(ty: &Type, generics: &[GenericParam]) -> bool {
    match ty {
        Type::Named(name) => names_a_concrete_type(&name.name, generics),
        Type::Reference { inner, .. } => is_annotatable(inner, generics),
        Type::Array { element, size, .. } => {
            is_annotatable(element, generics)
                && match size {
                    ast_types::ArraySize::Literal(_) => true,
                    ast_types::ArraySize::Const(name) => {
                        names_a_concrete_type(&name.name, generics)
                    }
                }
        }
        // `&[T]` is reachable at a call site only through the unsizing coercion an
        // argument position applies, which a `val` annotation does not — the same reason
        // `dyn Trait` is dropped below.
        Type::Slice { .. } => false,
        Type::Tuple { elements, .. } => elements.iter().all(|e| is_annotatable(e, generics)),
        Type::Generic { name, args, .. } => {
            names_a_concrete_type(&name.name, generics)
                && args.iter().all(|arg| match arg {
                    GenericArg::Type(ty) => is_annotatable(ty, generics),
                    GenericArg::Const { .. } => true,
                })
        }
        Type::ImplTrait { .. }
        | Type::DynTrait { .. }
        | Type::Function { .. }
        | Type::Tensor { .. } => false,
    }
}

/// Whether `name` denotes a type the call site can name too, rather than one of the
/// callee's own type parameters or its `Self`.
fn names_a_concrete_type(name: &str, generics: &[GenericParam]) -> bool {
    name != "Self" && !generics.iter().any(|g| g.name.name == name)
}
