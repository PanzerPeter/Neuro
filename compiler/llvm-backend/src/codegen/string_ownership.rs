// Whole-program facts about owned `string` buffers that no single expression can
// settle.
//
// A `string` is a `(ptr, len)` fat pointer that describes a `.rodata` literal
// and a `malloc`'d buffer identically, so ownership is decided statically, from the
// expression that produced the value. Two of the storing positions sit on the
// far side of a call, where the producing expression is not in view:
//
//   * a function's RETURN value — the caller sees `make()` and cannot tell whether the
//     body handed back a literal or a buffer;
//   * a by-value ARGUMENT — the callee owns the frame the buffer was handed into, and
//     the caller cannot tell whether it stored it somewhere that outlives the call.
//
// Both are answered here, once per program, by reading the callee's body. Each answer
// errs the way the rest of the heap-string machinery errs: an unprovable case is `false`
// and leaks one buffer, because the other direction frees `.rodata` or dangles.

use std::collections::{HashMap, HashSet};

use crate::types::Type;
use neuro_hir::{
    HirCollectionKind, HirExpr, HirExprKind, HirInterpPart, HirItem, HirPlace, HirStmt,
    HirTensorAxis, HirType,
};

/// The `StringBuilder` builder method that copies its bytes out into an owned `string`, and
/// the `string` methods that read their receiver without retaining it.
const TO_OWNED_METHOD: &str = "to_string";
/// `string.clone()`, which copies the bytes into a buffer of their own.
const CLONE_METHOD: &str = "clone";
const LEN_METHOD: &str = "len";
const PUSH_STR_METHOD: &str = "push_str";
/// The `string` methods that hand back a view into their receiver.
const VIEW_METHODS: [&str; 2] = ["slice", "char_slice"];

/// The standard-output builtins, which copy the bytes they are handed and keep none.
const IO_BUILTINS: [&str; 2] = ["print", "println"];

/// What the backend knows about `string` ownership across a call boundary.
#[derive(Debug, Default, Clone)]
pub(crate) struct StringOwnership {
    /// Functions, by the name a call site resolves to, whose every return path hands
    /// back a freshly allocated buffer. A call to one is an owned-string producer.
    returns_owned: HashSet<String>,
    /// `(function name, parameter index)` of every `string` parameter whose callee
    /// provably only reads it, so the caller may release the buffer it passed once the
    /// call returns.
    read_only_params: HashSet<(String, usize)>,
    /// Functions, by the name a call site resolves to, whose every return path hands back
    /// one `string` parameter unchanged, with that parameter's index. What such a call
    /// yields is owned exactly when the argument it was passed was.
    forwards: HashMap<String, usize>,
    /// Functions returning a holder, by the name a call site resolves to, with the paths
    /// of the `string` positions every return path stores a fresh allocation into. A call
    /// to one hands its caller those positions owned.
    holder_returns: HashMap<String, Vec<Vec<String>>>,
    /// Functions returning a holder, by the name a call site resolves to, with each
    /// `string` position every return path stores one parameter into unchanged, and that
    /// parameter's index. The position is owned exactly when the argument was.
    holder_forwards: HashMap<String, Vec<(Vec<String>, usize)>>,
}

impl StringOwnership {
    /// Whether a call to `name` yields a buffer its caller now owns.
    pub(crate) fn returns_owned(&self, name: &str) -> bool {
        self.returns_owned.contains(name)
    }

    /// Whether `name`'s `index`-th parameter is read and never retained.
    pub(crate) fn param_is_read_only(&self, name: &str, index: usize) -> bool {
        self.read_only_params.contains(&(name.to_string(), index))
    }

    /// The index of the `string` parameter a call to `name` hands back unchanged.
    pub(crate) fn forwarded_param(&self, name: &str) -> Option<usize> {
        self.forwards.get(name).copied()
    }

    /// The `string` positions, by path, a call to `name` hands back owned.
    pub(crate) fn held_strings_returned(&self, name: &str) -> &[Vec<String>] {
        self.holder_returns.get(name).map_or(&[], Vec::as_slice)
    }

    /// The `string` positions, by path, a call to `name` fills with an argument
    /// unchanged, each with that argument's index.
    pub(crate) fn held_strings_forwarded(&self, name: &str) -> &[(Vec<String>, usize)] {
        self.holder_forwards.get(name).map_or(&[], Vec::as_slice)
    }
}

/// The name [`StringOwnership::param_is_read_only`] answers to for a call of `method`
/// through a `&dyn trait_name`: a parameter is read only there when it is in every
/// implementor's method. The space keeps it apart from every symbol.
pub(crate) fn dyn_callee(trait_name: &str, method: &str) -> String {
    format!("dyn {trait_name}__{method}")
}

/// Read both summaries off the whole program.
///
/// The return summary is a fixpoint because one producer can be another's only return
/// path (`func a() -> string { return b() }`). It starts empty and grows, so it
/// converges, and an optimistic cycle — a function whose only return path is a call to
/// itself — never enters it.
pub(crate) fn analyze(items: &[HirItem]) -> StringOwnership {
    let bodies = string_returning_bodies(items);
    // A top-level name a local binding shadows is not reliably the function at a call
    // site: `codegen_call_dispatch` sends a binding of function type through the
    // indirect path, and the value behind it may be any `string`. Poisoning the name
    // here keeps the call from being read as a producer.
    let shadowed = shadowed_names(items);

    let mut returns_owned: HashSet<String> = HashSet::new();
    loop {
        let grown: Vec<String> = bodies
            .iter()
            .filter(|(name, _)| !returns_owned.contains(name) && !shadowed.contains(name))
            .filter(|(_, exits)| {
                !exits.is_empty() && exits.iter().all(|exit| allocates(exit, &returns_owned))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if grown.is_empty() {
            break;
        }
        returns_owned.extend(grown);
    }

    // A fixpoint for the same reason as the return summary: a parameter handed on to
    // another function is read only when that function's parameter is. It starts empty
    // and grows, so a parameter only ever passed around a cycle stays retained.
    let callables = callables(items);
    let implementors = implementors(items);
    let mut read_only_params: HashSet<(String, usize)> = HashSet::new();
    loop {
        let mut grown = Vec::new();
        for (name, params, body) in &callables {
            for (index, param) in params.iter().enumerate() {
                if !matches!(param.1, HirType::String)
                    || read_only_params.contains(&(name.clone(), index))
                {
                    continue;
                }
                let probe = Param {
                    name: param.0,
                    read_only: &read_only_params,
                    shadowed: &shadowed,
                    implementors: &implementors,
                };
                if body.iter().any(|stmt| stmt_retains(stmt, &probe)) {
                    continue;
                }
                grown.push((name.clone(), index));
            }
        }
        if grown.is_empty() {
            break;
        }
        read_only_params.extend(grown);
    }
    // A call through a trait object reaches whichever implementor the vtable holds, so
    // its argument is read only when every implementor's parameter is.
    let through_dyn: Vec<(String, usize)> = items
        .iter()
        .filter_map(|item| match item {
            HirItem::Impl(impl_def) => Some(impl_def),
            _ => None,
        })
        .filter_map(|impl_def| Some((impl_def.trait_name.as_deref()?, impl_def)))
        .flat_map(|(trait_name, impl_def)| {
            impl_def.methods.iter().flat_map(move |method| {
                (0..method.params.len()).map(move |index| (trait_name, &method.name, index))
            })
        })
        .filter(|(trait_name, method, index)| {
            reads_through_dyn(trait_name, method, *index, &implementors, &read_only_params)
        })
        .map(|(trait_name, method, index)| (dyn_callee(trait_name, method), index))
        .collect();
    read_only_params.extend(through_dyn);

    // A fixpoint so a forward through another forwarder counts; it starts empty and
    // grows, so a function forwarding only around a cycle never enters it.
    let mut forwards: HashMap<String, usize> = HashMap::new();
    loop {
        let grown: Vec<(String, usize)> = callables
            .iter()
            .filter(|(name, _, _)| !forwards.contains_key(name) && !shadowed.contains(name))
            .filter_map(|(name, params, body)| {
                let (_, exits) = bodies.iter().find(|(returning, _)| returning == name)?;
                let index = forwarded_by(exits.first()?, params, &forwards)?;
                let all_forward = exits
                    .iter()
                    .all(|exit| forwarded_by(exit, params, &forwards) == Some(index));
                (all_forward && !binds(body, params[index].0)).then(|| (name.clone(), index))
            })
            .collect();
        if grown.is_empty() {
            break;
        }
        forwards.extend(grown);
    }

    // The return summary again, per `string` position of a returned holder: the paths
    // every exit stores an allocation into. A fixpoint because an exit may be a call to
    // another such function; each pass only adds paths, so it converges, and a position
    // owned only around a cycle is never claimed.
    let holder_bodies = returning_bodies(items, |ty| {
        matches!(ty, Type::Struct(_) | Type::Tuple(_) | Type::Array { .. })
    });
    let mut holder_returns: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    loop {
        let mut grown = false;
        for (name, exits) in &holder_bodies {
            if exits.is_empty() || shadowed.contains(name) {
                continue;
            }
            let mut every: Option<Vec<Vec<String>>> = None;
            for exit in exits {
                let mut armed = Vec::new();
                allocated_positions(
                    exit,
                    &returns_owned,
                    &holder_returns,
                    &mut Vec::new(),
                    &mut armed,
                );
                every = Some(match every {
                    None => armed,
                    Some(prev) => prev.into_iter().filter(|p| armed.contains(p)).collect(),
                });
            }
            let every = every.unwrap_or_default();
            if every.len() > holder_returns.get(name).map_or(0, Vec::len) {
                holder_returns.insert(name.clone(), every);
                grown = true;
            }
        }
        if !grown {
            break;
        }
    }

    // The forwarder summary again, per `string` position of a returned holder. The
    // parameter may appear nowhere in the body but once in each exit, at the position:
    // a second copy of its fat pointer anywhere would outlive the release the position
    // now carries.
    let mut holder_forwards: HashMap<String, Vec<(Vec<String>, usize)>> = HashMap::new();
    for (name, exits) in &holder_bodies {
        if exits.is_empty() || shadowed.contains(name) {
            continue;
        }
        let Some((_, params, body)) = callables.iter().find(|(callable, ..)| callable == name)
        else {
            continue;
        };
        let mut every: Option<Vec<(Vec<String>, usize)>> = None;
        for exit in exits {
            let mut found = Vec::new();
            forwarded_positions(exit, params, &mut Vec::new(), &mut found);
            every = Some(match every {
                None => found,
                Some(prev) => prev.into_iter().filter(|p| found.contains(p)).collect(),
            });
        }
        let every: Vec<(Vec<String>, usize)> = every
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, index)| {
                let param = params[*index].0;
                !binds(body, param) && mention_count(body, param) == exits.len()
            })
            .collect();
        if !every.is_empty() {
            holder_forwards.insert(name.clone(), every);
        }
    }

    StringOwnership {
        returns_owned,
        read_only_params,
        forwards,
        holder_returns,
        holder_forwards,
    }
}

/// The `string` positions of the holder `expr` builds that hold a bare read of a
/// `string` parameter, with its index, walked with the segments the drop pass gives them.
fn forwarded_positions(
    expr: &HirExpr,
    params: &[(&str, &HirType)],
    path: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, usize)>,
) {
    if matches!(Type::from_hir(&expr.ty), Type::String) {
        if let HirExprKind::Variable(name) = &expr.kind
            && let Some(index) = params
                .iter()
                .position(|(param, ty)| param == name && matches!(ty, HirType::String))
        {
            out.push((path.clone(), index));
        }
        return;
    }
    match &expr.kind {
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                path.push(field.name.clone());
                forwarded_positions(&field.value, params, path, out);
                let _ = path.pop();
            }
        }
        HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
            for (index, element) in elements.iter().enumerate() {
                path.push(index.to_string());
                forwarded_positions(element, params, path, out);
                let _ = path.pop();
            }
        }
        HirExprKind::NewtypeConstruct { value, .. } => {
            forwarded_positions(value, params, path, out)
        }
        _ => {}
    }
}

/// How many times `name` is read anywhere in `body`, a closure capturing it included.
fn mention_count(body: &[HirStmt], name: &str) -> usize {
    let mut count = 0;
    walk_stmts(body, &mut |expr| match &expr.kind {
        HirExprKind::Variable(other) if other == name => count += 1,
        HirExprKind::Closure { captures, .. } => {
            count += captures.iter().filter(|c| c.name == name).count();
        }
        _ => {}
    });
    count
}

/// The paths of the `string` positions of the holder `expr` builds that it stores a
/// fresh allocation into: an aggregate literal's positions [`allocates`] proves, walked
/// with the segments the drop pass gives them, and a call's positions the summary so far
/// proves. Anything else proves nothing.
fn allocated_positions(
    expr: &HirExpr,
    producers: &HashSet<String>,
    holders: &HashMap<String, Vec<Vec<String>>>,
    path: &mut Vec<String>,
    out: &mut Vec<Vec<String>>,
) {
    if matches!(Type::from_hir(&expr.ty), Type::String) {
        if allocates(expr, producers) {
            out.push(path.clone());
        }
        return;
    }
    match &expr.kind {
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                path.push(field.name.clone());
                allocated_positions(&field.value, producers, holders, path, out);
                let _ = path.pop();
            }
        }
        HirExprKind::ArrayLiteral { elements } | HirExprKind::TupleLiteral { elements } => {
            for (index, element) in elements.iter().enumerate() {
                path.push(index.to_string());
                allocated_positions(element, producers, holders, path, out);
                let _ = path.pop();
            }
        }
        HirExprKind::NewtypeConstruct { value, .. } => {
            allocated_positions(value, producers, holders, path, out)
        }
        HirExprKind::Call { callee, .. } => {
            let returned = callee_key(callee).and_then(|key| holders.get(&key));
            for position in returned.into_iter().flatten() {
                out.push(path.iter().chain(position).cloned().collect());
            }
        }
        _ => {}
    }
}

/// The program with a `.clone()` around every constant `string` a function returns
/// beside an allocation, or `None` when no function mixes the two.
///
/// A function counts as handing its caller a buffer only when every exit allocates, so one
/// `"info"` arm beside a `"warn {k}"` arm made every result unowned, and the allocated
/// ones leaked. Copying the constant (a literal, or a call to a function returning only
/// literals) costs one allocation on that path and makes every path own its result,
/// which the summary then sees. A function returning only constants is left alone: it
/// never allocates and its results never reach `free`.
pub(crate) fn promote_literal_exits(items: &[HirItem]) -> Option<Vec<HirItem>> {
    let bodies = string_returning_bodies(items);
    let shadowed = shadowed_names(items);
    // Functions every exit of which is a literal, or a call to another such function:
    // what they return is always `.rodata`, so a copy of it is as safe as a literal's.
    let mut constant: HashSet<String> = HashSet::new();
    loop {
        let grown: Vec<String> = bodies
            .iter()
            .filter(|(name, _)| !constant.contains(name) && !shadowed.contains(name))
            .filter(|(_, exits)| {
                !exits.is_empty() && exits.iter().all(|exit| is_constant_string(exit, &constant))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if grown.is_empty() {
            break;
        }
        constant.extend(grown);
    }
    let mut producers: HashSet<String> = HashSet::new();
    loop {
        let grown: Vec<String> = bodies
            .iter()
            .filter(|(name, _)| !producers.contains(name) && !shadowed.contains(name))
            .filter(|(_, exits)| {
                exits.iter().any(|exit| allocates(exit, &producers))
                    && exits.iter().all(|exit| {
                        is_constant_string(exit, &constant) || allocates(exit, &producers)
                    })
            })
            .map(|(name, _)| name.clone())
            .collect();
        if grown.is_empty() {
            break;
        }
        producers.extend(grown);
    }
    let promoted: HashSet<String> = bodies
        .iter()
        .filter(|(name, exits)| {
            producers.contains(name) && exits.iter().any(|exit| is_constant_string(exit, &constant))
        })
        .map(|(name, _)| name.clone())
        .collect();
    if promoted.is_empty() {
        return None;
    }
    let mut items = items.to_vec();
    let mut copy = |exit: &mut HirExpr| copy_if_constant(exit, &constant);
    for item in &mut items {
        match item {
            HirItem::Function(func) if promoted.contains(&func.name) => {
                copy_constant_exits(&mut func.body, &mut copy);
            }
            HirItem::Closure(closure) if promoted.contains(&closure.name) => {
                copy_constant_exits(&mut closure.body, &mut copy);
            }
            HirItem::Impl(impl_def) => {
                let type_name = impl_def.type_name.clone();
                for method in &mut impl_def.methods {
                    if promoted.contains(&format!("{}__{}", type_name, method.name)) {
                        copy_constant_exits(&mut method.body, &mut copy);
                    }
                }
            }
            _ => {}
        }
    }
    Some(items)
}

/// Whether `expr` is always a `.rodata` string: a literal, or a call to a function that
/// returns only those.
fn is_constant_string(expr: &HirExpr, constant: &HashSet<String>) -> bool {
    match &expr.kind {
        HirExprKind::Literal(shared_types::Literal::String(_)) => true,
        HirExprKind::Call { callee, .. } => {
            callee_key(callee).is_some_and(|key| constant.contains(&key))
        }
        _ => false,
    }
}

/// The name a call to `callee` is summarized under: a function, an associated function
/// or a method on a user type, keyed as [`allocates`] keys them.
fn callee_key(callee: &HirExpr) -> Option<String> {
    match &callee.kind {
        HirExprKind::Variable(name) => Some(name.clone()),
        HirExprKind::Path { type_name, member } => Some(format!("{}__{}", type_name, member)),
        HirExprKind::FieldAccess { object, field } => match object.ty.referent() {
            HirType::Struct(type_name) | HirType::Enum(type_name) => {
                Some(format!("{}__{}", type_name, field))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Wrap each constant exit of `body` in `.clone()`, visiting exactly the exits
/// [`string_returning_bodies`] enumerates: statement-level `return`s and the tail's
/// branch tails.
fn copy_constant_exits(body: &mut [HirStmt], copy: &mut dyn FnMut(&mut HirExpr)) {
    for_each_return_mut(body, copy);
    block_tail_mut(body, copy);
}

fn copy_if_constant(expr: &mut HirExpr, constant: &HashSet<String>) {
    if !is_constant_string(expr, constant) {
        return;
    }
    let value = expr.clone();
    let callee = HirExpr::new(
        HirExprKind::FieldAccess {
            object: Box::new(value),
            field: CLONE_METHOD.to_string(),
        },
        expr.ty.clone(),
        expr.span,
    );
    *expr = HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(callee),
            args: Vec::new(),
        },
        expr.ty.clone(),
        expr.span,
    );
}

/// [`collect_returns`], visiting each returned value mutably.
fn for_each_return_mut(stmts: &mut [HirStmt], visit: &mut dyn FnMut(&mut HirExpr)) {
    for stmt in stmts {
        match stmt {
            HirStmt::Return {
                value: Some(value), ..
            } => visit(value),
            HirStmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                for_each_return_mut(then_block, visit);
                for (_, block) in else_if_blocks {
                    for_each_return_mut(block, visit);
                }
                if let Some(block) = else_block {
                    for_each_return_mut(block, visit);
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::ForRange { body, .. }
            | HirStmt::ForEach { body, .. } => for_each_return_mut(body, visit),
            HirStmt::ValElse { else_block, .. } => for_each_return_mut(else_block, visit),
            _ => {}
        }
    }
}

/// [`tail_exits`], visiting each branch tail mutably.
fn for_each_tail_mut(tail: &mut HirExpr, visit: &mut dyn FnMut(&mut HirExpr)) {
    match &mut tail.kind {
        HirExprKind::If {
            then_block,
            else_if_blocks,
            else_block: Some(else_block),
            ..
        } => {
            block_tail_mut(then_block, visit);
            for (_, block) in else_if_blocks {
                block_tail_mut(block, visit);
            }
            block_tail_mut(else_block, visit);
        }
        HirExprKind::Match { arms, .. } => {
            for arm in arms {
                for_each_tail_mut(&mut arm.body, visit);
            }
        }
        HirExprKind::Block { stmts } => block_tail_mut(stmts, visit),
        HirExprKind::Loop { label, body } => {
            for_each_break_mut(body, label.as_deref(), false, visit);
        }
        _ => visit(tail),
    }
}

/// [`aimed_breaks`], visiting each aimed `break` value mutably. It steps through exactly the
/// lists [`aimed_breaks`] enters, which [`loop_break_values`] has already vouched for.
fn for_each_break_mut(
    stmts: &mut [HirStmt],
    target: Option<&str>,
    nested: bool,
    visit: &mut dyn FnMut(&mut HirExpr),
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Break {
                label,
                value: Some(value),
                ..
            } => {
                if aims_at(label.as_deref(), target, nested) {
                    visit(value);
                }
            }
            HirStmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                for_each_break_mut(then_block, target, nested, visit);
                for (_, block) in else_if_blocks {
                    for_each_break_mut(block, target, nested, visit);
                }
                if let Some(block) = else_block {
                    for_each_break_mut(block, target, nested, visit);
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::ForRange { body, .. }
            | HirStmt::ForEach { body, .. } => for_each_break_mut(body, target, true, visit),
            HirStmt::ValElse { else_block, .. } => {
                for_each_break_mut(else_block, target, nested, visit);
            }
            HirStmt::Expr(expr)
            | HirStmt::VarDecl {
                init: Some(expr), ..
            } => for_each_break_in_expr_mut(expr, target, nested, visit),
            _ => {}
        }
    }
}

/// [`for_each_break_mut`] for the lists an expression holds, [`expression_lists`] mirrored.
fn for_each_break_in_expr_mut(
    expr: &mut HirExpr,
    target: Option<&str>,
    nested: bool,
    visit: &mut dyn FnMut(&mut HirExpr),
) {
    match &mut expr.kind {
        HirExprKind::Block { stmts }
        | HirExprKind::Unsafe { stmts }
        | HirExprKind::Pool { stmts, .. } => for_each_break_mut(stmts, target, nested, visit),
        HirExprKind::Loop { body, .. } => for_each_break_mut(body, target, true, visit),
        HirExprKind::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            for_each_break_mut(then_block, target, nested, visit);
            for (_, block) in else_if_blocks {
                for_each_break_mut(block, target, nested, visit);
            }
            if let Some(block) = else_block {
                for_each_break_mut(block, target, nested, visit);
            }
        }
        HirExprKind::Match { arms, .. } => {
            for arm in arms {
                for_each_break_in_expr_mut(&mut arm.body, target, nested, visit);
            }
        }
        _ => {}
    }
}

/// [`block_tail_exits`], visiting each exit mutably.
fn block_tail_mut(stmts: &mut [HirStmt], visit: &mut dyn FnMut(&mut HirExpr)) {
    let Some((HirStmt::Expr(tail), earlier)) = stmts.split_last_mut() else {
        return;
    };
    if let HirExprKind::Variable(name) = &tail.kind
        && local_initializer(earlier, name).is_some()
    {
        let name = name.clone();
        let init = earlier.iter_mut().rev().find_map(|stmt| match stmt {
            HirStmt::VarDecl {
                name: declared,
                init,
                ..
            } if *declared == name => init.as_mut(),
            _ => None,
        });
        if let Some(init) = init {
            for_each_tail_mut(init, visit);
        }
        return;
    }
    for_each_tail_mut(tail, visit);
}

/// The index of the `string` parameter `exit` hands back unchanged: a read of it, or a
/// call to a known forwarder whose forwarded argument is one.
fn forwarded_by(
    exit: &HirExpr,
    params: &[(&str, &HirType)],
    forwards: &HashMap<String, usize>,
) -> Option<usize> {
    match &exit.kind {
        HirExprKind::Variable(name) => params
            .iter()
            .position(|(param, ty)| *param == name && matches!(Type::from_hir(ty), Type::String)),
        HirExprKind::Call { callee, args } => match &callee.kind {
            HirExprKind::Variable(callee) => {
                forwarded_by(args.get(*forwards.get(callee)?)?, params, forwards)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Whether anything in `body` binds `name`, which would let an exit reading `name`
/// name that binding instead of the parameter: a forward it is not, and treating it as
/// one would hand the caller a buffer it never passed.
fn binds(body: &[HirStmt], name: &str) -> bool {
    let mut bound = HashSet::new();
    collect_bound_names(body, &mut bound);
    walk_stmts(body, &mut |expr| match &expr.kind {
        HirExprKind::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            collect_bound_names(then_block, &mut bound);
            for (_, block) in else_if_blocks {
                collect_bound_names(block, &mut bound);
            }
            if let Some(block) = else_block {
                collect_bound_names(block, &mut bound);
            }
        }
        HirExprKind::Block { stmts }
        | HirExprKind::Unsafe { stmts }
        | HirExprKind::Pool { stmts, .. }
        | HirExprKind::Loop { body: stmts, .. } => collect_bound_names(stmts, &mut bound),
        HirExprKind::Match { arms, .. } => {
            bound.extend(
                arms.iter()
                    .flat_map(|arm| arm.bindings.iter().map(|b| b.name.clone())),
            );
        }
        _ => {}
    });
    bound.contains(name)
}

/// One callable a call site can name: the name it resolves to, its parameters, and the
/// body that says what happens to them.
///
/// A method is keyed by the `Type__method` its call site mangles, and its receiver is
/// not in the parameter list: `self` is passed separately and is never a `string`.
type Callable<'a> = (String, Vec<(&'a str, &'a HirType)>, &'a [HirStmt]);

/// Each trait's implementors, by the type name their methods are mangled under.
fn implementors(items: &[HirItem]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for item in items {
        if let HirItem::Impl(impl_def) = item
            && let Some(trait_name) = &impl_def.trait_name
        {
            out.entry(trait_name.clone())
                .or_default()
                .push(impl_def.type_name.clone());
        }
    }
    out
}

/// Whether `method`'s `index`-th parameter is read only in every implementor of
/// `trait_name`, which is what a call through a `&dyn` of it may assume.
fn reads_through_dyn(
    trait_name: &str,
    method: &str,
    index: usize,
    implementors: &HashMap<String, Vec<String>>,
    read_only: &HashSet<(String, usize)>,
) -> bool {
    implementors.get(trait_name).is_some_and(|types| {
        types
            .iter()
            .all(|ty| read_only.contains(&(format!("{ty}__{method}"), index)))
    })
}

/// Every callable in the program, free functions and `impl` methods alike.
fn callables(items: &[HirItem]) -> Vec<Callable<'_>> {
    let mut out = Vec::new();
    for item in items {
        match item {
            HirItem::Function(func) => out.push((
                func.name.clone(),
                func.params
                    .iter()
                    .map(|p| (p.name.as_str(), &p.ty))
                    .collect(),
                func.body.as_slice(),
            )),
            HirItem::Impl(impl_def) => {
                for method in &impl_def.methods {
                    out.push((
                        format!("{}__{}", impl_def.type_name, method.name),
                        method
                            .params
                            .iter()
                            .map(|p| (p.name.as_str(), &p.ty))
                            .collect(),
                        method.body.as_slice(),
                    ));
                }
            }
            // A closure's lifted body, keyed by its generated name, which an indirect call
            // through a binding the literal initialized resolves to. Its environment
            // pointer is not in the parameter list, and captures are copies, never params.
            HirItem::Closure(closure) => out.push((
                closure.name.clone(),
                closure
                    .params
                    .iter()
                    .map(|p| (p.name.as_str(), &p.ty))
                    .collect(),
                closure.body.as_slice(),
            )),
            _ => {}
        }
    }
    out
}

/// The expressions a `string`-returning callable can leave through, by name.
///
/// A callable with an exit this cannot enumerate gets an empty list and so never
/// qualifies: the tail must be an expression statement, and every other exit an
/// explicit `return`. A tail that is an `if`, a `match` or a block exits through each
/// branch's own tail, and a tail `loop` through the `break` values aimed at it, which
/// [`tail_exits`] reads. A `return` inside an expression (a `loop` body, an `if` used as a
/// value) is added by [`collect_expression_returns`]. A `break` the tail walk cannot account
/// for makes the body unreadable, so it answers the safe way.
fn string_returning_bodies(items: &[HirItem]) -> Vec<(String, Vec<&HirExpr>)> {
    // A newtype over `string` is the `string` itself once erased.
    returning_bodies(items, |ty| matches!(ty, Type::String))
}

/// The exits [`string_returning_bodies`] reads, for the callables whose return type
/// `wanted` accepts.
fn returning_bodies(items: &[HirItem], wanted: fn(&Type) -> bool) -> Vec<(String, Vec<&HirExpr>)> {
    let mut out: Vec<(String, Vec<&HirExpr>)> = Vec::new();
    let mut returning: Vec<(String, &HirType, &[HirStmt])> = Vec::new();
    for item in items {
        match item {
            HirItem::Function(func) => {
                returning.push((func.name.clone(), &func.return_type, &func.body))
            }
            HirItem::Impl(impl_def) => returning.extend(impl_def.methods.iter().map(|method| {
                (
                    format!("{}__{}", impl_def.type_name, method.name),
                    &method.return_type,
                    method.body.as_slice(),
                )
            })),
            HirItem::Closure(closure) => {
                returning.push((closure.name.clone(), &closure.return_type, &closure.body))
            }
            _ => {}
        }
    }
    for (name, return_type, body) in returning {
        if !wanted(&Type::from_hir(return_type)) {
            continue;
        }
        let mut exits = Vec::new();
        collect_returns(body, &mut exits);
        collect_expression_returns(body, &mut exits);
        if matches!(body.last(), Some(HirStmt::Expr(_))) && !block_tail_exits(body, &mut exits) {
            exits.clear();
        }
        out.push((name, exits));
    }
    out
}

/// The values a tail expression leaves through: itself, or for an `if` with an `else`, a
/// `match` or a block, each branch's own tail, or for a `loop` the `break` values aimed at
/// it (none when it has no `break`: it leaves only through a `return`). `false` when a
/// branch ends in anything but an expression, whose value this cannot name.
pub(crate) fn tail_exits<'a>(tail: &'a HirExpr, out: &mut Vec<&'a HirExpr>) -> bool {
    match &tail.kind {
        HirExprKind::If {
            then_block,
            else_if_blocks,
            else_block: Some(else_block),
            ..
        } => {
            block_tail_exits(then_block, out)
                && else_if_blocks
                    .iter()
                    .all(|(_, block)| block_tail_exits(block, out))
                && block_tail_exits(else_block, out)
        }
        HirExprKind::Match { arms, .. } => arms.iter().all(|arm| tail_exits(&arm.body, out)),
        HirExprKind::Block { stmts } => block_tail_exits(stmts, out),
        HirExprKind::Loop { label, body } => loop_break_values(label.as_deref(), body, out),
        _ => {
            out.push(tail);
            true
        }
    }
}

/// The values a statement list leaves through its tail. A tail naming an immutable `val`
/// the same list declares hands on that `val`'s value, moved out of it, so it is read as
/// the initializer: `val b = "ab{i}"` then `b` leaves through the interpolation.
fn block_tail_exits<'a>(stmts: &'a [HirStmt], out: &mut Vec<&'a HirExpr>) -> bool {
    let Some((HirStmt::Expr(tail), earlier)) = stmts.split_last() else {
        return false;
    };
    if let HirExprKind::Variable(name) = &tail.kind
        && let Some(init) = local_initializer(earlier, name)
    {
        return tail_exits(init, out);
    }
    tail_exits(tail, out)
}

/// The `break` values aimed at the `loop` labeled `label` whose body is `body`: an
/// unlabeled `break` outside any loop nested in the body, and a labeled one naming `label`.
/// A `return` inside is not here, [`collect_expression_returns`] has it.
///
/// `false` when a `break` aimed here carries no value, or when the walk cannot account for
/// every `break` in the body (one inside an operand, or in a nested loop sharing `label`),
/// which would leave an exit unnamed. A closure body is a lifted item, never walked into.
fn loop_break_values<'a>(
    label: Option<&str>,
    body: &'a [HirStmt],
    out: &mut Vec<&'a HirExpr>,
) -> bool {
    let mut seen = 0;
    let mut aimed = Vec::new();
    if !aimed_breaks(body, label, false, &mut seen, &mut aimed) {
        return false;
    }
    // Every `break` the walk did not step over sits in a list it never entered.
    let mut total = count_breaks(body);
    walk_stmts(body, &mut |expr| {
        total += expression_blocks(expr)
            .into_iter()
            .map(count_breaks)
            .sum::<usize>();
    });
    out.extend(aimed);
    seen == total
}

/// Collect into `out` the value of each `break` in `stmts` aimed at the loop labeled
/// `target`; `nested` is whether `stmts` sits inside a loop of the body.
fn aimed_breaks<'a>(
    stmts: &'a [HirStmt],
    target: Option<&str>,
    nested: bool,
    seen: &mut usize,
    out: &mut Vec<&'a HirExpr>,
) -> bool {
    stmts.iter().all(|stmt| {
        if let HirStmt::Break { label, value, .. } = stmt {
            *seen += 1;
            if !aims_at(label.as_deref(), target, nested) {
                return true;
            }
            return value.as_ref().is_some_and(|value| {
                out.push(value);
                true
            });
        }
        nested_lists(stmt).into_iter().all(|(list, loop_label)| {
            // A nested loop under the target's own label would make `break label` ambiguous.
            if target.is_some() && loop_label.flatten() == target {
                return false;
            }
            aimed_breaks(list, target, nested || loop_label.is_some(), seen, out)
        })
    })
}

/// Whether a `break` labeled `label` leaves the loop labeled `target`.
fn aims_at(label: Option<&str>, target: Option<&str>, nested: bool) -> bool {
    match label {
        Some(_) => label == target,
        None => !nested,
    }
}

/// The statement lists a statement holds, each with the label of the loop it opens:
/// `Some(label)` for a nested loop (`Some(None)` when unlabeled), `None` for plain nesting.
fn nested_lists(stmt: &HirStmt) -> Vec<(&[HirStmt], Option<Option<&str>>)> {
    match stmt {
        HirStmt::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => std::iter::once(then_block.as_slice())
            .chain(else_if_blocks.iter().map(|(_, block)| block.as_slice()))
            .chain(else_block.as_deref())
            .map(|list| (list, None))
            .collect(),
        HirStmt::While { label, body, .. }
        | HirStmt::ForRange { label, body, .. }
        | HirStmt::ForEach { label, body, .. } => vec![(body.as_slice(), Some(label.as_deref()))],
        HirStmt::ValElse { else_block, .. } => vec![(else_block.as_slice(), None)],
        HirStmt::Expr(expr)
        | HirStmt::VarDecl {
            init: Some(expr), ..
        } => expression_lists(expr),
        _ => Vec::new(),
    }
}

/// [`nested_lists`] for the lists an expression holds: its own blocks, and those of a `match`
/// arm body, which is itself an expression.
fn expression_lists(expr: &HirExpr) -> Vec<(&[HirStmt], Option<Option<&str>>)> {
    if let HirExprKind::Match { arms, .. } = &expr.kind {
        return arms
            .iter()
            .flat_map(|arm| expression_lists(&arm.body))
            .collect();
    }
    let loop_label = match &expr.kind {
        HirExprKind::Loop { label, .. } => Some(label.as_deref()),
        _ => None,
    };
    expression_blocks(expr)
        .into_iter()
        .map(|list| (list, loop_label))
        .collect()
}

/// How many `break` statements `stmts` holds through plain statement nesting. A `break` in
/// the expression a statement holds is counted by the walk over the body's expressions.
fn count_breaks(stmts: &[HirStmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            HirStmt::Break { .. } => 1,
            HirStmt::If { .. }
            | HirStmt::While { .. }
            | HirStmt::ForRange { .. }
            | HirStmt::ForEach { .. }
            | HirStmt::ValElse { .. } => nested_lists(stmt)
                .into_iter()
                .map(|(list, _)| count_breaks(list))
                .sum(),
            _ => 0,
        })
        .sum()
}

/// The initializer of the binding `name` names after `stmts`, when the last declaration
/// of it there is an immutable `val`. A `mut` binding may hold anything by then, and a
/// name a later `val ... else` or `const` binds is not the `val`'s.
fn local_initializer<'a>(stmts: &'a [HirStmt], name: &str) -> Option<&'a HirExpr> {
    stmts.iter().rev().find_map(|stmt| match stmt {
        HirStmt::VarDecl {
            name: declared,
            init,
            mutable,
            ..
        } if declared == name => Some(if *mutable { None } else { init.as_ref() }),
        HirStmt::ValElse { bindings, .. } if bindings.iter().any(|b| b.name == name) => Some(None),
        HirStmt::Const { name: declared, .. } if declared == name => Some(None),
        _ => None,
    })?
}

/// Every `return value` in a statement list, including the nested blocks control can
/// reach. A closure body is a lifted item of its own, so none of its returns is here.
fn collect_returns<'a>(stmts: &'a [HirStmt], out: &mut Vec<&'a HirExpr>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Return {
                value: Some(value), ..
            } => out.push(value),
            HirStmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                collect_returns(then_block, out);
                for (_, block) in else_if_blocks {
                    collect_returns(block, out);
                }
                if let Some(block) = else_block {
                    collect_returns(block, out);
                }
            }
            HirStmt::While { body, .. }
            | HirStmt::ForRange { body, .. }
            | HirStmt::ForEach { body, .. } => collect_returns(body, out),
            HirStmt::ValElse { else_block, .. } => collect_returns(else_block, out),
            _ => {}
        }
    }
}

/// Every `return value` in a statement list that belongs to an expression: a block, a
/// `loop`, an `if` or a `match` arm written where a value goes, at any depth. Each is an
/// exit of the function like a statement-level one. A closure is not walked into: its
/// body is a lifted item whose returns leave the closure.
fn collect_expression_returns<'a>(body: &'a [HirStmt], out: &mut Vec<&'a HirExpr>) {
    walk_stmts(body, &mut |expr| {
        for stmts in expression_blocks(expr) {
            collect_returns(stmts, out);
        }
    });
}

/// The statement lists `expr` itself holds: the blocks of an `if`, a block, an `unsafe`
/// block, a `pool` and a `loop` body. Not those of expressions nested in its operands.
fn expression_blocks(expr: &HirExpr) -> Vec<&[HirStmt]> {
    match &expr.kind {
        HirExprKind::If {
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => std::iter::once(then_block.as_slice())
            .chain(else_if_blocks.iter().map(|(_, block)| block.as_slice()))
            .chain(else_block.as_deref())
            .collect(),
        HirExprKind::Block { stmts }
        | HirExprKind::Unsafe { stmts }
        | HirExprKind::Pool { stmts, .. }
        | HirExprKind::Loop { body: stmts, .. } => vec![stmts.as_slice()],
        _ => Vec::new(),
    }
}

/// Whether `expr` always yields a freshly allocated buffer, reading a call against the
/// producers established so far.
///
/// The three expression shapes are the ones [`CodegenContext::produces_owned_string`]
/// recognises; this duplicates them rather than calling it because the summary is
/// computed before any LLVM context exists. The two stay in step through the tests that
/// drive both.
fn allocates(expr: &HirExpr, producers: &HashSet<String>) -> bool {
    match &expr.kind {
        HirExprKind::InterpString { .. } => true,
        HirExprKind::If {
            else_block: Some(_),
            ..
        }
        | HirExprKind::Match { .. }
        | HirExprKind::Block { .. } => {
            let mut tails = Vec::new();
            tail_exits(expr, &mut tails) && tails.iter().all(|tail| allocates(tail, producers))
        }
        HirExprKind::NewtypeConstruct { value, .. } => allocates(value, producers),
        HirExprKind::Binary {
            op: ast_types::BinaryOp::Add,
            ..
        } => matches!(expr.ty, HirType::String),
        HirExprKind::Call { callee, args } => match &callee.kind {
            HirExprKind::Variable(name) => producers.contains(name),
            HirExprKind::Path { type_name, member } => {
                producers.contains(&format!("{}__{}", type_name, member))
            }
            // A user method is keyed by the `Type__method` its call site mangles.
            HirExprKind::FieldAccess { object, field } => {
                if let HirType::Struct(type_name) | HirType::Enum(type_name) = object.ty.referent()
                {
                    return producers.contains(&format!("{}__{}", type_name, field));
                }
                // `StringBuilder::to_string` copies the builder's bytes into a buffer of their
                // own. The receiver's type is what identifies it: a user type that
                // declares its own `to_string` may return a `.rodata` literal, and reading
                // that as an allocation would hand `.rodata` to `free`.
                // `v.remove(i)` / `v.swap_remove(i)` hand the slot's own buffer out.
                let taken = matches!(field.as_str(), "remove" | "swap_remove")
                    && matches!(expr.ty, HirType::String)
                    && indexes_a_collection(&object.ty);
                taken
                    || (args.is_empty()
                        && ((field == TO_OWNED_METHOD && is_builder(&object.ty))
                            || (field == CLONE_METHOD
                                && matches!(object.ty.referent(), HirType::String))))
            }
            _ => false,
        },
        _ => false,
    }
}

/// Whether `ty` is one of the standard collections, through a borrow of it or directly.
fn indexes_a_collection(ty: &HirType) -> bool {
    match ty {
        HirType::Reference { inner, .. } => indexes_a_collection(inner),
        HirType::Collection { .. } => true,
        _ => false,
    }
}

/// Whether `ty` is the `StringBuilder` builder, through a borrow of it or directly.
fn is_builder(ty: &HirType) -> bool {
    match ty {
        HirType::Reference { inner, .. } => is_builder(inner),
        HirType::Collection { kind, .. } => matches!(kind, HirCollectionKind::StringBuilder),
        _ => false,
    }
}

/// Names a local binding, parameter, or loop variable takes anywhere in the program.
fn shadowed_names(items: &[HirItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for (_, params, body) in callables(items) {
        out.extend(params.iter().map(|(name, _)| (*name).to_string()));
        collect_bound_names(body, &mut out);
    }
    out
}

fn collect_bound_names(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            HirStmt::VarDecl { name, .. } | HirStmt::Const { name, .. } => {
                out.insert(name.clone());
            }
            HirStmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                collect_bound_names(then_block, out);
                for (_, block) in else_if_blocks {
                    collect_bound_names(block, out);
                }
                if let Some(block) = else_block {
                    collect_bound_names(block, out);
                }
            }
            HirStmt::While { body, .. } => collect_bound_names(body, out),
            HirStmt::ForRange {
                index,
                iterator,
                body,
                ..
            }
            | HirStmt::ForEach {
                index,
                iterator,
                body,
                ..
            } => {
                out.insert(iterator.clone());
                if let Some(index) = index {
                    out.insert(index.clone());
                }
                collect_bound_names(body, out);
            }
            HirStmt::ValElse {
                bindings,
                else_binding,
                else_block,
                ..
            } => {
                out.extend(bindings.iter().map(|b| b.name.clone()));
                out.extend(else_binding.iter().map(|b| b.name.clone()));
                collect_bound_names(else_block, out);
            }
            _ => {}
        }
    }
}

/// Whether a statement may leave the buffer behind `name` reachable after it runs.
///
/// The parameter a retention walk follows, and what it already knows about the
/// program's other parameters.
struct Param<'a> {
    name: &'a str,
    /// Parameters already proven read only, which a call may hand this one to.
    read_only: &'a HashSet<(String, usize)>,
    /// Top-level names a local binding shadows somewhere, which a call may not trust.
    shadowed: &'a HashSet<String>,
    /// Each trait's implementors, every one of which a call through a `&dyn` may reach.
    implementors: &'a HashMap<String, Vec<String>>,
}

/// A whitelist, not a blacklist: an occurrence of the parameter is safe only in a
/// position that is known to copy the bytes out, and every unrecognised position is a
/// retention. That is what makes an unhandled HIR shape leak rather than dangle.
fn stmt_retains(stmt: &HirStmt, name: &Param) -> bool {
    match stmt {
        HirStmt::Expr(expr) => retains(expr, name),
        HirStmt::VarDecl { init, .. } => init.as_ref().is_some_and(|e| retains(e, name)),
        // A returned value leaves the frame, so the parameter itself retains there; a
        // value merely READ out of it (`return s.len()`) does not.
        HirStmt::Return { value, .. } | HirStmt::Break { value, .. } => {
            value.as_ref().is_some_and(|e| retains(e, name))
        }
        HirStmt::Assign { place, value, .. }
        | HirStmt::TensorCompoundAssign { place, value, .. } => {
            place_mentions(place, name.name) || retains(value, name)
        }
        HirStmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            retains(condition, name)
                || then_block.iter().any(|s| stmt_retains(s, name))
                || else_if_blocks.iter().any(|(cond, block)| {
                    retains(cond, name) || block.iter().any(|s| stmt_retains(s, name))
                })
                || else_block
                    .as_ref()
                    .is_some_and(|block| block.iter().any(|s| stmt_retains(s, name)))
        }
        HirStmt::While {
            condition, body, ..
        } => retains(condition, name) || body.iter().any(|s| stmt_retains(s, name)),
        HirStmt::ForRange {
            start,
            end,
            step,
            body,
            ..
        } => {
            retains(start, name)
                || retains(end, name)
                || step.as_ref().is_some_and(|step| retains(step, name))
                || body.iter().any(|s| stmt_retains(s, name))
        }
        HirStmt::ForEach { iterable, body, .. } => {
            retains(iterable, name) || body.iter().any(|s| stmt_retains(s, name))
        }
        HirStmt::Continue { .. } => false,
        HirStmt::ValElse {
            scrutinee,
            else_block,
            ..
        } => retains(scrutinee, name) || else_block.iter().any(|s| stmt_retains(s, name)),
        HirStmt::Const { value, .. } => retains(value, name),
    }
}

/// Whether `expr` retains the buffer behind `name`, per the whitelist above.
fn retains(expr: &HirExpr, name: &Param) -> bool {
    match &expr.kind {
        // A bare occurrence in a position this function's callers did not whitelist.
        HirExprKind::Variable(other) => other == name.name,
        // `+`, `==` and `!=` copy their operands' bytes into the result or read only
        // the comparison out, so an operand is dead at the operator.
        HirExprKind::Binary { left, right, .. } => {
            read_retains(left, name) || read_retains(right, name)
        }
        // A rendered hole is memcpy'd into the joined buffer.
        HirExprKind::InterpString { parts } => parts.iter().any(|part| match part {
            HirInterpPart::Text(_) => false,
            HirInterpPart::Formatted { expr, .. } => read_retains(expr, name),
        }),
        HirExprKind::Call { callee, args } => call_retains(callee, args, name),
        // `as` converts between numeric types, so what it yields is never a buffer; only
        // its operand can hold on to one.
        HirExprKind::Cast { value } => retains(value, name),
        HirExprKind::Block { stmts } | HirExprKind::Unsafe { stmts } => {
            stmts.iter().any(|s| stmt_retains(s, name))
        }
        HirExprKind::Pool { stmts, .. } | HirExprKind::Loop { body: stmts, .. } => {
            stmts.iter().any(|s| stmt_retains(s, name))
        }
        HirExprKind::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
        } => {
            retains(condition, name)
                || then_block.iter().any(|s| stmt_retains(s, name))
                || else_if_blocks.iter().any(|(cond, block)| {
                    retains(cond, name) || block.iter().any(|s| stmt_retains(s, name))
                })
                || else_block
                    .as_ref()
                    .is_some_and(|block| block.iter().any(|s| stmt_retains(s, name)))
        }
        // Every remaining shape may store the fat pointer or hand back a view into it,
        // so any mention of the parameter under one is a retention.
        _ => mentions(expr, name.name),
    }
}

/// Whether a call retains the buffer behind `name` through its callee or its arguments.
fn call_retains(callee: &HirExpr, args: &[HirExpr], name: &Param) -> bool {
    match &callee.kind {
        // `print(s)` / `println(s)`: `emit` has consumed the bytes when it returns.
        HirExprKind::Variable(func) if IO_BUILTINS.contains(&func.as_str()) => {
            args.iter().any(|arg| read_retains(arg, name))
        }
        // `s.len()` takes the length word out; `b.push_str(s)` appends a copy. A method
        // this program declares reads an argument whose parameter is already known to be
        // read only, keyed `Type__method` as the call site mangles it. Every other method
        // may hand back a view (`.slice`, `.chars`) or store the pointer.
        HirExprKind::FieldAccess { object, field } => {
            let receiver_is_read = field == LEN_METHOD || field == CLONE_METHOD;
            let declared = match object.ty.referent() {
                HirType::Struct(type_name) | HirType::Enum(type_name) => {
                    Some(format!("{type_name}__{field}"))
                }
                _ => None,
            };
            let through_dyn = match object.ty.referent() {
                HirType::DynObject(trait_name) => Some(trait_name),
                _ => None,
            };
            let arg_is_read = |index: usize| {
                field == PUSH_STR_METHOD
                    || declared
                        .as_ref()
                        .is_some_and(|key| name.read_only.contains(&(key.clone(), index)))
                    || through_dyn.is_some_and(|trait_name| {
                        reads_through_dyn(
                            trait_name,
                            field,
                            index,
                            name.implementors,
                            name.read_only,
                        )
                    })
            };
            let receiver = if receiver_is_read {
                read_retains(object, name)
            } else {
                mentions(object, name.name)
            };
            receiver
                || args.iter().enumerate().any(|(index, arg)| {
                    if arg_is_read(index) {
                        read_retains(arg, name)
                    } else {
                        mentions(arg, name.name)
                    }
                })
        }
        // A function this program declares, whose own parameter in that position is
        // already known to be read only: the bytes are read during the call and kept by
        // nobody. A name a local shadows may be a closure, and is not trusted.
        HirExprKind::Variable(func) if !name.shadowed.contains(func) => {
            args.iter().enumerate().any(|(index, arg)| {
                if name.read_only.contains(&(func.clone(), index)) {
                    read_retains(arg, name)
                } else {
                    mentions(arg, name.name)
                }
            })
        }
        _ => mentions(callee, name.name) || args.iter().any(|arg| mentions(arg, name.name)),
    }
}

/// `retains`, for a slot whose direct occupant is read rather than stored: the
/// parameter itself is fine there, anything built around it is judged on its own.
fn read_retains(expr: &HirExpr, name: &Param) -> bool {
    match &expr.kind {
        HirExprKind::Variable(other) if other == name.name => false,
        // A view read in the slot that makes it (`s.slice(0..2).len()`) is gone once the
        // read is, and the parameter outlives the call; only its receiver is in question.
        HirExprKind::Call { callee, args } => match &callee.kind {
            HirExprKind::FieldAccess { object, field }
                if VIEW_METHODS.contains(&field.as_str()) =>
            {
                read_retains(object, name) || args.iter().any(|arg| retains(arg, name))
            }
            _ => retains(expr, name),
        },
        _ => retains(expr, name),
    }
}

/// Whether `name` occurs anywhere in `expr`. The conservative answer for a position
/// the whitelist does not recognise.
fn mentions(expr: &HirExpr, name: &str) -> bool {
    let mut found = false;
    walk(expr, &mut |e| {
        if matches!(&e.kind, HirExprKind::Variable(other) if other == name) {
            found = true;
        }
    });
    found
}

/// Whether `place` names `name` anywhere: as its root binding, or in an expression
/// it is built from.
fn place_mentions(place: &HirPlace, name: &str) -> bool {
    let mut found = matches!(place, HirPlace::Var { name: root, .. } if root == name);
    walk_place(place, &mut |e| {
        if matches!(&e.kind, HirExprKind::Variable(other) if other == name) {
            found = true;
        }
    });
    found
}

/// Apply `visit` to every expression a place is built from. The place itself is storage
/// rather than an expression, so it is not visited; walking it in place, instead of
/// through `HirPlace::to_expr`, spares a deep clone of its base on every assignment.
fn walk_place<'a>(place: &'a HirPlace, visit: &mut impl FnMut(&'a HirExpr)) {
    match place {
        HirPlace::Var { .. } => {}
        HirPlace::Field { object, .. } | HirPlace::TupleIndex { object, .. } => walk(object, visit),
        HirPlace::Index { object, index, .. } => {
            walk(object, visit);
            walk(index, visit);
        }
        HirPlace::TensorIndex { object, axes, .. } => {
            walk(object, visit);
            for axis in axes {
                if let HirTensorAxis::Position(position) = axis {
                    walk(position, visit);
                }
            }
        }
        HirPlace::Deref { pointer, .. } => walk(pointer, visit),
    }
}

/// Apply `visit` to `expr` and every expression under it, statements included.
fn walk<'a>(expr: &'a HirExpr, visit: &mut impl FnMut(&'a HirExpr)) {
    visit(expr);
    match &expr.kind {
        HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path { .. }
        | HirExprKind::GridPosition { .. }
        | HirExprKind::TensorIdentity
        | HirExprKind::CollectionNew { capacity: None } => {}
        HirExprKind::CollectionNew {
            capacity: Some(capacity),
        } => walk(capacity, visit),
        HirExprKind::Closure { .. } => {}
        HirExprKind::Binary { left, right, .. } => {
            walk(left, visit);
            walk(right, visit);
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Reference { operand, .. }
        | HirExprKind::Deref { operand } => walk(operand, visit),
        HirExprKind::Call { callee, args } => {
            walk(callee, visit);
            for arg in args {
                walk(arg, visit);
            }
        }
        HirExprKind::StructLiteral { fields, base, .. } => {
            for field in fields {
                walk(&field.value, visit);
            }
            if let Some(base) = base {
                walk(base, visit);
            }
        }
        HirExprKind::FieldAccess { object, .. }
        | HirExprKind::TupleIndex { object, .. }
        | HirExprKind::NewtypeAccess { object } => walk(object, visit),
        HirExprKind::Cast { value }
        | HirExprKind::DynCoerce { value }
        | HirExprKind::SliceCoerce { value }
        | HirExprKind::NewtypeConstruct { value, .. }
        | HirExprKind::TensorFill { value } => walk(value, visit),
        HirExprKind::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
        } => {
            walk(condition, visit);
            walk_stmts(then_block, visit);
            for (cond, block) in else_if_blocks {
                walk(cond, visit);
                walk_stmts(block, visit);
            }
            if let Some(block) = else_block {
                walk_stmts(block, visit);
            }
        }
        HirExprKind::Block { stmts }
        | HirExprKind::Unsafe { stmts }
        | HirExprKind::Pool { stmts, .. }
        | HirExprKind::Loop { body: stmts, .. }
        | HirExprKind::KernelPartition { body: stmts, .. } => walk_stmts(stmts, visit),
        HirExprKind::Range { start, end, .. } => {
            walk(start, visit);
            walk(end, visit);
        }
        HirExprKind::ArrayLiteral { elements }
        | HirExprKind::TensorLiteral { elements }
        | HirExprKind::TupleLiteral { elements } => {
            for element in elements {
                walk(element, visit);
            }
        }
        HirExprKind::TensorRandomNormal { mean, std } => {
            walk(mean, visit);
            walk(std, visit);
        }
        HirExprKind::Index { object, index } => {
            walk(object, visit);
            walk(index, visit);
        }
        HirExprKind::TensorIndex { object, axes } => {
            walk(object, visit);
            for axis in axes {
                if let HirTensorAxis::Position(position) = axis {
                    walk(position, visit);
                }
            }
        }
        HirExprKind::TensorShapeCast { receiver, .. }
        | HirExprKind::TensorDetach { receiver }
        | HirExprKind::TensorReduce { receiver, .. }
        | HirExprKind::TensorSort { receiver, .. } => walk(receiver, visit),
        HirExprKind::TensorApply {
            receiver,
            operand,
            callee,
            ..
        } => {
            walk(receiver, visit);
            if let Some(operand) = operand {
                walk(operand, visit);
            }
            walk(callee, visit);
        }
        HirExprKind::TensorEinsum { operands, .. } => {
            for operand in operands {
                walk(operand, visit);
            }
        }
        HirExprKind::Math {
            operand, exponent, ..
        } => {
            walk(operand, visit);
            if let Some(exponent) = exponent {
                walk(exponent, visit);
            }
        }
        HirExprKind::EnumConstruct { payload, .. } => {
            for field in payload {
                walk(field, visit);
            }
        }
        HirExprKind::ArrayRest { array, .. } => walk(array, visit),
        HirExprKind::Match { scrutinee, arms } => {
            walk(scrutinee, visit);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    walk(guard, visit);
                }
                walk(&arm.body, visit);
            }
        }
        HirExprKind::InterpString { parts } => {
            for part in parts {
                if let HirInterpPart::Formatted { expr, .. } = part {
                    walk(expr, visit);
                }
            }
        }
    }
}

fn walk_stmts<'a>(stmts: &'a [HirStmt], visit: &mut impl FnMut(&'a HirExpr)) {
    for stmt in stmts {
        match stmt {
            HirStmt::VarDecl { init, .. } => {
                if let Some(init) = init {
                    walk(init, visit);
                }
            }
            HirStmt::Assign { place, value, .. }
            | HirStmt::TensorCompoundAssign { place, value, .. } => {
                walk_place(place, visit);
                walk(value, visit);
            }
            HirStmt::Return { value, .. } | HirStmt::Break { value, .. } => {
                if let Some(value) = value {
                    walk(value, visit);
                }
            }
            HirStmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                walk(condition, visit);
                walk_stmts(then_block, visit);
                for (cond, block) in else_if_blocks {
                    walk(cond, visit);
                    walk_stmts(block, visit);
                }
                if let Some(block) = else_block {
                    walk_stmts(block, visit);
                }
            }
            HirStmt::While {
                condition, body, ..
            } => {
                walk(condition, visit);
                walk_stmts(body, visit);
            }
            HirStmt::ForRange {
                start,
                end,
                step,
                body,
                ..
            } => {
                walk(start, visit);
                walk(end, visit);
                if let Some(step) = step {
                    walk(step, visit);
                }
                walk_stmts(body, visit);
            }
            HirStmt::ForEach { iterable, body, .. } => {
                walk(iterable, visit);
                walk_stmts(body, visit);
            }
            HirStmt::ValElse {
                scrutinee,
                else_block,
                ..
            } => {
                walk(scrutinee, visit);
                walk_stmts(else_block, visit);
            }
            HirStmt::Const { value, .. } => walk(value, visit),
            HirStmt::Expr(expr) => walk(expr, visit),
            HirStmt::Continue { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The summary of `source`, after the literal-copy pre-pass `build_module` runs.
    fn summary(source: &str) -> StringOwnership {
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("lowering failed");
        let promoted = promote_literal_exits(&hir.items);
        analyze(promoted.as_deref().unwrap_or(&hir.items))
    }

    #[test]
    fn a_holder_filled_from_a_parameter_forwards_it_per_position() {
        let owned = summary(
            "struct P {\n    name: string,\n    n: i32\n}\nfunc wrap(s: string) -> P { P { name: s, n: 0 } }\nfunc pair(a: string, b: string) -> (string, string) { (a, b) }\nfunc half(s: string, c: bool) -> P {\n    if c { return P { name: s, n: 1 } }\n    P { name: \"lit\", n: 2 }\n}\nfunc echoed(s: string) -> P {\n    println(s)\n    P { name: s, n: 3 }\n}\nfunc main() -> i32 { 0 }\n",
        );
        assert_eq!(
            owned.held_strings_forwarded("wrap"),
            &[(vec!["name".to_string()], 0)]
        );
        assert_eq!(
            owned.held_strings_forwarded("pair"),
            &[(vec!["0".to_string()], 0), (vec!["1".to_string()], 1)]
        );
        // One exit stores a literal there, so the position is not the argument's.
        assert!(owned.held_strings_forwarded("half").is_empty());
        // Any other mention of the parameter may keep a second copy of its pointer.
        assert!(owned.held_strings_forwarded("echoed").is_empty());
    }

    #[test]
    fn a_call_through_a_trait_object_reads_what_every_implementor_reads() {
        let owned = summary(
            "trait Sink {\n    func eat(&self, s: string) -> i32\n}\nstruct A { k: i32 }\nstruct B { k: i32 }\nimpl Sink for A {\n    func eat(&self, s: string) -> i32 { s.len() as i32 }\n}\nimpl Sink for B {\n    func eat(&self, s: string) -> i32 {\n        println(s)\n        self.k\n    }\n}\ntrait Pass {\n    func pass(&self, s: string) -> string\n}\nimpl Pass for A {\n    func pass(&self, s: string) -> string { s }\n}\nimpl Pass for B {\n    func pass(&self, s: string) -> string { s + \"\" }\n}\nfunc relay(d: &dyn Sink, s: string) -> i32 { d.eat(s) }\nfunc main() -> i32 { 0 }\n",
        );
        assert!(owned.param_is_read_only(&dyn_callee("Sink", "eat"), 0));
        // `A` hands its argument back, so a call through the object may not release it.
        assert!(!owned.param_is_read_only(&dyn_callee("Pass", "pass"), 0));
        // A parameter passed on through the object is read, as through a direct call.
        assert!(owned.param_is_read_only("relay", 1));
    }

    #[test]
    fn a_view_read_where_it_is_made_does_not_retain_its_receiver() {
        let owned = summary(
            "func first(s: string) -> i32 { s.slice(0..2).len() as i32 }\nfunc shout(s: string) -> string { s.slice(0..2) + \"!\" }\nfunc show(s: string) { println(\"{s.char_slice(1..3)}\") }\nfunc keep(s: string) -> string {\n    val v = s.slice(0..2)\n    v + \"\"\n}\nfunc give(s: string) -> &string { s.slice(0..2) }\nfunc main() -> i32 { 0 }\n",
        );
        assert!(owned.param_is_read_only("first", 0));
        assert!(owned.param_is_read_only("shout", 0));
        assert!(owned.param_is_read_only("show", 0));
        // A view bound to a local, or handed back, may outlive the read.
        assert!(!owned.param_is_read_only("keep", 0));
        assert!(!owned.param_is_read_only("give", 0));
    }

    #[test]
    fn regression_bug_103_a_literal_beside_an_allocation_is_copied() {
        let owned = summary(
            "func label(k: i32) -> string {\n    match k {\n        0 => \"info\",\n        _ => \"warn {k}\",\n    }\n}\nfunc pick(f: bool, a: string) -> string {\n    if f { return \"none\" }\n    return a + \"x\"\n}\nfunc main() -> i32 { 0 }\n",
        );
        assert!(owned.returns_owned("label"));
        assert!(owned.returns_owned("pick"));
    }

    #[test]
    fn regression_bug_103_a_call_returning_only_literals_is_copied_like_one() {
        let owned = summary(
            "func flag(on: bool) -> string {\n    if on { return \"on\" }\n    return \"off\"\n}\nfunc describe(k: i32) -> string {\n    match k {\n        0 => flag(true),\n        _ => \"n {k}\",\n    }\n}\nfunc main() -> i32 { 0 }\n",
        );
        assert!(owned.returns_owned("describe"));
        // A function of constants alone allocates nothing, so its results never reach
        // `free`.
        assert!(!owned.returns_owned("flag"));
    }

    #[test]
    fn regression_bug_103_a_function_returning_only_literals_is_left_alone() {
        let source = "func lits(k: i32) -> string { if k == 0 { \"a\" } else { \"bb\" } }\nfunc main() -> i32 { 0 }\n";
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("lowering failed");
        assert!(promote_literal_exits(&hir.items).is_none());
        assert!(!analyze(&hir.items).returns_owned("lits"));
    }

    #[test]
    fn a_tail_naming_an_immutable_local_val_returns_its_initializer() {
        let owned = summary(
            "func build(i: i32) -> string {\n    val b = \"ab{i}\"\n    b\n}\nfunc branch(c: bool, i: i32) -> string {\n    val b = if c { \"a{i}\" } else { \"bb{i}\" }\n    b\n}\nfunc reassigned(i: i32) -> string {\n    mut b = \"ab{i}\"\n    b = \"lit\"\n    b\n}\nfunc main() -> i32 { 0 }\n",
        );
        assert!(owned.returns_owned("build"));
        assert!(owned.returns_owned("branch"));
        // A `mut` binding may hold a literal by the time the tail reads it.
        assert!(!owned.returns_owned("reassigned"));
    }

    #[test]
    fn a_loop_tail_exits_through_the_break_values_aimed_at_it() {
        let owned = summary(
            "func plain(a: string, b: string, c: bool) -> string {\n    loop {\n        if c { return a + b }\n        break a + \"!\"\n    }\n}\nfunc labeled(a: string, c: bool) -> string {\n    outer: loop {\n        loop {\n            if c { break outer a + \"?\" }\n            break\n        }\n        break outer a + \"!\"\n    }\n}\nfunc inner(a: string, n: i32) -> string {\n    loop {\n        val k = loop { if n > 0 { break 1 } }\n        while n < 0 { break }\n        break a + \"{k}\"\n    }\n}\nfunc arm(a: string, c: bool) -> string {\n    if c { loop { break a + \"p\" } } else { a + \"q\" }\n}\nfunc matched(a: string, n: i32) -> string {\n    loop {\n        match n {\n            0 => { break \"zero\" }\n            _ => { break a + \"m\" }\n        }\n    }\n}\nfunc main() -> i32 { 0 }\n",
        );
        for name in ["plain", "labeled", "inner", "arm", "matched"] {
            assert!(owned.returns_owned(name), "{name} should produce");
        }
    }

    #[test]
    fn a_loop_tail_with_a_break_that_does_not_allocate_is_not_a_producer() {
        let owned = summary(
            "func forwards(a: string, c: bool) -> string {\n    loop {\n        if c { return a + \"x\" }\n        break a\n    }\n}\nfunc valueless(a: string) -> string {\n    loop {\n        while true { break }\n        return a + \"x\"\n    }\n}\nfunc main() -> i32 { 0 }\n",
        );
        assert!(!owned.returns_owned("forwards"));
        // No `break` aims at the loop, so the `return` is the only exit.
        assert!(owned.returns_owned("valueless"));
    }

    #[test]
    fn a_literal_break_beside_an_allocation_is_copied_not_freed() {
        let source = "func f(a: string, c: bool) -> string {\n    loop {\n        if c { break \"none\" }\n        break a + \"!\"\n    }\n}\nfunc main() -> i32 { 0 }\n";
        let ast = syntax_parsing::parse(source).expect("parsing failed");
        let hir = hir_lowering::lower_program(&ast).expect("lowering failed");
        assert!(!analyze(&hir.items).returns_owned("f"));
        let promoted = promote_literal_exits(&hir.items).expect("the literal break is copied");
        assert!(analyze(&promoted).returns_owned("f"));
    }

    #[test]
    fn a_returned_holder_owns_only_the_positions_every_exit_allocates() {
        let owned = summary(
            "struct P { name: string, tag: string }\nfunc make(a: string) -> P { P { name: a + \"!\", tag: \"t\" } }\nfunc mixed(a: string, c: bool) -> P {\n    if c { return P { name: a + \"x\", tag: a + \"y\" } }\n    P { name: a + \"z\", tag: \"lit\" }\n}\nfunc chained(a: string) -> (P, string) { (make(a), \"q\") }\nfunc kept(p: P) -> P { p }\nfunc main() -> i32 { 0 }\n",
        );
        let name = [vec!["name".to_string()]];
        assert_eq!(owned.held_strings_returned("make"), name);
        assert_eq!(owned.held_strings_returned("mixed"), name);
        let nested: Vec<String> = ["0", "name"].map(String::from).to_vec();
        assert_eq!(owned.held_strings_returned("chained"), [nested]);
        assert!(owned.held_strings_returned("kept").is_empty());
    }
}
