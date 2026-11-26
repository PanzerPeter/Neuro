// What a signature's explicit lifetimes say a call keeps: the parameters whose borrows its
// result may hold, and those a store into a `&mut` parameter's referent may take.
//
// Without explicit lifetimes a call is judged conservatively: its result holds every borrow
// its arguments and receiver carry, and a `&mut` argument whose referent can hold a borrow
// takes every borrow the others carry. A signature that names every lifetime its result (or
// a `&mut` parameter's referent) has narrows that to the parameters whose types name one of
// those lifetimes. A parameter whose type may carry a borrow it does not name (a holder
// declaring no lifetime, a function value, a trait object) is kept, as under elision.
//
// The narrowing is sound because the callee's body is held to the same summary: a value the
// body returns, or stores through a `&mut` parameter, may carry a parameter's borrows only
// where the summary says so. A parameter's borrows are traced through the body by the mark
// `SymbolTable::hold_param_origin` gives each parameter.

use std::collections::{HashMap, HashSet};

use ast_types::{Expr, GenericArg, Parameter, SelfParam, Type as AstType};

use super::TypeChecker;
use crate::symbol_table::param_origin;
use crate::types::Type;

/// The receiver's name among a method's parameters.
pub(crate) const RECEIVER: &str = "self";

/// What a call to one function keeps of its inputs, read off its explicit lifetimes.
#[derive(Debug, Clone, Default)]
pub(crate) struct LifetimeFlow {
    /// The parameters' names in call order, the receiver excluded.
    params: Vec<String>,
    /// The parameters (`self` for the receiver) whose borrows the result may hold, or `None`
    /// when the return type leaves a lifetime to elision, which lets it take any of them.
    result: Option<HashSet<String>>,
    /// For each `&mut` parameter (`self` for a `&mut self` receiver) whose referent's type
    /// names every lifetime it has: the parameters whose borrows a store into it may take.
    stores: HashMap<String, HashSet<String>>,
}

impl LifetimeFlow {
    /// Whether the result may hold what the parameter `param` carries.
    pub(crate) fn result_takes(&self, param: &str) -> bool {
        self.result
            .as_ref()
            .is_none_or(|names| names.contains(param))
    }

    /// Whether the result may hold what the argument at `index` carries.
    pub(crate) fn result_takes_argument(&self, index: usize) -> bool {
        self.params
            .get(index)
            .is_none_or(|param| self.result_takes(param))
    }

    /// Whether a store into what `target` points at may take what `param` carries.
    pub(crate) fn store_takes(&self, target: &str, param: &str) -> bool {
        self.stores
            .get(target)
            .is_none_or(|names| names.contains(param))
    }

    /// The parameters a store into what `target` points at may take, when the signature
    /// names every lifetime of the referent's type.
    pub(crate) fn stores_into(&self, target: &str) -> Option<&HashSet<String>> {
        self.stores.get(target)
    }

    /// [`Self::store_takes`] by argument position: `None` is the receiver.
    pub(crate) fn store_takes_argument(
        &self,
        target: Option<usize>,
        source: Option<usize>,
    ) -> bool {
        let name = |index: Option<usize>| match index {
            None => Some(RECEIVER),
            Some(index) => self.params.get(index).map(String::as_str),
        };
        match (name(target), name(source)) {
            (Some(target), Some(source)) => self.store_takes(target, source),
            _ => true,
        }
    }
}

/// How one parameter's type, or the return type, stands against the signature's lifetimes.
struct LifetimeView {
    /// The lifetime names the type writes.
    names: HashSet<String>,
    /// Every borrow a value of the type can carry sits at a lifetime position the type
    /// has, named or elided.
    closed: bool,
    /// Every lifetime position the type has is named.
    all_named: bool,
}

impl LifetimeView {
    /// Whether the type names every lifetime a value of it can carry a borrow under.
    fn fully_named(&self) -> bool {
        self.closed && self.all_named
    }

    /// Whether a value of this type may carry a borrow under one of `names`: it names one,
    /// or carries borrows it names no lifetime for.
    fn may_reach(&self, names: &HashSet<String>) -> bool {
        !self.closed || !self.names.is_disjoint(names)
    }
}

/// The receiver of a method being summarized: how it is taken, and the lifetimes the `impl`
/// gives its type.
pub(crate) struct Receiver<'a> {
    pub(crate) mode: &'a SelfParam,
    pub(crate) type_name: &'a str,
    pub(crate) type_lifetimes: &'a [shared_types::Identifier],
}

impl TypeChecker {
    /// The summary of a signature, or `None` when the signature names no lifetime and every
    /// call to it is judged conservatively. `declared` is every lifetime in scope (the
    /// function's own and its `impl`'s), `own_lifetimes` the ones the method declares itself,
    /// which may be the receiver's (`&'b self`).
    pub(crate) fn lifetime_flow(
        &self,
        declared: &HashSet<String>,
        own_lifetimes: bool,
        receiver: Option<Receiver<'_>>,
        params: &[Parameter],
        ret: Option<&AstType>,
    ) -> Option<LifetimeFlow> {
        if declared.is_empty() {
            return None;
        }
        let self_names: HashSet<String> = receiver
            .as_ref()
            .map(|r| r.type_lifetimes.iter().map(|l| l.name.clone()).collect())
            .unwrap_or_default();
        let mut views: Vec<(String, LifetimeView)> = Vec::new();
        if let Some(receiver) = &receiver {
            let holds = self
                .impl_target_type(receiver.type_name)
                .is_none_or(|ty| self.may_hold_borrow(&ty));
            let mut names = self_names.clone();
            // `&'b self` is erased by the parser, so a lifetime the method declares may be
            // the receiver's.
            if own_lifetimes && !matches!(receiver.mode, SelfParam::Owned) {
                names.extend(declared.iter().cloned());
            }
            views.push((
                RECEIVER.to_string(),
                LifetimeView {
                    names,
                    closed: !holds || !receiver.type_lifetimes.is_empty(),
                    all_named: false,
                },
            ));
        }
        for param in params {
            views.push((
                param.name.name.clone(),
                self.lifetime_view(&param.ty, &self_names),
            ));
        }
        let reaching = |names: &HashSet<String>, own: &str| -> HashSet<String> {
            views
                .iter()
                .filter(|(name, view)| name != own && view.may_reach(names))
                .map(|(name, _)| name.clone())
                .collect()
        };
        let result = ret
            .map(|ret| self.lifetime_view(ret, &self_names))
            .filter(LifetimeView::fully_named)
            .map(|view| reaching(&view.names, ""));
        let mut stores = HashMap::new();
        for param in params {
            if let AstType::Reference {
                inner,
                mutable: true,
                ..
            } = &param.ty
            {
                let referent = self.lifetime_view(inner, &self_names);
                if referent.fully_named() && !referent.names.is_empty() {
                    let name = &param.name.name;
                    stores.insert(name.clone(), reaching(&referent.names, name));
                }
            }
        }
        if let Some(receiver) = &receiver
            && matches!(receiver.mode, SelfParam::RefMut)
            && !receiver.type_lifetimes.is_empty()
            && self.declared_lifetimes.get(receiver.type_name).copied()
                == Some(receiver.type_lifetimes.len())
        {
            stores.insert(RECEIVER.to_string(), reaching(&self_names, RECEIVER));
        }
        Some(LifetimeFlow {
            params: params.iter().map(|p| p.name.name.clone()).collect(),
            result,
            stores,
        })
    }

    fn lifetime_view(&self, ty: &AstType, self_names: &HashSet<String>) -> LifetimeView {
        let mut names = HashSet::new();
        let mut named = 0;
        self.collect_lifetime_names(ty, self_names, &mut names, &mut named);
        let positions = self.lifetime_positions(ty);
        LifetimeView {
            closed: self.lifetime_closed(ty),
            all_named: named >= positions,
            names,
        }
    }

    /// Gather the lifetime names `ty` writes into `names`, counting each named position in
    /// `named`. `Self` names the `impl`'s lifetimes.
    fn collect_lifetime_names(
        &self,
        ty: &AstType,
        self_names: &HashSet<String>,
        names: &mut HashSet<String>,
        named: &mut usize,
    ) {
        match ty {
            AstType::Reference {
                inner, lifetime, ..
            } => {
                if let Some(lifetime) = lifetime.as_ref().filter(|l| l.name != "_") {
                    names.insert(lifetime.name.clone());
                    *named += 1;
                }
                self.collect_lifetime_names(inner, self_names, names, named);
            }
            AstType::Named(name) if name.name == "Self" => {
                names.extend(self_names.iter().cloned());
                *named += self_names.len();
            }
            AstType::Generic {
                args, lifetimes, ..
            } => {
                for lifetime in lifetimes.iter().filter(|l| l.name != "_") {
                    names.insert(lifetime.name.clone());
                    *named += 1;
                }
                for arg in args {
                    if let GenericArg::Type(arg) = arg {
                        self.collect_lifetime_names(arg, self_names, names, named);
                    }
                }
            }
            AstType::Tuple { elements, .. } => {
                for element in elements {
                    self.collect_lifetime_names(element, self_names, names, named);
                }
            }
            AstType::Array { element, .. } | AstType::Slice { element, .. } => {
                self.collect_lifetime_names(element, self_names, names, named);
            }
            _ => {}
        }
    }

    /// Whether every borrow a value of `ty` can carry sits at a lifetime position of `ty`: no
    /// function value, trait object, `impl Trait` or type parameter, and no holder that
    /// declares no lifetime for the reference it holds.
    fn lifetime_closed(&self, ty: &AstType) -> bool {
        match ty {
            AstType::Reference { inner, .. } => self.lifetime_closed(inner),
            AstType::Tuple { elements, .. } => elements.iter().all(|e| self.lifetime_closed(e)),
            AstType::Array { element, .. } | AstType::Slice { element, .. } => {
                self.lifetime_closed(element)
            }
            AstType::Named(name) => self.named_type_closed(&name.name, &[]),
            AstType::Generic { name, args, .. } => self.named_type_closed(&name.name, args),
            AstType::Tensor { .. } => true,
            _ => false,
        }
    }

    fn named_type_closed(&self, name: &str, args: &[GenericArg]) -> bool {
        if self.generic_scope.contains(name) || name == "Self" {
            return false;
        }
        let args_closed = args.iter().all(|arg| match arg {
            GenericArg::Type(arg) => self.lifetime_closed(arg),
            _ => true,
        });
        if !args_closed {
            return false;
        }
        if self.declared_lifetimes.get(name).is_some_and(|n| *n > 0) {
            return true;
        }
        // A type declaring no lifetime is closed when it holds no borrow of its own: a
        // primitive, or a builtin generic whose borrows are its arguments'.
        match self.impl_target_type(name) {
            Some(ty) => !self.may_hold_borrow(&ty),
            None => true,
        }
    }

    /// The summary of the function `func` calls, when it has one.
    pub(crate) fn callee_flow(&self, func: &Expr) -> Option<&LifetimeFlow> {
        let mut callee = func;
        while let Expr::Paren(inner, _) = callee {
            callee = inner;
        }
        // A local function value shadowing a declared function is not that function.
        if let Expr::Identifier(id) = callee
            && self.symbols.lookup(&id.name).is_some()
        {
            return None;
        }
        self.lifetime_flows.get(&self.callee_key(callee)?)
    }

    /// The parameter of the function being checked whose borrows `place` stands for: its
    /// origin mark, or the parameter's own binding.
    pub(crate) fn origin_param<'p>(&self, place: &'p str) -> Option<&'p str> {
        if let Some(param) = param_origin(place) {
            return Some(param);
        }
        (self.current_params.contains(place)
            && self.symbols.defining_depth(place) == Some(self.current_param_scope))
        .then_some(place)
    }

    /// Mark every parameter of the body just entered, `self` included, as carrying its
    /// caller's borrows, and put the body's summary in force.
    pub(crate) fn enter_lifetime_flow(&mut self, key: &str, params: &[(String, &Type)]) {
        self.current_flow = self.lifetime_flows.get(key).cloned();
        self.flow_governs_returns = true;
        self.current_param_scope = self.symbols.depth().saturating_sub(1);
        self.current_params.clear();
        for (name, ty) in params {
            if name == RECEIVER || matches!(ty, Type::Reference { .. }) || self.may_hold_borrow(ty)
            {
                self.symbols.hold_param_origin(name);
                self.current_params.insert(name.clone());
            }
        }
    }

    /// Lift what [`Self::enter_lifetime_flow`] put in force.
    pub(crate) fn leave_lifetime_flow(&mut self) {
        self.current_flow = None;
        self.current_params.clear();
    }
}
