//! Returning a reference: the rule that a returned borrow must reach something that
//! outlives the function, not one of its own locals.

use ast_types::{Expr, Stmt};

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use crate::types::Type;

/// The trailing value expression of a block: the last statement when it is a
/// bare expression. Used to follow a returned reference into the tail of an
/// `if`/`else` arm or a bare block.
pub(crate) fn tail_expr(stmts: &[Stmt]) -> Option<&Expr> {
    match stmts.last() {
        Some(Stmt::Expr(expr)) => Some(expr),
        _ => None,
    }
}

/// The base place identifier a borrow points into, peeling parentheses, field
/// access, and dereference (`&self.field` roots at `self`, `&(x)` at `x`). A
/// non-place operand (literal, call) has no root and yields `None`.
pub(super) fn root_place_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.name.clone()),
        Expr::Paren(inner, _) => root_place_name(inner),
        Expr::FieldAccess { object, .. } => root_place_name(object),
        Expr::Deref { operand, .. } => root_place_name(operand),
        _ => None,
    }
}

impl TypeChecker {
    /// Whether `name` is a binding local to the current function: present in the
    /// symbol table but not in the set of places that outlive the call.
    /// Function locals and by-value parameters are local; reference parameters and
    /// `self` outlive. A name absent from the table (a constant, an out-of-scope
    /// place) is conservatively treated as non-local so a valid program is never
    /// rejected.
    pub(super) fn is_local_to_function(&self, name: &str) -> bool {
        self.symbols.lookup(name).is_some() && !self.current_fn_outliving.contains(name)
    }

    /// Whether `func` names a tuple variant of an enum, so a call of it builds a value
    /// holding its arguments rather than running a function.
    fn is_variant_constructor(&self, func: &Expr) -> bool {
        let Expr::Path {
            type_name, member, ..
        } = func
        else {
            return false;
        };
        (self.enum_defs.contains_key(&type_name.name) || self.is_generic_enum(&type_name.name))
            && !self
                .functions
                .contains_key(&format!("{}__{}", type_name.name, member.name))
    }

    /// Whether a value of `ty` can hold a reference: a reference itself, or a tuple,
    /// array, struct or enum with one somewhere inside. A returned value of such a type
    /// is what the returned-reference check walks.
    pub(crate) fn type_holds_reference(&self, ty: &Type) -> bool {
        self.holds_reference_within(ty, &mut Vec::new())
    }

    fn holds_reference_within(&self, ty: &Type, seen: &mut Vec<String>) -> bool {
        match ty {
            Type::Reference { .. } => true,
            Type::Tuple(elements) => elements
                .iter()
                .any(|e| self.holds_reference_within(e, seen)),
            Type::Array { element, .. } => self.holds_reference_within(element, seen),
            Type::Struct(name) | Type::Enum(name) => {
                // A recursive type reaches itself only behind an indirection, which this
                // walk has already answered for, so a name seen once is not entered again.
                if seen.contains(name) {
                    return false;
                }
                seen.push(name.clone());
                let field_types: Vec<Type> = match self.struct_defs.get(name) {
                    Some(fields) => fields.iter().map(|(_, t)| t.clone()).collect(),
                    None => self
                        .enum_defs
                        .get(name)
                        .map(|variants| {
                            variants
                                .iter()
                                .flat_map(|v| v.fields.iter().map(|(_, t)| t.clone()))
                                .collect()
                        })
                        .unwrap_or_default(),
                };
                field_types
                    .iter()
                    .any(|t| self.holds_reference_within(t, seen))
            }
            _ => false,
        }
    }

    /// Verify a returned reference does not borrow a function-local place.
    ///
    /// Called when the current function's declared return type holds a reference.
    /// A `&place` return dangles when `place` is local; returning an existing
    /// reference binding dangles when that binding borrows a local place. The walk
    /// follows `if`/`else` arms and bare blocks so each tail that produces the
    /// returned reference is checked. Returning a reference parameter (or one
    /// derived from `self`) is sound and passes.
    pub(crate) fn check_returned_reference(&mut self, expr: &Expr) {
        match expr {
            Expr::Paren(inner, _) => self.check_returned_reference(inner),
            Expr::Reference { operand, span, .. } => {
                if let Some(name) = root_place_name(operand)
                    && self.is_local_to_function(&name)
                {
                    self.record_error(TypeError::ReturnsReferenceToLocal { name, span: *span });
                }
            }
            Expr::Identifier(ident) => {
                if let Some(place) = self.symbols.borrow_provenance(&ident.name)
                    && self.is_local_to_function(&place)
                {
                    self.record_error(TypeError::ReturnsReferenceToLocal {
                        name: place,
                        span: ident.span,
                    });
                }
            }
            Expr::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                if let Some(tail) = tail_expr(then_block) {
                    self.check_returned_reference(tail);
                }
                for (_, block) in else_if_blocks {
                    if let Some(tail) = tail_expr(block) {
                        self.check_returned_reference(tail);
                    }
                }
                if let Some(block) = else_block
                    && let Some(tail) = tail_expr(block)
                {
                    self.check_returned_reference(tail);
                }
            }
            Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } => {
                if let Some(tail) = tail_expr(stmts) {
                    self.check_returned_reference(tail);
                }
            }
            // Each arm body can produce the returned reference.
            Expr::Match { arms, .. } => {
                for arm in arms {
                    self.check_returned_reference(&arm.body);
                }
            }
            // A value built around a borrow carries it out just the same: `H { r: &s }`,
            // `E::A(&s)` and `(&s, 1)` dangle exactly as `&s` does.
            Expr::StructLiteral { fields, .. } | Expr::EnumStructLiteral { fields, .. } => {
                for field in fields {
                    self.check_returned_reference(&field.value);
                }
            }
            Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
                for element in elements {
                    self.check_returned_reference(element);
                }
            }
            Expr::Call { func, args, .. } if self.is_variant_constructor(func) => {
                for arg in args {
                    self.check_returned_reference(arg);
                }
            }
            _ => {}
        }
    }
}
