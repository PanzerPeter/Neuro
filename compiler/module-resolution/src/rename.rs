//! Alpha-renaming a provided trait method's own generic parameters as it is copied into an
//! `impl` that declares the same names.
//!
//! The copy would otherwise give one name two meanings inside the impl, the impl's parameter
//! and the method's. Every occurrence of a renamed name in the method is rewritten, binders
//! included, to a spelling no source can write (`T'`). So nothing the body wrote can be
//! captured by the new name, and a local that shadowed the old one still shadows the new one.
//! Field, method, member and label names are a different namespace and are left alone.

use std::collections::HashMap;

use ast_types::{
    ArraySize, EnumPatternPayload, Expr, GenericArg, GenericParam, GenericParamKind, InterpPart,
    MethodDef, Pattern, Place, Stmt, TensorDim, TensorExtent, TensorIndexArg, Type,
};
use shared_types::Identifier;

/// Rename each of `method`'s generic parameters that `taken` also names, everywhere in it.
///
/// A value parameter spelled like one keeps both names: renaming it would rename the label a
/// caller writes, so that clash is left for the checker to report.
pub(crate) fn rename_clashing_generics(method: &mut MethodDef, taken: &[GenericParam]) {
    let names: HashMap<String, String> = method
        .generics
        .iter()
        .filter(|own| taken.iter().any(|t| t.name.name == own.name.name))
        .filter(|own| method.params.iter().all(|p| p.name.name != own.name.name))
        .map(|own| (own.name.name.clone(), format!("{}'", own.name.name)))
        .collect();
    if names.is_empty() {
        return;
    }
    let renamer = Renamer { names };
    renamer.generics(&mut method.generics);
    for predicate in &mut method.where_predicates {
        renamer.expr(predicate);
    }
    for param in &mut method.params {
        renamer.ty(&mut param.ty);
    }
    if let Some(ret) = &mut method.return_type {
        renamer.ty(ret);
    }
    renamer.stmts(&mut method.body);
}

/// Rename each type parameter `names` maps, everywhere in `ty`.
pub(crate) fn rename_type(ty: &mut Type, names: HashMap<String, String>) {
    Renamer { names }.ty(ty);
}

struct Renamer {
    names: HashMap<String, String>,
}

impl Renamer {
    /// A name, or a path whose first segment is the name (`T::Item`, `T::zero`).
    fn ident(&self, ident: &mut Identifier) {
        let (head, rest) = match ident.name.split_once("::") {
            Some((head, rest)) => (head, Some(rest)),
            None => (ident.name.as_str(), None),
        };
        if let Some(new) = self.names.get(head) {
            ident.name = match rest {
                Some(rest) => format!("{new}::{rest}"),
                None => new.clone(),
            };
        }
    }

    fn generics(&self, generics: &mut [GenericParam]) {
        for generic in generics {
            self.ident(&mut generic.name);
            if let GenericParamKind::Const(ty) = &mut generic.kind {
                self.ty(ty);
            }
            for bound in &mut generic.bounds {
                for (_, ty) in &mut bound.assoc_bindings {
                    self.ty(ty);
                }
            }
        }
    }

    fn generic_args(&self, args: &mut [GenericArg]) {
        for arg in args {
            match arg {
                GenericArg::Type(ty) => self.ty(ty),
                GenericArg::Shape { dims, .. } => self.shape(dims),
                GenericArg::Const { .. } => {}
            }
        }
    }

    fn shape(&self, dims: &mut [TensorDim]) {
        for dim in dims {
            match &mut dim.extent {
                TensorExtent::Param(name)
                | TensorExtent::Splice(name)
                | TensorExtent::NamedDynamic(name) => self.ident(name),
                TensorExtent::Expr(expr) => self.expr(expr),
                TensorExtent::Literal(_) | TensorExtent::Dynamic(_) => {}
            }
        }
    }

    fn ty(&self, ty: &mut Type) {
        match ty {
            Type::Named(name) => self.ident(name),
            Type::Generic { name, args, .. } => {
                self.ident(name);
                self.generic_args(args);
            }
            Type::Reference { inner, .. } => self.ty(inner),
            Type::Array { element, size, .. } => {
                self.ty(element);
                if let ArraySize::Const(name) = size {
                    self.ident(name);
                }
            }
            Type::Slice { element, .. } => self.ty(element),
            Type::Tuple { elements, .. } => elements.iter_mut().for_each(|e| self.ty(e)),
            Type::Function { params, ret, .. } => {
                params.iter_mut().for_each(|p| self.ty(p));
                self.ty(ret);
            }
            Type::Tensor {
                element_type,
                shape,
                ..
            } => {
                self.ty(element_type);
                self.shape(shape);
            }
            Type::ImplTrait { assoc_bindings, .. } => {
                for (_, ty) in assoc_bindings {
                    self.ty(ty);
                }
            }
            Type::DynTrait { .. } => {}
        }
    }

    fn stmts(&self, stmts: &mut [Stmt]) {
        stmts.iter_mut().for_each(|stmt| self.stmt(stmt));
    }

    fn stmt(&self, stmt: &mut Stmt) {
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                self.ident(name);
                if let Some(ty) = ty {
                    self.ty(ty);
                }
                if let Some(init) = init {
                    self.expr(init);
                }
            }
            Stmt::Assign { place, value, .. } => {
                self.place(place);
                self.expr(value);
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expr(value);
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.branches(condition, then_block, else_if_blocks, else_block),
            Stmt::While {
                condition, body, ..
            } => {
                self.expr(condition);
                self.stmts(body);
            }
            Stmt::ForRange {
                index,
                iterator,
                start,
                end,
                step,
                adapters,
                body,
                ..
            } => {
                if let Some(index) = index {
                    self.ident(index);
                }
                self.ident(iterator);
                self.expr(start);
                self.expr(end);
                if let Some(step) = step {
                    self.expr(step);
                }
                adapters.iter_mut().for_each(|a| self.expr(&mut a.callee));
                self.stmts(body);
            }
            Stmt::ForEach {
                index,
                iterator,
                iterable,
                adapters,
                body,
                ..
            } => {
                if let Some(index) = index {
                    self.ident(index);
                }
                self.ident(iterator);
                self.expr(iterable);
                adapters.iter_mut().for_each(|a| self.expr(&mut a.callee));
                self.stmts(body);
            }
            Stmt::ValElse {
                pattern,
                ty,
                value,
                else_binding,
                else_block,
                ..
            } => {
                self.pattern(pattern);
                if let Some(ty) = ty {
                    self.ty(ty);
                }
                self.expr(value);
                if let Some(binding) = else_binding {
                    self.ident(binding);
                }
                self.stmts(else_block);
            }
            Stmt::Const {
                name, ty, value, ..
            } => {
                self.ident(name);
                self.ty(ty);
                self.expr(value);
            }
            Stmt::Expr(expr) => self.expr(expr),
            Stmt::Continue { .. } => {}
        }
    }

    fn branches(
        &self,
        condition: &mut Expr,
        then_block: &mut [Stmt],
        else_if_blocks: &mut [(Expr, Vec<Stmt>)],
        else_block: &mut Option<Vec<Stmt>>,
    ) {
        self.expr(condition);
        self.stmts(then_block);
        for (condition, block) in else_if_blocks {
            self.expr(condition);
            self.stmts(block);
        }
        if let Some(block) = else_block {
            self.stmts(block);
        }
    }

    fn place(&self, place: &mut Place) {
        match place {
            Place::Var(name) => self.ident(name),
            Place::Field { object, .. }
            | Place::TupleIndex { object, .. }
            | Place::Deref {
                pointer: object, ..
            } => self.expr(object),
            Place::Index { object, index, .. } => {
                self.expr(object);
                self.expr(index);
            }
            Place::TensorIndex {
                object, indices, ..
            } => {
                self.expr(object);
                indices.iter_mut().for_each(|i| self.index_arg(i));
            }
        }
    }

    fn index_arg(&self, index: &mut TensorIndexArg) {
        match index {
            TensorIndexArg::Position(expr) => self.expr(expr),
            TensorIndexArg::Range {
                start, end, step, ..
            } => {
                self.expr(start);
                self.expr(end);
                if let Some(step) = step {
                    self.expr(step);
                }
            }
            TensorIndexArg::FullAxis(_) => {}
        }
    }

    fn pattern(&self, pattern: &mut Pattern) {
        match pattern {
            Pattern::Binding(name) => self.ident(name),
            Pattern::At { name, pattern, .. } => {
                self.ident(name);
                self.pattern(pattern);
            }
            Pattern::Enum { payload, .. } | Pattern::UnqualifiedEnum { payload, .. } => {
                match payload {
                    EnumPatternPayload::Unit => {}
                    EnumPatternPayload::Tuple(patterns) => {
                        patterns.iter_mut().for_each(|p| self.pattern(p));
                    }
                    EnumPatternPayload::Struct(fields) => {
                        fields.iter_mut().for_each(|f| self.pattern(&mut f.pattern));
                    }
                }
            }
            Pattern::Array { elements, rest, .. } => {
                elements.iter_mut().for_each(|p| self.pattern(p));
                if let Some(ast_types::ArrayPatternRest {
                    binding: Some(name),
                    ..
                }) = rest
                {
                    self.ident(name);
                }
            }
            Pattern::Wildcard(_) | Pattern::Literal(..) | Pattern::Range { .. } => {}
        }
    }

    fn expr(&self, expr: &mut Expr) {
        match expr {
            Expr::Identifier(name) => self.ident(name),
            Expr::Path { type_name, .. } => self.ident(type_name),
            Expr::Literal(..) => {}
            Expr::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Call {
                func,
                type_args,
                args,
                ..
            } => {
                self.expr(func);
                self.generic_args(type_args);
                args.iter_mut().for_each(|a| self.expr(a));
            }
            Expr::Instance {
                callee, type_args, ..
            } => {
                self.expr(callee);
                self.generic_args(type_args);
            }
            Expr::Unary { operand, .. }
            | Expr::Reference { operand, .. }
            | Expr::Deref { operand, .. }
            | Expr::Try { operand, .. } => self.expr(operand),
            Expr::Paren(inner, _) => self.expr(inner),
            Expr::StructLiteral { fields, base, .. } => {
                fields.iter_mut().for_each(|f| self.expr(&mut f.value));
                if let Some(base) = base {
                    self.expr(base);
                }
            }
            Expr::EnumStructLiteral { fields, .. } => {
                fields.iter_mut().for_each(|f| self.expr(&mut f.value));
            }
            Expr::FieldAccess { object, .. } | Expr::TupleIndex { object, .. } => self.expr(object),
            Expr::Cast {
                expr, target_type, ..
            } => {
                self.expr(expr);
                self.ty(target_type);
            }
            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.branches(condition, then_block, else_if_blocks, else_block),
            Expr::Block { stmts, .. }
            | Expr::Unsafe { stmts, .. }
            | Expr::Pool { stmts, .. }
            | Expr::Loop { body: stmts, .. } => self.stmts(stmts),
            Expr::Range { start, end, .. } => {
                self.expr(start);
                self.expr(end);
            }
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                elements.iter_mut().for_each(|e| self.expr(e));
            }
            Expr::Index { object, index, .. } => {
                self.expr(object);
                self.expr(index);
            }
            Expr::TensorIndex {
                object, indices, ..
            } => {
                self.expr(object);
                indices.iter_mut().for_each(|i| self.index_arg(i));
            }
            Expr::ArrayRest { array, .. } => self.expr(array),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr(scrutinee);
                for arm in arms {
                    arm.patterns.iter_mut().for_each(|p| self.pattern(p));
                    if let Some(guard) = &mut arm.guard {
                        self.expr(guard);
                    }
                    self.expr(&mut arm.body);
                }
            }
            Expr::Closure {
                params, ret, body, ..
            } => {
                for param in params {
                    self.ident(&mut param.name);
                    if let Some(ty) = &mut param.ty {
                        self.ty(ty);
                    }
                }
                if let Some(ret) = ret {
                    self.ty(ret);
                }
                self.expr(body);
            }
            Expr::InterpString { parts, .. } => {
                for part in parts {
                    if let InterpPart::Formatted { expr, .. } = part {
                        self.expr(expr);
                    }
                }
            }
        }
    }
}
