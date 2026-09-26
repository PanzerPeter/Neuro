// One mutable traversal reaching every call expression in a program.
//
// A call this walk misses keeps its labels, and the type checker after it would then
// match arguments against parameters in the order they were written: the one way a
// named argument could bind to the wrong parameter instead of failing loudly. So the
// walk visits every expression position, not only the ones a call is usually written in.
//
// It also keeps the local names in scope, because a local may shadow a top-level
// function of the same name, and a call through that local must not be checked against
// the function's signature: the type checker resolves it to the local.

use ast_types::{Expr, Item, MatchArm, Parameter, Place, Stmt, TensorIndexArg};

use crate::binding::Bound;
use crate::errors::ArgumentError;

/// The callback shape. `dyn` rather than a generic parameter: the walk is deeply
/// recursive and every level would otherwise be monomorphized per closure. The `bool`
/// says whether the callee is a bare name a local binding holds. It answers with what it
/// did to the call, which decides how the walk continues through it.
pub(crate) type CallFn<'f> = &'f mut dyn FnMut(&mut Expr, bool, &mut Vec<ArgumentError>) -> Bound;

pub(crate) fn walk_items(items: &mut [Item], f: CallFn, errors: &mut Vec<ArgumentError>) {
    let mut walker = Walker {
        f,
        errors,
        scopes: Vec::new(),
    };
    walker.items(items);
}

struct Walker<'w, 'f> {
    f: CallFn<'f>,
    errors: &'w mut Vec<ArgumentError>,
    /// The local names of each open block, innermost last.
    scopes: Vec<Vec<String>>,
}

impl Walker<'_, '_> {
    fn items(&mut self, items: &mut [Item]) {
        for item in items {
            self.item(item);
        }
    }

    fn item(&mut self, item: &mut Item) {
        match item {
            Item::Function(def) => {
                for predicate in &mut def.where_predicates {
                    self.expr(predicate);
                }
                self.body(&def.params, false, &mut def.body);
            }
            Item::Struct(def) => {
                for predicate in &mut def.where_predicates {
                    self.expr(predicate);
                }
            }
            Item::Impl(def) => {
                for predicate in &mut def.where_predicates {
                    self.expr(predicate);
                }
                for method in &mut def.methods {
                    self.body(
                        &method.params,
                        method.self_param.is_some(),
                        &mut method.body,
                    );
                }
            }
            Item::Trait(def) => {
                for method in &mut def.methods {
                    let has_self = method.self_param.is_some();
                    if let Some(body) = &mut method.default_body {
                        self.body(&method.params, has_self, body);
                    }
                }
            }
            Item::Const(def) => self.expr(&mut def.value),
            Item::Module(def) => self.items(&mut def.items),
            Item::Enum(_) | Item::Newtype(_) | Item::Import(_) | Item::NoPrelude(_) => {}
        }
    }

    /// A function or method body, with its parameters in scope.
    fn body(&mut self, params: &[Parameter], has_self: bool, stmts: &mut [Stmt]) {
        let mut names: Vec<String> = params.iter().map(|p| p.name.name.clone()).collect();
        if has_self {
            names.push("self".to_string());
        }
        self.scopes.push(names);
        self.stmts(stmts);
        self.scopes.pop();
    }

    /// A block: the names its statements bind end with it.
    fn block(&mut self, stmts: &mut [Stmt]) {
        self.scopes.push(Vec::new());
        self.stmts(stmts);
        self.scopes.pop();
    }

    fn stmts(&mut self, stmts: &mut [Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    /// Bring `name` into the innermost open scope.
    fn bind(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(name.to_string());
        }
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.iter().any(|n| n == name))
    }

    fn stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::VarDecl { name, init, .. } => {
                // The initializer is checked before the name exists: `val f = f(1)` calls
                // whatever `f` meant above this line.
                if let Some(init) = init {
                    self.expr(init);
                }
                self.bind(&name.name);
            }
            Stmt::Assign { place, value, .. } => {
                self.place(place);
                self.expr(value);
            }
            Stmt::Const { name, value, .. } => {
                self.expr(value);
                self.bind(&name.name);
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
            } => {
                self.expr(condition);
                self.block(then_block);
                for (cond, block) in else_if_blocks {
                    self.expr(cond);
                    self.block(block);
                }
                if let Some(block) = else_block {
                    self.block(block);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expr(condition);
                self.block(body);
            }
            Stmt::ForRange {
                start,
                end,
                adapters,
                body,
                index,
                iterator,
                ..
            } => {
                self.expr(start);
                self.expr(end);
                for adapter in adapters {
                    self.expr(&mut adapter.callee);
                }
                self.loop_body(
                    index.as_ref().map(|i| i.name.as_str()),
                    &iterator.name,
                    body,
                );
            }
            Stmt::ForEach {
                iterable,
                adapters,
                body,
                index,
                iterator,
                ..
            } => {
                self.expr(iterable);
                for adapter in adapters {
                    self.expr(&mut adapter.callee);
                }
                self.loop_body(
                    index.as_ref().map(|i| i.name.as_str()),
                    &iterator.name,
                    body,
                );
            }
            Stmt::ValElse {
                pattern,
                value,
                else_binding,
                else_block,
                ..
            } => {
                self.expr(value);
                self.scopes.push(Vec::new());
                if let Some(binding) = else_binding {
                    self.bind(&binding.name);
                }
                self.stmts(else_block);
                self.scopes.pop();
                for name in pattern.binding_names() {
                    self.bind(&name);
                }
            }
            Stmt::Continue { .. } => {}
            Stmt::Expr(expr) => self.expr(expr),
        }
    }

    fn loop_body(&mut self, index: Option<&str>, iterator: &str, body: &mut [Stmt]) {
        let mut names = vec![iterator.to_string()];
        names.extend(index.map(str::to_string));
        self.scopes.push(names);
        self.stmts(body);
        self.scopes.pop();
    }

    fn place(&mut self, place: &mut Place) {
        match place {
            Place::Var(_) => {}
            Place::Field { object, .. }
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
                self.tensor_indices(indices);
            }
        }
    }

    fn tensor_indices(&mut self, indices: &mut [TensorIndexArg]) {
        for index in indices {
            match index {
                TensorIndexArg::Position(expr) => self.expr(expr),
                TensorIndexArg::Range { start, end, .. } => {
                    self.expr(start);
                    self.expr(end);
                }
                TensorIndexArg::FullAxis(_) => {}
            }
        }
    }

    fn expr(&mut self, expr: &mut Expr) {
        // A call is bound before its arguments are walked: binding only reorders them, so a
        // nested call is reached either way, and reporting the outer call first puts the
        // diagnostics in source order.
        if let Expr::Call { func, .. } = expr {
            let local =
                matches!(func.as_ref(), Expr::Identifier(ident) if self.is_local(&ident.name));
            if (self.f)(expr, local, self.errors) == Bound::Hoisted {
                // The call is now a block binding its arguments to temporaries, and the
                // call it ends with is already bound. Only the initializers are walked:
                // visiting that call again would re-bind a call whose labels are gone,
                // which is exactly the shape a required label rejects.
                if let Expr::Block { stmts, .. } = expr {
                    for stmt in stmts {
                        if let Stmt::VarDecl {
                            init: Some(init), ..
                        } = stmt
                        {
                            self.expr(init);
                        }
                    }
                }
                return;
            }
        }

        match expr {
            Expr::Literal(_, _)
            | Expr::Identifier(_)
            | Expr::Path { .. }
            | Expr::Compose { .. } => {}
            Expr::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Call { func, args, .. } => {
                self.expr(func);
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Unary { operand, .. }
            | Expr::Reference { operand, .. }
            | Expr::Deref { operand, .. }
            | Expr::Try { operand, .. } => self.expr(operand),
            Expr::Paren(inner, _) => self.expr(inner),
            Expr::InterpString { parts, .. } => {
                for part in parts {
                    if let ast_types::InterpPart::Formatted { expr, .. } = part {
                        self.expr(expr);
                    }
                }
            }
            Expr::StructLiteral { fields, base, .. } => {
                for field in fields {
                    self.expr(&mut field.value);
                }
                if let Some(base) = base {
                    self.expr(base);
                }
            }
            Expr::EnumStructLiteral { fields, .. } => {
                for field in fields {
                    self.expr(&mut field.value);
                }
            }
            Expr::FieldAccess { object, .. }
            | Expr::TupleIndex { object, .. }
            | Expr::ArrayRest { array: object, .. } => self.expr(object),
            Expr::Cast { expr, .. } => self.expr(expr),
            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                self.expr(condition);
                self.block(then_block);
                for (cond, block) in else_if_blocks {
                    self.expr(cond);
                    self.block(block);
                }
                if let Some(block) = else_block {
                    self.block(block);
                }
            }
            Expr::Block { stmts, .. }
            | Expr::Unsafe { stmts, .. }
            | Expr::Pool { stmts, .. }
            | Expr::Loop { body: stmts, .. } => self.block(stmts),
            Expr::Range { start, end, .. } => {
                self.expr(start);
                self.expr(end);
            }
            Expr::ArrayLiteral { elements, .. } | Expr::TupleLiteral { elements, .. } => {
                for element in elements {
                    self.expr(element);
                }
            }
            Expr::Index { object, index, .. } => {
                self.expr(object);
                self.expr(index);
            }
            Expr::TensorIndex {
                object, indices, ..
            } => {
                self.expr(object);
                self.tensor_indices(indices);
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.arm(arm);
                }
            }
            Expr::Closure { params, body, .. } => {
                self.scopes
                    .push(params.iter().map(|p| p.name.name.clone()).collect());
                self.expr(body);
                self.scopes.pop();
            }
        }
    }

    fn arm(&mut self, arm: &mut MatchArm) {
        let names = arm
            .patterns
            .iter()
            .flat_map(|pattern| pattern.binding_names())
            .collect();
        self.scopes.push(names);
        if let Some(guard) = &mut arm.guard {
            self.expr(guard);
        }
        self.expr(&mut arm.body);
        self.scopes.pop();
    }
}
