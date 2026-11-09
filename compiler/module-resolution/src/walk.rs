//! One mutable traversal reaching every place a module-qualified name can be written.
//!
//! Discovery and rewriting are the same walk with different callbacks: writing it twice
//! is how the two would drift apart and leave a qualifier standing in a corner of the
//! grammar nobody re-checked.
//!
//! The walk also tracks which local names are in scope: parameters, `val` / `var` bindings,
//! pattern and loop bindings, closure parameters and generic parameters. A name whose head
//! is a local is never offered to the callback, which is what lets a local shadow an
//! imported name or one of the module's own items.

use ast_types::{
    ClosureParam, EnumPatternPayload, Expr, GenericArg, GenericParam, GenericParamKind, InterpPart,
    Item, MatchArm, MethodDef, Pattern, Place, Stmt, TensorIndexArg, TraitBound, Type,
    VariantPayload,
};
use shared_types::Identifier;

use crate::ModuleError;

/// A place in the AST a qualified or imported name can appear.
pub(crate) enum Site<'a> {
    /// An `Expr::Path`, `Expr::EnumStructLiteral`, or bare `Expr::Identifier`. Handed over
    /// whole because resolving the name can change which node it is: `geometry::Point { x: 1.0 }`
    /// parses as a struct-variant construction and resolves to a plain struct literal.
    Expr(&'a mut Expr),
    /// A name that can only be a type or a trait: a `Type::Named` / `Type::Generic`, a
    /// struct literal's, an `impl` block's type and trait, a trait bound, `impl` / `dyn`.
    TypeName(&'a mut Identifier),
    /// A `match` / `val-else` pattern that names a variant: qualified, imported, or (for a
    /// payload-less variant) still indistinguishable from a binding.
    Pattern(&'a mut Pattern),
}

/// The callback shape both passes use. `dyn` rather than a generic parameter: the walk is
/// deeply recursive and every level would otherwise be monomorphized twice.
pub(crate) type SiteFn<'f> = &'f mut dyn FnMut(Site<'_>) -> Result<(), ModuleError>;

pub(crate) fn walk_items(items: &mut [Item], f: SiteFn) -> Result<(), ModuleError> {
    let mut walker = Walker {
        f,
        locals: Vec::new(),
    };
    for item in items {
        walker.item(item)?;
    }
    Ok(())
}

/// The first `::` segment of a name, which is what a local can shadow.
fn head(name: &str) -> &str {
    name.split("::").next().unwrap_or(name)
}

struct Walker<'f> {
    f: SiteFn<'f>,
    /// Every local name in scope, innermost last. A scope records the length on entry and
    /// truncates back to it on exit. Generic parameters share the list: a type parameter
    /// `T` shadows an item `T` in a type position as a value binding does in an expression,
    /// and a const parameter `N` is read as a value.
    locals: Vec<String>,
}

impl Walker<'_> {
    fn is_local(&self, name: &str) -> bool {
        let head = head(name);
        self.locals.iter().any(|local| local == head)
    }

    fn bind(&mut self, name: &Identifier) {
        self.locals.push(name.name.clone());
    }

    fn type_name(&mut self, name: &mut Identifier) -> Result<(), ModuleError> {
        if self.is_local(&name.name) {
            return Ok(());
        }
        (self.f)(Site::TypeName(name))
    }

    /// Run `body` in a scope of its own: every local it binds is gone afterwards.
    fn scoped(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<(), ModuleError>,
    ) -> Result<(), ModuleError> {
        let mark = self.locals.len();
        let outcome = body(self);
        self.locals.truncate(mark);
        outcome
    }

    fn item(&mut self, item: &mut Item) -> Result<(), ModuleError> {
        match item {
            Item::Function(def) => self.scoped(|w| {
                w.generics(&mut def.generics)?;
                for predicate in &mut def.where_predicates {
                    w.expr(predicate)?;
                }
                for param in &mut def.params {
                    w.ty(&mut param.ty)?;
                }
                if let Some(ret) = &mut def.return_type {
                    w.ty(ret)?;
                }
                for param in &def.params {
                    w.bind(&param.name);
                }
                w.stmts(&mut def.body)
            }),
            Item::Struct(def) => self.scoped(|w| {
                w.generics(&mut def.generics)?;
                for predicate in &mut def.where_predicates {
                    w.expr(predicate)?;
                }
                for field in &mut def.fields {
                    w.ty(&mut field.ty)?;
                }
                Ok(())
            }),
            Item::Enum(def) => self.scoped(|w| {
                w.generics(&mut def.generics)?;
                for variant in &mut def.variants {
                    match &mut variant.payload {
                        VariantPayload::Unit => {}
                        VariantPayload::Tuple(types) => {
                            for ty in types {
                                w.ty(ty)?;
                            }
                        }
                        VariantPayload::Struct(fields) => {
                            for field in fields {
                                w.ty(&mut field.ty)?;
                            }
                        }
                    }
                }
                Ok(())
            }),
            Item::Trait(def) => {
                self.bounds(&mut def.supertraits)?;
                for method in &mut def.methods {
                    self.scoped(|w| {
                        w.generics(&mut method.generics)?;
                        for predicate in &mut method.where_predicates {
                            w.expr(predicate)?;
                        }
                        for param in &mut method.params {
                            w.ty(&mut param.ty)?;
                        }
                        if let Some(ret) = &mut method.return_type {
                            w.ty(ret)?;
                        }
                        for param in &method.params {
                            w.bind(&param.name);
                        }
                        match &mut method.default_body {
                            Some(body) => w.stmts(body),
                            None => Ok(()),
                        }
                    })?;
                }
                Ok(())
            }
            Item::Impl(def) => self.scoped(|w| {
                w.generics(&mut def.generics)?;
                // The block's own type and trait are names an import may have renamed:
                // `impl Pt` after `import geometry::{Point as Pt}` extends `Point`.
                w.type_name(&mut def.type_name)?;
                if let Some(trait_name) = &mut def.trait_name {
                    w.type_name(trait_name)?;
                }
                for ty in &mut def.type_args {
                    w.ty(ty)?;
                }
                for predicate in &mut def.where_predicates {
                    w.expr(predicate)?;
                }
                for (_, ty) in &mut def.assoc_types {
                    w.ty(ty)?;
                }
                for method in &mut def.methods {
                    w.method(method)?;
                }
                Ok(())
            }),
            Item::Const(def) => {
                self.ty(&mut def.ty)?;
                self.expr(&mut def.value)
            }
            Item::Newtype(def) => self.ty(&mut def.inner),
            // An import is consumed at load time, an inline block is lifted into a module of
            // its own there too, and the `@no_prelude` marker is dropped there: none survives
            // to be walked.
            Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => Ok(()),
        }
    }

    /// Walk a `<...>` list, binding each parameter for the rest of the enclosing scope:
    /// a const parameter's type and every trait a bound names are walked first.
    fn generics(&mut self, params: &mut [GenericParam]) -> Result<(), ModuleError> {
        for param in params.iter() {
            self.bind(&param.name);
        }
        for param in params {
            if let GenericParamKind::Const(ty) = &mut param.kind {
                self.ty(ty)?;
            }
            self.bounds(&mut param.bounds)?;
        }
        Ok(())
    }

    /// Walk a bound list: each trait it names, and the types its `Trait<Assoc = T>` bind.
    fn bounds(&mut self, bounds: &mut [TraitBound]) -> Result<(), ModuleError> {
        for bound in bounds {
            self.type_name(&mut bound.trait_name)?;
            for (_, ty) in &mut bound.assoc_bindings {
                self.ty(ty)?;
            }
        }
        Ok(())
    }

    fn method(&mut self, method: &mut MethodDef) -> Result<(), ModuleError> {
        self.scoped(|w| {
            w.generics(&mut method.generics)?;
            for predicate in &mut method.where_predicates {
                w.expr(predicate)?;
            }
            for param in &mut method.params {
                w.ty(&mut param.ty)?;
            }
            if let Some(ret) = &mut method.return_type {
                w.ty(ret)?;
            }
            for param in &method.params {
                w.bind(&param.name);
            }
            w.stmts(&mut method.body)
        })
    }

    fn ty(&mut self, ty: &mut Type) -> Result<(), ModuleError> {
        match ty {
            Type::Named(name) => self.type_name(name),
            Type::Generic { name, args, .. } => {
                self.type_name(name)?;
                for arg in args {
                    if let GenericArg::Type(inner) = arg {
                        self.ty(inner)?;
                    }
                }
                Ok(())
            }
            Type::Reference { inner, .. } => self.ty(inner),
            Type::Array { element, .. } | Type::Slice { element, .. } => self.ty(element),
            Type::Tuple { elements, .. } => {
                for element in elements {
                    self.ty(element)?;
                }
                Ok(())
            }
            Type::Function { params, ret, .. } => {
                for param in params {
                    self.ty(param)?;
                }
                self.ty(ret)
            }
            Type::Tensor { element_type, .. } => self.ty(element_type),
            Type::ImplTrait {
                trait_name,
                assoc_bindings,
                ..
            } => {
                self.type_name(trait_name)?;
                for (_, ty) in assoc_bindings {
                    self.ty(ty)?;
                }
                Ok(())
            }
            Type::DynTrait { trait_name, .. } => self.type_name(trait_name),
        }
    }

    /// Walk a block: a binding it makes ends with it.
    fn stmts(&mut self, stmts: &mut [Stmt]) -> Result<(), ModuleError> {
        self.scoped(|w| {
            for stmt in stmts {
                w.stmt(stmt)?;
            }
            Ok(())
        })
    }

    fn stmt(&mut self, stmt: &mut Stmt) -> Result<(), ModuleError> {
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                if let Some(ty) = ty {
                    self.ty(ty)?;
                }
                if let Some(init) = init {
                    self.expr(init)?;
                }
                // The initializer is read before the name exists, so `val x = x + 1`
                // still reads the outer `x`.
                self.bind(name);
                Ok(())
            }
            Stmt::Assign { place, value, .. } => {
                self.place(place)?;
                self.expr(value)
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expr(value)?;
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
                self.expr(condition)?;
                self.stmts(then_block)?;
                for (cond, block) in else_if_blocks {
                    self.expr(cond)?;
                    self.stmts(block)?;
                }
                if let Some(block) = else_block {
                    self.stmts(block)?;
                }
                Ok(())
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expr(condition)?;
                self.stmts(body)
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
                self.expr(start)?;
                self.expr(end)?;
                if let Some(step) = step {
                    self.expr(step)?;
                }
                for adapter in adapters {
                    self.expr(&mut adapter.callee)?;
                }
                self.scoped(|w| {
                    if let Some(index) = index {
                        w.bind(index);
                    }
                    w.bind(iterator);
                    w.stmts(body)
                })
            }
            Stmt::ForEach {
                index,
                iterator,
                iterable,
                adapters,
                body,
                ..
            } => {
                self.expr(iterable)?;
                for adapter in adapters {
                    self.expr(&mut adapter.callee)?;
                }
                self.scoped(|w| {
                    if let Some(index) = index {
                        w.bind(index);
                    }
                    w.bind(iterator);
                    w.stmts(body)
                })
            }
            Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expr(value)?;
                }
                Ok(())
            }
            Stmt::Continue { .. } => Ok(()),
            Stmt::ValElse {
                pattern,
                value,
                else_block,
                ..
            } => {
                // The pattern's bindings reach the rest of the block, but neither the
                // scrutinee nor the `else` arm, so both are walked before them.
                self.expr(value)?;
                self.stmts(else_block)?;
                self.pattern(pattern)
            }
            Stmt::Const {
                name, ty, value, ..
            } => {
                self.ty(ty)?;
                self.expr(value)?;
                self.bind(name);
                Ok(())
            }
            Stmt::Expr(expr) => self.expr(expr),
        }
    }

    /// Walk the expressions a place reaches through. The root binding is not a site:
    /// a module qualifier cannot open an assignment target.
    fn place(&mut self, place: &mut Place) -> Result<(), ModuleError> {
        match place {
            Place::Var(_) => Ok(()),
            Place::Field { object, .. }
            | Place::TupleIndex { object, .. }
            | Place::Deref {
                pointer: object, ..
            } => self.expr(object),
            Place::Index { object, index, .. } => {
                self.expr(object)?;
                self.expr(index)
            }
            Place::TensorIndex {
                object, indices, ..
            } => {
                self.expr(object)?;
                self.tensor_indices(indices)
            }
        }
    }

    fn tensor_indices(&mut self, indices: &mut [TensorIndexArg]) -> Result<(), ModuleError> {
        for index in indices {
            match index {
                TensorIndexArg::Position(expr) => self.expr(expr)?,
                TensorIndexArg::Range {
                    start, end, step, ..
                } => {
                    self.expr(start)?;
                    self.expr(end)?;
                    if let Some(step) = step {
                        self.expr(step)?;
                    }
                }
                TensorIndexArg::FullAxis(_) => {}
            }
        }
        Ok(())
    }

    /// Offer a name-bearing expression to the callback unless a local shadows its head.
    fn expr_site(&mut self, expr: &mut Expr) -> Result<(), ModuleError> {
        let shadowed = match &*expr {
            Expr::Identifier(name) => self.is_local(&name.name),
            Expr::Path { type_name, .. } => self.is_local(&type_name.name),
            Expr::EnumStructLiteral { enum_name, .. } => self.is_local(&enum_name.name),
            _ => false,
        };
        if shadowed {
            return Ok(());
        }
        (self.f)(Site::Expr(expr))
    }

    fn expr(&mut self, expr: &mut Expr) -> Result<(), ModuleError> {
        // The callback runs before the descent: it may replace this node, and the
        // replacement's children still need walking.
        // A struct literal written as one is visited by its name alone; one the callback
        // produced from a qualified `geometry::Point { .. }` is already resolved, and is not
        // offered twice.
        match expr {
            Expr::Path { .. } | Expr::EnumStructLiteral { .. } | Expr::Identifier(_) => {
                self.expr_site(expr)?
            }
            Expr::StructLiteral { name, .. } => self.type_name(name)?,
            _ => {}
        }

        match expr {
            Expr::Literal(_, _) | Expr::Identifier(_) => Ok(()),
            Expr::Instance {
                callee, type_args, ..
            } => {
                self.expr(callee)?;
                self.generic_args(type_args)
            }
            Expr::Binary { left, right, .. } => {
                self.expr(left)?;
                self.expr(right)
            }
            Expr::Call {
                func,
                type_args,
                args,
                ..
            } => {
                self.expr(func)?;
                self.generic_args(type_args)?;
                for arg in args {
                    self.expr(arg)?;
                }
                Ok(())
            }
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::InterpString { parts, .. } => {
                for part in parts {
                    if let InterpPart::Formatted { expr, .. } = part {
                        self.expr(expr)?;
                    }
                }
                Ok(())
            }
            Expr::Paren(inner, _) => self.expr(inner),
            Expr::StructLiteral { fields, base, .. } => {
                for field in fields {
                    self.expr(&mut field.value)?;
                }
                if let Some(base) = base {
                    self.expr(base)?;
                }
                Ok(())
            }
            Expr::FieldAccess { object, .. } => self.expr(object),
            Expr::EnumStructLiteral { fields, .. } => {
                for field in fields {
                    self.expr(&mut field.value)?;
                }
                Ok(())
            }
            Expr::Path { .. } => Ok(()),
            Expr::Cast {
                expr, target_type, ..
            } => {
                self.expr(expr)?;
                self.ty(target_type)
            }
            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                self.expr(condition)?;
                self.stmts(then_block)?;
                for (cond, block) in else_if_blocks {
                    self.expr(cond)?;
                    self.stmts(block)?;
                }
                if let Some(block) = else_block {
                    self.stmts(block)?;
                }
                Ok(())
            }
            Expr::Block { stmts, .. }
            | Expr::Unsafe { stmts, .. }
            | Expr::Pool { stmts, .. }
            | Expr::Loop { body: stmts, .. } => self.stmts(stmts),
            Expr::Reference { operand, .. } | Expr::Deref { operand, .. } => self.expr(operand),
            Expr::Range { start, end, .. } => {
                self.expr(start)?;
                self.expr(end)
            }
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                for element in elements {
                    self.expr(element)?;
                }
                Ok(())
            }
            Expr::Index { object, index, .. } => {
                self.expr(object)?;
                self.expr(index)
            }
            Expr::TensorIndex {
                object, indices, ..
            } => {
                self.expr(object)?;
                self.tensor_indices(indices)
            }
            Expr::TupleIndex { object, .. } => self.expr(object),
            Expr::ArrayRest { array, .. } => self.expr(array),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr(scrutinee)?;
                for arm in arms {
                    self.arm(arm)?;
                }
                Ok(())
            }
            Expr::Closure {
                params, ret, body, ..
            } => self.scoped(|w| {
                for param in params.iter_mut() {
                    w.closure_param(param)?;
                }
                if let Some(ret) = ret {
                    w.ty(ret)?;
                }
                for param in params.iter() {
                    w.bind(&param.name);
                }
                w.expr(body)
            }),
            Expr::Try { operand, .. } => self.expr(operand),
        }
    }

    fn generic_args(&mut self, args: &mut [GenericArg]) -> Result<(), ModuleError> {
        for arg in args {
            if let GenericArg::Type(ty) = arg {
                self.ty(ty)?;
            }
        }
        Ok(())
    }

    fn arm(&mut self, arm: &mut MatchArm) -> Result<(), ModuleError> {
        self.scoped(|w| {
            for pattern in &mut arm.patterns {
                w.pattern(pattern)?;
            }
            if let Some(guard) = &mut arm.guard {
                w.expr(guard)?;
            }
            w.expr(&mut arm.body)
        })
    }

    /// Walk a pattern, binding every name it introduces for the rest of the scope.
    fn pattern(&mut self, pattern: &mut Pattern) -> Result<(), ModuleError> {
        // As in `expr`, the callback runs first: resolving an imported variant turns a
        // binding or an unqualified variant into a `Pattern::Enum`, whose payload still
        // needs walking afterwards. Whatever is still a binding after it is a local.
        (self.f)(Site::Pattern(pattern))?;

        let payload = match pattern {
            Pattern::Binding(name) => {
                self.bind(name);
                return Ok(());
            }
            Pattern::Wildcard(_) | Pattern::Literal(_, _) | Pattern::Range { .. } => {
                return Ok(());
            }
            Pattern::At { name, pattern, .. } => {
                self.bind(name);
                return self.pattern(pattern);
            }
            Pattern::Array { elements, rest, .. } => {
                for element in elements {
                    self.pattern(element)?;
                }
                if let Some(binding) = rest.as_ref().and_then(|rest| rest.binding.as_ref()) {
                    self.bind(binding);
                }
                return Ok(());
            }
            Pattern::Enum { payload, .. } | Pattern::UnqualifiedEnum { payload, .. } => payload,
        };
        match payload {
            EnumPatternPayload::Unit => Ok(()),
            EnumPatternPayload::Tuple(subs) => {
                for sub in subs {
                    self.pattern(sub)?;
                }
                Ok(())
            }
            EnumPatternPayload::Struct(fields) => {
                for field in fields {
                    self.pattern(&mut field.pattern)?;
                }
                Ok(())
            }
        }
    }

    fn closure_param(&mut self, param: &mut ClosureParam) -> Result<(), ModuleError> {
        match &mut param.ty {
            Some(ty) => self.ty(ty),
            None => Ok(()),
        }
    }
}
