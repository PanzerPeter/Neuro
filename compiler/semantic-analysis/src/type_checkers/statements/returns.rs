//! Returning a reference: the rule that a returned borrow must reach something that
//! outlives the function, not one of its own locals.

use ast_types::{BinaryOp, Expr, Stmt};
use shared_types::Span;

use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use crate::types::Type;

use super::borrow_target_of;

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
    /// array, struct, enum or newtype with one somewhere inside. A value of such a type
    /// holds the borrows it was built around, and a returned one is what the
    /// returned-reference check walks.
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
            Type::Newtype(name) => self
                .newtype_defs
                .get(name)
                .cloned()
                .is_some_and(|inner| self.holds_reference_within(&inner, seen)),
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
    /// A `&place` return dangles when `place` is local; returning an existing binding
    /// dangles when anything it borrows is local, whether it is a reference or a value
    /// built around one. The walk follows `if`/`else` arms, bare blocks and `match` arms
    /// so each tail that produces the returned value is checked. Returning a reference
    /// parameter (or one derived from `self`) is sound and passes.
    pub(crate) fn check_returned_reference(&mut self, expr: &Expr) {
        self.check_escaping_borrows(expr, Escape::Return);
    }

    /// Verify a value stored through a reference (`*r = v`, `p.field = v` with `p` a
    /// `&mut` parameter or `self`) holds no borrow of a function-local place: the
    /// reference's target outlives the call, and the local does not.
    pub(crate) fn check_stored_through_reference(&mut self, expr: &Expr) {
        self.check_escaping_borrows(expr, Escape::Store);
    }

    fn report_escape(&mut self, name: String, span: Span, escape: Escape) {
        self.record_error(match escape {
            Escape::Return => TypeError::ReturnsReferenceToLocal { name, span },
            Escape::Store => TypeError::StoresReferenceToLocal { name, span },
        });
    }

    fn check_escaping_borrows(&mut self, expr: &Expr, escape: Escape) {
        // A `.slice` view borrows its receiver exactly as `&receiver` does.
        if matches!(expr, Expr::Call { .. })
            && let Some((place, _)) = borrow_target_of(expr)
        {
            if self.is_local_to_function(&place) {
                self.report_escape(place, expr.span(), escape);
            }
            return;
        }
        match expr {
            Expr::Paren(inner, _) => self.check_escaping_borrows(inner, escape),
            Expr::Reference { operand, span, .. } => {
                if let Some(name) = root_place_name(operand)
                    && self.is_local_to_function(&name)
                {
                    self.report_escape(name, *span, escape);
                }
            }
            Expr::Identifier(ident) => {
                if let Some(place) = self
                    .symbols
                    .borrowed_places(&ident.name)
                    .into_iter()
                    .find(|place| self.is_local_to_function(place))
                {
                    self.report_escape(place, ident.span, escape);
                }
            }
            // A part of a value holds that value's borrows: `h.r` dangles when `h` was
            // built around a borrow of a local.
            Expr::FieldAccess { object, .. }
            | Expr::TupleIndex { object, .. }
            | Expr::Index { object, .. } => self.check_escaping_borrows(object, escape),
            Expr::Try { operand, .. } => self.check_escaping_borrows(operand, escape),
            Expr::Binary {
                left,
                op: BinaryOp::NullCoalesce,
                right,
                ..
            } => {
                self.check_escaping_borrows(left, escape);
                self.check_escaping_borrows(right, escape);
            }
            Expr::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                if let Some(tail) = tail_expr(then_block) {
                    self.check_escaping_borrows(tail, escape);
                }
                for (_, block) in else_if_blocks {
                    if let Some(tail) = tail_expr(block) {
                        self.check_escaping_borrows(tail, escape);
                    }
                }
                if let Some(block) = else_block
                    && let Some(tail) = tail_expr(block)
                {
                    self.check_escaping_borrows(tail, escape);
                }
            }
            Expr::Block { stmts, .. } | Expr::Unsafe { stmts, .. } => {
                if let Some(tail) = tail_expr(stmts) {
                    self.check_escaping_borrows(tail, escape);
                }
            }
            // Each arm body can produce the value, and an arm may yield a part of the
            // scrutinee its pattern bound, so a scrutinee borrowing a local dangles too.
            Expr::Match {
                scrutinee, arms, ..
            } => {
                self.check_escaping_borrows(scrutinee, escape);
                for arm in arms {
                    self.check_escaping_borrows(&arm.body, escape);
                }
            }
            // A value built around a borrow carries it out just the same: `H { r: &s }`,
            // `E::A(&s)` and `(&s, 1)` dangle exactly as `&s` does.
            Expr::StructLiteral { fields, base, .. } => {
                for field in fields {
                    self.check_escaping_borrows(&field.value, escape);
                }
                if let Some(base) = base {
                    self.check_escaping_borrows(base, escape);
                }
            }
            Expr::EnumStructLiteral { fields, .. } => {
                for field in fields {
                    self.check_escaping_borrows(&field.value, escape);
                }
            }
            Expr::TupleLiteral { elements, .. } | Expr::ArrayLiteral { elements, .. } => {
                for element in elements {
                    self.check_escaping_borrows(element, escape);
                }
            }
            // A call's result may hold any borrow its arguments hand it (lifetime
            // elision), and a method's whatever its receiver holds.
            Expr::Call { func, args, .. } => {
                for arg in args {
                    self.check_escaping_borrows(arg, escape);
                }
                if !self.is_variant_constructor(func)
                    && let Expr::FieldAccess { object, .. } = func.as_ref()
                {
                    self.check_escaping_borrows(object, escape);
                }
            }
            _ => {}
        }
    }
}

/// Where a value holding a borrow is going when it leaves the function's frame.
#[derive(Clone, Copy)]
enum Escape {
    /// Returned to the caller.
    Return,
    /// Written through a reference into storage the caller owns.
    Store,
}
