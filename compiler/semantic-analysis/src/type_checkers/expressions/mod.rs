// Expression type checking: the `check_expr` dispatch, plus the arms small enough
// to need no support code. Every other category lives in a sibling module here;
// each adds methods to the same `impl TypeChecker` block.

mod blocks;
mod builtins;
mod calls;
mod const_predicates;
mod enum_exprs;
mod operators;
mod places;
mod sequences;
mod struct_exprs;
mod try_expr;

use super::{declarations, TypeChecker, VariantForm};
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::Expr;
use shared_types::Literal;

pub(crate) use const_predicates::eval_const_predicate;
use const_predicates::mentions_type_parameter;

/// The builtin deep-copy method name shared by `string` and Clone-deriving structs.
pub(crate) const CLONE_METHOD: &str = "clone";

/// The associated function that builds an empty standard collection.
pub(crate) const COLLECTION_CTOR: &str = "new";

impl TypeChecker {
    /// Check an expression and return its type.
    /// Returns None if there was an error (which has been recorded).
    /// Use this for better error recovery - checking can continue with Unknown type.
    ///
    /// # Parameters
    /// - `expr`: The expression to type check
    /// - `expected`: Optional expected type for contextual type inference
    pub(crate) fn check_expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Option<Type> {
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Integer(value, suffix_opt) => {
                    if let Some(suffix) = suffix_opt {
                        Some(self.infer_suffixed_integer_type(*value, suffix, *span))
                    } else {
                        Some(self.infer_integer_type(*value, expected, *span))
                    }
                }
                Literal::Float(_, suffix_opt) => {
                    if let Some(suffix) = suffix_opt {
                        Some(self.infer_suffixed_float_type(suffix))
                    } else {
                        Some(self.infer_float_type(expected))
                    }
                }
                Literal::Boolean(_) => Some(Type::Bool),
                Literal::Char(_) => Some(Type::Char), // Char literals have char type
                Literal::String(_) => Some(Type::String), // String literals have string type
            },

            Expr::Identifier(ident) => {
                // Variables take priority; constants are a fallback so locals can shadow consts.
                if let Some(symbol_info) = self.symbols.lookup(&ident.name) {
                    let ty = symbol_info.ty.clone();
                    if let Some(moved_at) = symbol_info.moved_at {
                        self.record_error(TypeError::UseOfMovedValue {
                            name: ident.name.clone(),
                            span: ident.span,
                            moved_at,
                        });
                    }
                    Some(ty)
                } else if let Some(const_ty) = self.constants.get(&ident.name).cloned() {
                    Some(const_ty)
                } else if let Some(const_param_ty) = self.const_scope.get(&ident.name).cloned() {
                    // A const generic parameter used as a value in a generic body
                    // has its declared integer type.
                    Some(const_param_ty)
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: ident.name.clone(),
                        span: ident.span,
                    });
                    None
                }
            }

            Expr::Binary {
                left,
                op,
                right,
                span,
            } => self.check_binary_expr(left, op, right, span, expected),

            Expr::Unary { op, operand, span } => self.check_unary_expr(op, operand, span, expected),

            Expr::Cast {
                expr,
                target_type,
                span,
            } => self.check_cast_expr(expr, target_type, span),

            Expr::Call {
                func,
                type_args,
                args,
                span,
            } => self.check_call_expr(func, type_args, args, span, expected),

            Expr::Path {
                type_name,
                member,
                span,
            } => self.check_path_expr(type_name, member, span, expected),

            Expr::Paren(inner, _) => {
                // Propagate expected type through parentheses
                self.check_expr(inner, expected)
            }

            Expr::StructLiteral {
                name,
                fields,
                base,
                span,
            } => self.check_struct_literal_expr(name, fields, base, span),

            // Struct-variant enum construction `E::V { field: expr, ... }`.
            Expr::EnumStructLiteral {
                enum_name,
                variant,
                fields,
                span,
            } => Some(self.check_enum_struct_literal(enum_name, variant, fields, *span, expected)),

            Expr::FieldAccess {
                object,
                field,
                span,
            } => self.check_field_access_expr(object, field, span),

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => self.check_if_expr(condition, then_block, else_if_blocks, else_block, span),

            Expr::Block { stmts, .. } => self.check_bare_block_expr(stmts),

            // A `loop` evaluates to the value carried by its value-producing
            // `break`s (which must all agree on type); with only plain `break`s it
            // yields unit. `while`/`for` have no expression form.
            //
            // A `loop` that no `break` targets has no exit edge at all: it either
            // runs forever or leaves via `return`. It therefore produces no value
            // and must satisfy whatever type its context demands — the same
            // divergent contract the panic-family builtins carry.
            Expr::Loop { label, body, .. } => self.check_loop_expr(label, body, expected),

            // `unsafe` is inert in Phase 1.7: it introduces a scope and yields
            // its trailing expression's type, exactly like a bare block.
            Expr::Unsafe { stmts, .. } => self.check_unsafe_block_expr(stmts),

            // Borrow `&place` / `&mut place`. The result type is `&T`
            // (or `&mut T`). Checking the operand reads its type without consuming it:
            // a borrow never moves the borrowed value, which is the whole point of a
            // reference.
            Expr::Reference {
                operand,
                mutable,
                span,
            } => self.check_reference_expr(operand, *mutable, span),

            // Dereference `*operand`: the result is the referent type `T`. The
            // operand must have a reference type; dereferencing anything else is an
            // error. Reading through either `&T` or `&mut T` is permitted.
            Expr::Deref { operand, span } => self.check_deref_expr(operand, span),

            // A range `a..b` is not a first-class value: it is consumed directly
            // by `string.slice` via `check_string_slice`, so reaching it through the
            // general expression path means it was used somewhere a range is not allowed.
            // Still check the bounds for cascaded diagnostics.
            Expr::Range {
                start, end, span, ..
            } => self.check_range_expr(start, end, span),

            // Array literal `[e0, ...]`: all elements share one type, fixed by
            // the first and required of the rest. An empty literal needs a `[T; N]`
            // annotation to know its element type.
            Expr::ArrayLiteral { elements, span } => {
                self.check_array_literal_expr(elements, span, expected)
            }

            // Array indexing `object[index]`: the object is an array (or a
            // borrow of one, auto-derefed per); the index is an integer; the
            // result is the element type.
            Expr::Index {
                object,
                index,
                span,
            } => self.check_index_expr(object, index, span),

            // Array rest pattern remainder `..rest`: the compiler-internal node
            // a `val [a, b, ..rest] = arr` desugar produces. The source must be an
            // array; the result is the `[T; N - start]` tail. `exact` (no rest binding
            // in the pattern) requires the lengths to match precisely.
            Expr::ArrayRest {
                array,
                start,
                exact,
                span,
            } => self.check_array_rest_expr(array, *start, *exact, span),

            // Tuple literal `(e0, e1, ...)`: each element is checked against the
            // corresponding element type of an expected tuple annotation, when present.
            Expr::TupleLiteral { elements, .. } => {
                self.check_tuple_literal_expr(elements, expected)
            }

            // Tuple index `object.N`: the object must be a tuple (or a borrow of
            // one); `N` must be within bounds; the result is the N-th element type.
            Expr::TupleIndex {
                object,
                index,
                span,
            } => self.check_tuple_index_expr(object, *index, span),

            // Pattern matching `match scrutinee { ... }`.
            Expr::Match {
                scrutinee,
                arms,
                span,
            } => Some(self.check_match(scrutinee, arms, *span, expected)),

            Expr::Try { operand, span } => self.check_try_expr(operand, *span),

            Expr::Closure {
                params,
                ret,
                body,
                is_move,
                span,
            } => Some(self.check_closure(params, ret.as_ref(), body, *is_move, *span)),
        }
    }
}
