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

use std::collections::HashSet;

use neuro_hir::{
    HirCollectionKind, HirExpr, HirExprKind, HirInterpPart, HirItem, HirStmt, HirTensorAxis,
    HirType,
};

/// The `String` builder method that copies its bytes out into an owned `string`, and
/// the `string` methods that read their receiver without retaining it.
const TO_OWNED_METHOD: &str = "to_string";
const LEN_METHOD: &str = "len";
const PUSH_STR_METHOD: &str = "push_str";

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
            .filter(|(name, _)| !returns_owned.contains(*name) && !shadowed.contains(*name))
            .filter(|(_, exits)| {
                !exits.is_empty() && exits.iter().all(|exit| allocates(exit, &returns_owned))
            })
            .map(|(name, _)| (*name).to_string())
            .collect();
        if grown.is_empty() {
            break;
        }
        returns_owned.extend(grown);
    }

    let mut read_only_params = HashSet::new();
    for (name, params, body) in callables(items) {
        for (index, param) in params.iter().enumerate() {
            if !matches!(param.1, HirType::String) {
                continue;
            }
            if body.iter().any(|stmt| stmt_retains(stmt, param.0)) {
                continue;
            }
            read_only_params.insert((name.clone(), index));
        }
    }

    StringOwnership {
        returns_owned,
        read_only_params,
    }
}

/// One callable a call site can name: the name it resolves to, its parameters, and the
/// body that says what happens to them.
///
/// A method is keyed by the `Type__method` its call site mangles, and its receiver is
/// not in the parameter list: `self` is passed separately and is never a `string`.
type Callable<'a> = (String, Vec<(&'a str, &'a HirType)>, &'a [HirStmt]);

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
            _ => {}
        }
    }
    out
}

/// The expressions a `string`-returning callable can leave through, by name.
///
/// A callable with an exit this cannot enumerate gets an empty list and so never
/// qualifies: the tail must be an expression statement, and every other exit an
/// explicit `return`. A tail that is an `if` or a `match` yields whatever its branch
/// yields, which is not a shape this reads through, so it answers the safe way.
fn string_returning_bodies(items: &[HirItem]) -> Vec<(&str, Vec<&HirExpr>)> {
    let mut out: Vec<(&str, Vec<&HirExpr>)> = Vec::new();
    for item in items {
        let (name, return_type, body) = match item {
            HirItem::Function(func) => (func.name.as_str(), &func.return_type, &func.body),
            _ => continue,
        };
        if !matches!(return_type, HirType::String) {
            continue;
        }
        let mut exits = Vec::new();
        collect_returns(body, &mut exits);
        if let Some(HirStmt::Expr(tail)) = body.last() {
            exits.push(tail);
        }
        out.push((name, exits));
    }
    out
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
        // A collection copies a `string` out of its slot, so the read owns the copy.
        HirExprKind::Index { object, .. } => {
            matches!(expr.ty, HirType::String) && indexes_a_collection(&object.ty)
        }
        HirExprKind::Binary {
            op: ast_types::BinaryOp::Add,
            ..
        } => matches!(expr.ty, HirType::String),
        HirExprKind::Call { callee, args } => match &callee.kind {
            HirExprKind::Variable(name) => args.is_empty() && producers.contains(name),
            // `String::to_string` copies the builder's bytes into a buffer of their
            // own. The receiver's type is what identifies it: a user type that declares
            // its own `to_string` may return a `.rodata` literal, and reading that as an
            // allocation would hand `.rodata` to `free`.
            HirExprKind::FieldAccess { object, field } => {
                args.is_empty() && field == TO_OWNED_METHOD && is_builder(&object.ty)
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

/// Whether `ty` is the `String` builder, through a borrow of it or directly.
fn is_builder(ty: &HirType) -> bool {
    match ty {
        HirType::Reference { inner, .. } => is_builder(inner),
        HirType::Collection { kind, .. } => matches!(kind, HirCollectionKind::String),
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
/// A whitelist, not a blacklist: an occurrence of the parameter is safe only in a
/// position that is known to copy the bytes out, and every unrecognised position is a
/// retention. That is what makes an unhandled HIR shape leak rather than dangle.
fn stmt_retains(stmt: &HirStmt, name: &str) -> bool {
    let span = shared_types::Span::new(0, 0);
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
            mentions(&place.to_expr(span), name) || retains(value, name)
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
            start, end, body, ..
        } => {
            retains(start, name) || retains(end, name) || body.iter().any(|s| stmt_retains(s, name))
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
fn retains(expr: &HirExpr, name: &str) -> bool {
    match &expr.kind {
        // A bare occurrence in a position this function's callers did not whitelist.
        HirExprKind::Variable(other) => other == name,
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
        _ => mentions(expr, name),
    }
}

/// Whether a call retains the buffer behind `name` through its callee or its arguments.
fn call_retains(callee: &HirExpr, args: &[HirExpr], name: &str) -> bool {
    match &callee.kind {
        // `print(s)` / `println(s)`: `emit` has consumed the bytes when it returns.
        HirExprKind::Variable(func) if IO_BUILTINS.contains(&func.as_str()) => {
            args.iter().any(|arg| read_retains(arg, name))
        }
        // `s.len()` takes the length word out; `b.push_str(s)` appends a copy. Every
        // other method may hand back a view (`.slice`, `.chars`) or store the pointer.
        HirExprKind::FieldAccess { object, field } => {
            let receiver_is_read = field == LEN_METHOD;
            let args_are_reads = field == PUSH_STR_METHOD;
            let receiver = if receiver_is_read {
                read_retains(object, name)
            } else {
                mentions(object, name)
            };
            receiver
                || args.iter().any(|arg| {
                    if args_are_reads {
                        read_retains(arg, name)
                    } else {
                        mentions(arg, name)
                    }
                })
        }
        _ => mentions(callee, name) || args.iter().any(|arg| mentions(arg, name)),
    }
}

/// `retains`, for a slot whose direct occupant is read rather than stored: the
/// parameter itself is fine there, anything built around it is judged on its own.
fn read_retains(expr: &HirExpr, name: &str) -> bool {
    if matches!(&expr.kind, HirExprKind::Variable(other) if other == name) {
        return false;
    }
    retains(expr, name)
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

/// Apply `visit` to `expr` and every expression under it, statements included.
fn walk(expr: &HirExpr, visit: &mut impl FnMut(&HirExpr)) {
    visit(expr);
    match &expr.kind {
        HirExprKind::Literal(_)
        | HirExprKind::Variable(_)
        | HirExprKind::Path { .. }
        | HirExprKind::TensorIdentity
        | HirExprKind::CollectionNew => {}
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
        | HirExprKind::Loop { body: stmts, .. } => walk_stmts(stmts, visit),
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
        | HirExprKind::TensorReduce { receiver, .. }
        | HirExprKind::TensorSort { receiver, .. } => walk(receiver, visit),
        HirExprKind::TensorEinsum { operands, .. } => {
            for operand in operands {
                walk(operand, visit);
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

fn walk_stmts(stmts: &[HirStmt], visit: &mut impl FnMut(&HirExpr)) {
    let span = shared_types::Span::new(0, 0);
    for stmt in stmts {
        match stmt {
            HirStmt::VarDecl { init, .. } => {
                if let Some(init) = init {
                    walk(init, visit);
                }
            }
            HirStmt::Assign { place, value, .. } => {
                walk(&place.to_expr(span), visit);
                walk(value, visit);
            }
            HirStmt::TensorCompoundAssign { place, value, .. } => {
                walk(&place.to_expr(span), visit);
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
                start, end, body, ..
            } => {
                walk(start, visit);
                walk(end, visit);
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
