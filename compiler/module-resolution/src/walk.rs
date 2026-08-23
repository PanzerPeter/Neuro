//! One mutable traversal reaching every place a module-qualified name can be written.
//!
//! Discovery and rewriting are the same walk with different callbacks — writing it twice
//! is how the two would drift apart and leave a qualifier standing in a corner of the
//! grammar nobody re-checked.

use ast_types::{
    ClosureParam, EnumPatternPayload, Expr, GenericArg, GenericParamKind, Item, MatchArm,
    MethodDef, Pattern, Stmt, Type, VariantPayload,
};
use shared_types::Identifier;

use crate::ModuleError;

/// A place in the AST a qualified or imported name can appear.
pub(crate) enum Site<'a> {
    /// An `Expr::Path`, `Expr::EnumStructLiteral`, or bare `Expr::Identifier`. Handed over
    /// whole because resolving the name can change which node it is: `geometry::Point { x: 1.0 }`
    /// parses as a struct-variant construction and resolves to a plain struct literal.
    Expr(&'a mut Expr),
    /// The name of a `Type::Named` or `Type::Generic`.
    TypeName(&'a mut Identifier),
    /// A `match` / `val-else` pattern that names a variant — qualified, imported, or (for a
    /// payload-less variant) still indistinguishable from a binding.
    Pattern(&'a mut Pattern),
}

/// The callback shape both passes use. `dyn` rather than a generic parameter: the walk is
/// deeply recursive and every level would otherwise be monomorphized twice.
pub(crate) type SiteFn<'f> = &'f mut dyn FnMut(Site<'_>) -> Result<(), ModuleError>;

pub(crate) fn walk_items(items: &mut [Item], f: SiteFn) -> Result<(), ModuleError> {
    for item in items {
        walk_item(item, f)?;
    }
    Ok(())
}

fn walk_item(item: &mut Item, f: SiteFn) -> Result<(), ModuleError> {
    match item {
        Item::Function(def) => {
            for param in &mut def.generics {
                if let GenericParamKind::Const(ty) = &mut param.kind {
                    walk_type(ty, f)?;
                }
            }
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f)?;
            }
            for param in &mut def.params {
                walk_type(&mut param.ty, f)?;
            }
            if let Some(ret) = &mut def.return_type {
                walk_type(ret, f)?;
            }
            walk_stmts(&mut def.body, f)
        }
        Item::Struct(def) => {
            for param in &mut def.generics {
                if let GenericParamKind::Const(ty) = &mut param.kind {
                    walk_type(ty, f)?;
                }
            }
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f)?;
            }
            for field in &mut def.fields {
                walk_type(&mut field.ty, f)?;
            }
            Ok(())
        }
        Item::Enum(def) => {
            for param in &mut def.generics {
                if let GenericParamKind::Const(ty) = &mut param.kind {
                    walk_type(ty, f)?;
                }
            }
            for variant in &mut def.variants {
                match &mut variant.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(types) => {
                        for ty in types {
                            walk_type(ty, f)?;
                        }
                    }
                    VariantPayload::Struct(fields) => {
                        for field in fields {
                            walk_type(&mut field.ty, f)?;
                        }
                    }
                }
            }
            Ok(())
        }
        Item::Trait(def) => {
            for method in &mut def.methods {
                for param in &mut method.params {
                    walk_type(&mut param.ty, f)?;
                }
                if let Some(ret) = &mut method.return_type {
                    walk_type(ret, f)?;
                }
                if let Some(body) = &mut method.default_body {
                    walk_stmts(body, f)?;
                }
            }
            Ok(())
        }
        Item::Impl(def) => {
            for param in &mut def.generics {
                if let GenericParamKind::Const(ty) = &mut param.kind {
                    walk_type(ty, f)?;
                }
            }
            for ty in &mut def.type_args {
                walk_type(ty, f)?;
            }
            for predicate in &mut def.where_predicates {
                walk_expr(predicate, f)?;
            }
            for (_, ty) in &mut def.assoc_types {
                walk_type(ty, f)?;
            }
            for method in &mut def.methods {
                walk_method(method, f)?;
            }
            Ok(())
        }
        Item::Const(def) => {
            walk_type(&mut def.ty, f)?;
            walk_expr(&mut def.value, f)
        }
        Item::Newtype(def) => walk_type(&mut def.inner, f),
        // Imports are lifted out of the item list as each module is loaded, so the walk
        // never meets one.
        // An import is consumed at load time, and an inline block is lifted into a module
        // of its own there too — neither survives to be walked.
        Item::Import(_) | Item::Module(_) => Ok(()),
    }
}

fn walk_method(method: &mut MethodDef, f: SiteFn) -> Result<(), ModuleError> {
    for param in &mut method.params {
        walk_type(&mut param.ty, f)?;
    }
    if let Some(ret) = &mut method.return_type {
        walk_type(ret, f)?;
    }
    walk_stmts(&mut method.body, f)
}

fn walk_type(ty: &mut Type, f: SiteFn) -> Result<(), ModuleError> {
    match ty {
        Type::Named(name) => f(Site::TypeName(name)),
        Type::Generic { name, args, .. } => {
            f(Site::TypeName(name))?;
            for arg in args {
                if let GenericArg::Type(inner) = arg {
                    walk_type(inner, f)?;
                }
            }
            Ok(())
        }
        Type::Reference { inner, .. } => walk_type(inner, f),
        Type::Array { element, .. } => walk_type(element, f),
        Type::Tuple { elements, .. } => {
            for element in elements {
                walk_type(element, f)?;
            }
            Ok(())
        }
        Type::Function { params, ret, .. } => {
            for param in params {
                walk_type(param, f)?;
            }
            walk_type(ret, f)
        }
        Type::Tensor { element_type, .. } => walk_type(element_type, f),
        // A trait name is not a value namespace, and `impl mod::Trait` does not parse.
        Type::ImplTrait { .. } | Type::DynTrait { .. } => Ok(()),
    }
}

fn walk_stmts(stmts: &mut [Stmt], f: SiteFn) -> Result<(), ModuleError> {
    for stmt in stmts {
        walk_stmt(stmt, f)?;
    }
    Ok(())
}

fn walk_stmt(stmt: &mut Stmt, f: SiteFn) -> Result<(), ModuleError> {
    match stmt {
        Stmt::VarDecl { ty, init, .. } => {
            if let Some(ty) = ty {
                walk_type(ty, f)?;
            }
            if let Some(init) = init {
                walk_expr(init, f)?;
            }
            Ok(())
        }
        Stmt::Assignment { value, .. } => walk_expr(value, f),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                walk_expr(value, f)?;
            }
            Ok(())
        }
        Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            walk_expr(condition, f)?;
            walk_stmts(then_block, f)?;
            for (cond, block) in else_if_blocks {
                walk_expr(cond, f)?;
                walk_stmts(block, f)?;
            }
            if let Some(block) = else_block {
                walk_stmts(block, f)?;
            }
            Ok(())
        }
        Stmt::While {
            condition, body, ..
        } => {
            walk_expr(condition, f)?;
            walk_stmts(body, f)
        }
        Stmt::ForRange {
            start, end, body, ..
        } => {
            walk_expr(start, f)?;
            walk_expr(end, f)?;
            walk_stmts(body, f)
        }
        Stmt::ForEach { iterable, body, .. } => {
            walk_expr(iterable, f)?;
            walk_stmts(body, f)
        }
        Stmt::Break { value, .. } => {
            if let Some(value) = value {
                walk_expr(value, f)?;
            }
            Ok(())
        }
        Stmt::Continue { .. } => Ok(()),
        Stmt::FieldAssignment { value, .. } => walk_expr(value, f),
        Stmt::DerefAssignment { pointer, value, .. } => {
            walk_expr(pointer, f)?;
            walk_expr(value, f)
        }
        Stmt::IndexAssignment { index, value, .. } => {
            walk_expr(index, f)?;
            walk_expr(value, f)
        }
        Stmt::ValElse {
            pattern,
            value,
            else_block,
            ..
        } => {
            walk_pattern(pattern, f)?;
            walk_expr(value, f)?;
            walk_stmts(else_block, f)
        }
        Stmt::Const { ty, value, .. } => {
            walk_type(ty, f)?;
            walk_expr(value, f)
        }
        Stmt::Expr(expr) => walk_expr(expr, f),
    }
}

fn walk_expr(expr: &mut Expr, f: SiteFn) -> Result<(), ModuleError> {
    // The callback runs before the descent: it may replace this node, and the replacement's
    // children still need walking.
    if matches!(
        expr,
        Expr::Path { .. } | Expr::EnumStructLiteral { .. } | Expr::Identifier(_)
    ) {
        f(Site::Expr(expr))?;
    }

    match expr {
        Expr::Literal(_, _) | Expr::Identifier(_) => Ok(()),
        Expr::Binary { left, right, .. } => {
            walk_expr(left, f)?;
            walk_expr(right, f)
        }
        Expr::Call {
            func,
            type_args,
            args,
            ..
        } => {
            walk_expr(func, f)?;
            for arg in type_args {
                if let GenericArg::Type(ty) = arg {
                    walk_type(ty, f)?;
                }
            }
            for arg in args {
                walk_expr(arg, f)?;
            }
            Ok(())
        }
        Expr::Unary { operand, .. } => walk_expr(operand, f),
        Expr::Paren(inner, _) => walk_expr(inner, f),
        Expr::StructLiteral { fields, base, .. } => {
            for field in fields {
                walk_expr(&mut field.value, f)?;
            }
            if let Some(base) = base {
                walk_expr(base, f)?;
            }
            Ok(())
        }
        Expr::FieldAccess { object, .. } => walk_expr(object, f),
        Expr::EnumStructLiteral { fields, .. } => {
            for field in fields {
                walk_expr(&mut field.value, f)?;
            }
            Ok(())
        }
        Expr::Path { .. } => Ok(()),
        Expr::Cast {
            expr, target_type, ..
        } => {
            walk_expr(expr, f)?;
            walk_type(target_type, f)
        }
        Expr::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            walk_expr(condition, f)?;
            walk_stmts(then_block, f)?;
            for (cond, block) in else_if_blocks {
                walk_expr(cond, f)?;
                walk_stmts(block, f)?;
            }
            if let Some(block) = else_block {
                walk_stmts(block, f)?;
            }
            Ok(())
        }
        Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } | Expr::Loop { body: stmts, .. } => {
            walk_stmts(stmts, f)
        }
        Expr::Reference { operand, .. } | Expr::Deref { operand, .. } => walk_expr(operand, f),
        Expr::Range { start, end, .. } => {
            walk_expr(start, f)?;
            walk_expr(end, f)
        }
        Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
            for element in elements {
                walk_expr(element, f)?;
            }
            Ok(())
        }
        Expr::Index { object, index, .. } => {
            walk_expr(object, f)?;
            walk_expr(index, f)
        }
        Expr::TupleIndex { object, .. } => walk_expr(object, f),
        Expr::ArrayRest { array, .. } => walk_expr(array, f),
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, f)?;
            for arm in arms {
                walk_arm(arm, f)?;
            }
            Ok(())
        }
        Expr::Closure {
            params, ret, body, ..
        } => {
            for param in params.iter_mut() {
                walk_closure_param(param, f)?;
            }
            if let Some(ret) = ret {
                walk_type(ret, f)?;
            }
            walk_expr(body, f)
        }
        Expr::Try { operand, .. } => walk_expr(operand, f),
    }
}

fn walk_arm(arm: &mut MatchArm, f: SiteFn) -> Result<(), ModuleError> {
    for pattern in &mut arm.patterns {
        walk_pattern(pattern, f)?;
    }
    if let Some(guard) = &mut arm.guard {
        walk_expr(guard, f)?;
    }
    walk_expr(&mut arm.body, f)
}

fn walk_pattern(pattern: &mut Pattern, f: SiteFn) -> Result<(), ModuleError> {
    // As in `walk_expr`, the callback runs first: resolving an imported variant turns a
    // binding or an unqualified variant into a `Pattern::Enum`, whose payload still needs
    // walking afterwards.
    f(Site::Pattern(pattern))?;

    let payload = match pattern {
        Pattern::Wildcard(_) | Pattern::Binding(_) | Pattern::Literal(_, _) => return Ok(()),
        Pattern::Range { .. } => return Ok(()),
        Pattern::Enum { payload, .. } | Pattern::UnqualifiedEnum { payload, .. } => payload,
    };
    match payload {
        EnumPatternPayload::Unit => Ok(()),
        EnumPatternPayload::Tuple(subs) => {
            for sub in subs {
                walk_pattern(sub, f)?;
            }
            Ok(())
        }
        EnumPatternPayload::Struct(fields) => {
            for field in fields {
                walk_pattern(&mut field.pattern, f)?;
            }
            Ok(())
        }
    }
}

fn walk_closure_param(param: &mut ClosureParam, f: SiteFn) -> Result<(), ModuleError> {
    match &mut param.ty {
        Some(ty) => walk_type(ty, f),
        None => Ok(()),
    }
}
