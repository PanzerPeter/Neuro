// One mutable traversal reaching every call expression in a program.
//
// A call this walk misses keeps its labels, and the type checker after it would then
// match arguments against parameters in the order they were written — the one way a
// named argument could bind to the wrong parameter instead of failing loudly. So the
// walk visits every expression position, not only the ones a call is usually written in.

use ast_types::{Expr, Item, MatchArm, Stmt};

use crate::binding::Bound;
use crate::errors::ArgumentError;

/// The callback shape. `dyn` rather than a generic parameter: the walk is deeply
/// recursive and every level would otherwise be monomorphized per closure. It answers
/// with what it did to the call, which decides how the walk continues through it.
pub(crate) type CallFn<'f> = &'f mut dyn FnMut(&mut Expr, &mut Vec<ArgumentError>) -> Bound;

pub(crate) fn walk_items(items: &mut [Item], f: CallFn, errors: &mut Vec<ArgumentError>) {
    for item in items {
        walk_item(item, f, errors);
    }
}

fn walk_item(item: &mut Item, f: CallFn, errors: &mut Vec<ArgumentError>) {
    match item {
        Item::Function(def) => {
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f, errors);
            }
            walk_stmts(&mut def.body, f, errors);
        }
        Item::Struct(def) => {
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f, errors);
            }
        }
        Item::Impl(def) => {
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f, errors);
            }
            for method in &mut def.methods {
                walk_stmts(&mut method.body, f, errors);
            }
        }
        Item::Trait(def) => {
            for method in &mut def.methods {
                if let Some(body) = &mut method.default_body {
                    walk_stmts(body, f, errors);
                }
            }
        }
        Item::Const(def) => walk_expr(&mut def.value, f, errors),
        Item::Module(def) => walk_items(&mut def.items, f, errors),
        Item::Enum(_) | Item::Newtype(_) | Item::Import(_) | Item::NoPrelude(_) => {}
    }
}

fn walk_stmts(stmts: &mut [Stmt], f: CallFn, errors: &mut Vec<ArgumentError>) {
    for stmt in stmts {
        walk_stmt(stmt, f, errors);
    }
}

fn walk_stmt(stmt: &mut Stmt, f: CallFn, errors: &mut Vec<ArgumentError>) {
    match stmt {
        Stmt::VarDecl { init, .. } => {
            if let Some(init) = init {
                walk_expr(init, f, errors);
            }
        }
        Stmt::Assignment { value, .. }
        | Stmt::FieldAssignment { value, .. }
        | Stmt::Const { value, .. } => walk_expr(value, f, errors),
        Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
            if let Some(value) = value {
                walk_expr(value, f, errors);
            }
        }
        Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            walk_expr(condition, f, errors);
            walk_stmts(then_block, f, errors);
            for (cond, block) in else_if_blocks {
                walk_expr(cond, f, errors);
                walk_stmts(block, f, errors);
            }
            if let Some(block) = else_block {
                walk_stmts(block, f, errors);
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            walk_expr(condition, f, errors);
            walk_stmts(body, f, errors);
        }
        Stmt::ForRange {
            start, end, body, ..
        } => {
            walk_expr(start, f, errors);
            walk_expr(end, f, errors);
            walk_stmts(body, f, errors);
        }
        Stmt::ForEach { iterable, body, .. } => {
            walk_expr(iterable, f, errors);
            walk_stmts(body, f, errors);
        }
        Stmt::DerefAssignment { pointer, value, .. } => {
            walk_expr(pointer, f, errors);
            walk_expr(value, f, errors);
        }
        Stmt::IndexAssignment { index, value, .. } => {
            walk_expr(index, f, errors);
            walk_expr(value, f, errors);
        }
        Stmt::ValElse {
            value, else_block, ..
        } => {
            walk_expr(value, f, errors);
            walk_stmts(else_block, f, errors);
        }
        Stmt::Continue { .. } => {}
        Stmt::Expr(expr) => walk_expr(expr, f, errors),
    }
}

fn walk_expr(expr: &mut Expr, f: CallFn, errors: &mut Vec<ArgumentError>) {
    // A call is bound before its arguments are walked: binding only reorders them, so a
    // nested call is reached either way, and reporting the outer call first puts the
    // diagnostics in source order.
    if matches!(expr, Expr::Call { .. }) && f(expr, errors) == Bound::Hoisted {
        // The call is now a block binding its arguments to temporaries, and the call it
        // ends with is already bound. Only the initializers are walked: visiting that
        // call again would re-bind a call whose labels are gone, which is exactly the
        // shape a required label rejects.
        if let Expr::Block { stmts, .. } = expr {
            for stmt in stmts {
                if let Stmt::VarDecl {
                    init: Some(init), ..
                } = stmt
                {
                    walk_expr(init, f, errors);
                }
            }
        }
        return;
    }

    match expr {
        Expr::Literal(_, _) | Expr::Identifier(_) | Expr::Path { .. } => {}
        Expr::Binary { left, right, .. } => {
            walk_expr(left, f, errors);
            walk_expr(right, f, errors);
        }
        Expr::Call { func, args, .. } => {
            walk_expr(func, f, errors);
            for arg in args {
                walk_expr(arg, f, errors);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::Reference { operand, .. }
        | Expr::Deref { operand, .. }
        | Expr::Try { operand, .. } => walk_expr(operand, f, errors),
        Expr::Paren(inner, _) => walk_expr(inner, f, errors),
        Expr::InterpString { parts, .. } => {
            for part in parts {
                if let ast_types::InterpPart::Formatted { expr, .. } = part {
                    walk_expr(expr, f, errors);
                }
            }
        }
        Expr::StructLiteral { fields, base, .. } => {
            for field in fields {
                walk_expr(&mut field.value, f, errors);
            }
            if let Some(base) = base {
                walk_expr(base, f, errors);
            }
        }
        Expr::EnumStructLiteral { fields, .. } => {
            for field in fields {
                walk_expr(&mut field.value, f, errors);
            }
        }
        Expr::FieldAccess { object, .. }
        | Expr::TupleIndex { object, .. }
        | Expr::ArrayRest { array: object, .. } => walk_expr(object, f, errors),
        Expr::Cast { expr, .. } => walk_expr(expr, f, errors),
        Expr::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            walk_expr(condition, f, errors);
            walk_stmts(then_block, f, errors);
            for (cond, block) in else_if_blocks {
                walk_expr(cond, f, errors);
                walk_stmts(block, f, errors);
            }
            if let Some(block) = else_block {
                walk_stmts(block, f, errors);
            }
        }
        Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } | Expr::Loop { body: stmts, .. } => {
            walk_stmts(stmts, f, errors)
        }
        Expr::Range { start, end, .. } => {
            walk_expr(start, f, errors);
            walk_expr(end, f, errors);
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                walk_expr(element, f, errors);
            }
        }
        Expr::Index { object, index, .. } => {
            walk_expr(object, f, errors);
            walk_expr(index, f, errors);
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, f, errors);
            for arm in arms {
                walk_arm(arm, f, errors);
            }
        }
        Expr::Closure { body, .. } => walk_expr(body, f, errors),
    }
}

fn walk_arm(arm: &mut MatchArm, f: CallFn, errors: &mut Vec<ArgumentError>) {
    if let Some(guard) = &mut arm.guard {
        walk_expr(guard, f, errors);
    }
    walk_expr(&mut arm.body, f, errors);
}
