// Neuro Programming Language - Semantic Analysis
// Main type checking engine

use std::collections::{HashMap, HashSet};

use ast_types::{Attribute, Item, MethodDef, Stmt};

use crate::errors::TypeError;
use crate::symbol_table::SymbolTable;
use crate::types::Type;
use crate::warnings::{Warning, WarningCode};

/// Type checker state
pub(crate) struct TypeChecker {
    /// Symbol table for variables
    symbols: SymbolTable,
    /// Function signatures (global scope) — includes mangled method names
    functions: HashMap<String, Type>,
    /// Struct definitions: name → ordered list of (field_name, field_type)
    struct_defs: HashMap<String, Vec<(String, Type)>>,
    /// Names of structs that derive `Copy` (`@derive(Copy)`). A Copy struct is
    /// duplicated on assignment instead of moved (§2.3).
    copy_structs: HashSet<String>,
    /// Names of structs that derive `Clone` — either explicitly via `@derive(Clone)`
    /// or implicitly because they derive `Copy`. A Clone struct supports `.clone()`.
    clone_structs: HashSet<String>,
    /// Methods per struct: struct_name → method_name → mangled function key in `functions`
    ///
    /// The mangled key follows the convention `StructName__methodName`.
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Compile-time constant names and their declared types (module and function scope).
    pub(crate) constants: HashMap<String, Type>,
    /// Collected type errors
    errors: Vec<TypeError>,
    /// Collected non-fatal lint warnings
    warnings: Vec<Warning>,
    /// Current function's return type (for validating return statements)
    current_function_return_type: Option<Type>,
    /// Names of bindings in the current function whose storage outlives the call:
    /// reference-typed parameters and the `self` receiver of an instance method.
    /// A returned reference is only safe when it ultimately borrows one of these —
    /// borrowing any other (function-local) place dangles (§2.6).
    current_fn_outliving: HashSet<String>,
    /// Currently active loops, innermost last (§3.7). Stack depth doubles as the
    /// loop-nesting count used to reject `break` / `continue` outside any loop;
    /// each entry carries its label and value-break typing state.
    loop_stack: Vec<LoopContext>,
}

/// Per-active-loop tracking for `break`/`continue` resolution and value-break
/// typing (§3.7).
struct LoopContext {
    /// Loop label (`outer:`), or `None` for an unlabeled loop.
    label: Option<String>,
    /// Whether this loop can yield a value via `break v`. Only `loop` can; a
    /// `while`/`for` always yields unit, so a value-carrying `break` targeting one
    /// is an error.
    is_value_loop: bool,
    /// The agreed type of value-carrying `break`s seen so far, or `None` until the
    /// first one. All value-breaks targeting the same loop must agree on type.
    break_value_ty: Option<Type>,
}

mod declarations;
mod expressions;
mod literals;
mod moves;
mod resolution;
mod statements;

#[cfg(test)]
mod tests;

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            struct_defs: HashMap::new(),
            copy_structs: HashSet::new(),
            clone_structs: HashSet::new(),
            impl_methods: HashMap::new(),
            constants: HashMap::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            current_function_return_type: None,
            current_fn_outliving: HashSet::new(),
            loop_stack: Vec::new(),
        }
    }

    /// Record an error and continue type checking
    pub(crate) fn record_error(&mut self, error: TypeError) {
        self.errors.push(error);
    }

    /// Get all collected errors
    pub(crate) fn into_errors(self) -> Vec<TypeError> {
        self.errors
    }

    /// Get all collected lint warnings.
    pub(crate) fn into_warnings(self) -> Vec<Warning> {
        self.warnings
    }

    /// Check if there are any errors
    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Whether a value of `ty` is `Copy` — duplicated on assignment rather than moved (§2.3).
    ///
    /// Primitive scalars are always Copy; `string` never is; a struct is Copy only when it
    /// derives `Copy`. Other type forms (functions, void, unknown) are not Copy receivers
    /// in any move-tracked position, so the distinction is immaterial for them.
    pub(crate) fn is_type_copy(&self, ty: &Type) -> bool {
        match ty {
            Type::String | Type::Void | Type::Function { .. } | Type::Unknown => false,
            Type::Struct(name) => self.copy_structs.contains(name),
            // A borrow `&T` / `&mut T` is `Copy` — copying the reference is sound
            // because it never moves the borrowed value (§2.4, §2.5). Note: aliasing
            // exclusivity for `&mut T` is enforced by the borrow checker, not here.
            Type::Reference { .. } => true,
            _ => true,
        }
    }

    /// Whether values of `ty` participate in move-by-default ownership (§2.2).
    ///
    /// `string` is always tracked. A struct is tracked unless it derives `Copy`, mirroring
    /// the spec rule that user types are move-by-default and opt into copying via `@derive`.
    pub(crate) fn is_type_move_tracked(&self, ty: &Type) -> bool {
        match ty {
            Type::String => true,
            Type::Struct(name) => !self.copy_structs.contains(name),
            _ => false,
        }
    }

    /// Whether a struct named `name` supports `.clone()` — i.e. it derives `Clone` (or `Copy`,
    /// which implies `Clone`).
    pub(crate) fn struct_is_clone(&self, name: &str) -> bool {
        self.clone_structs.contains(name)
    }

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Pass 1: register struct definitions so type names resolve in method signatures.
        // This also records each struct's Copy/Clone derivation intent.
        for item in items {
            if let Item::Struct(def) = item {
                let _ = self.register_struct(def);
            }
        }

        // Pass 1b: validate `@derive(Copy)` — every field of a Copy struct must itself
        // be Copy (§2.3). Runs after all structs are registered so a Copy field that is
        // another struct resolves regardless of source order.
        for item in items {
            if let Item::Struct(def) = item {
                self.validate_copy_derive(def);
            }
        }

        // Pass 2: register impl method signatures (uses struct_defs from pass 1).
        for item in items {
            if let Item::Impl(def) = item {
                let _ = self.register_impl(def);
            }
        }

        // Pass 3: register module-level constants so they are visible in function bodies
        // regardless of source order.
        for item in items {
            if let Item::Const(def) = item {
                let _ = self.register_const_item(def);
            }
        }

        // Pass 4: check function, method, and const bodies.
        for item in items {
            match item {
                Item::Function(func) => {
                    let _ = self.check_function(func);
                }
                Item::Impl(def) => self.check_impl(def),
                Item::Const(def) => {
                    let _ = self.check_const_item(def);
                }
                Item::Struct(_) => {}
            }
        }

        // Pass 5: lint passes — independent of type errors so the developer
        // always sees style guidance alongside other diagnostics.
        self.run_lints(items);

        if self.has_errors() {
            Err(())
        } else {
            Ok(())
        }
    }

    /// Walk every function and method body emitting lint warnings.
    ///
    /// Currently implements `prefer-loop-over-while-true` (§3.7): a
    /// `while true { ... }` statement is replaced by `loop { ... }` for
    /// stylistic reasons; the warning is suppressed when the enclosing
    /// function carries `@allow(prefer_loop_over_while_true)`.
    fn run_lints(&mut self, items: &[Item]) {
        for item in items {
            match item {
                Item::Function(func) => {
                    let suppress_while_true =
                        attr_allows(&func.attributes, WarningCode::PreferLoopOverWhileTrue);
                    self.lint_block(&func.body, suppress_while_true);
                }
                Item::Impl(def) => {
                    for method in &def.methods {
                        let suppress_while_true =
                            attr_allows(&method.attributes, WarningCode::PreferLoopOverWhileTrue);
                        self.lint_method(method, suppress_while_true);
                    }
                }
                Item::Struct(_) | Item::Const(_) => {}
            }
        }
    }

    fn lint_method(&mut self, method: &MethodDef, suppress_while_true: bool) {
        self.lint_block(&method.body, suppress_while_true);
    }

    fn lint_block(&mut self, body: &[Stmt], suppress_while_true: bool) {
        for stmt in body {
            self.lint_stmt(stmt, suppress_while_true);
        }
    }

    fn lint_stmt(&mut self, stmt: &Stmt, suppress_while_true: bool) {
        match stmt {
            Stmt::While {
                condition, body, ..
            } => {
                if !suppress_while_true && is_literal_true(condition) {
                    self.warnings.push(Warning {
                        code: WarningCode::PreferLoopOverWhileTrue,
                        message: "`while true { ... }` should be written as `loop { ... }`; \
                             silence with `@allow(prefer_loop_over_while_true)` on the \
                             enclosing function"
                            .to_string(),
                        span: condition.span(),
                    });
                }
                self.lint_block(body, suppress_while_true);
            }
            Stmt::If {
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                self.lint_block(then_block, suppress_while_true);
                for (_, block) in else_if_blocks {
                    self.lint_block(block, suppress_while_true);
                }
                if let Some(block) = else_block {
                    self.lint_block(block, suppress_while_true);
                }
            }
            Stmt::ForRange { body, .. } => {
                self.lint_block(body, suppress_while_true);
            }
            Stmt::Loop { body, .. } => {
                self.lint_block(body, suppress_while_true);
            }
            _ => {}
        }
    }
}

/// True when `expr` is the bare boolean literal `true`. Parenthesised
/// `(true)` is intentionally not matched so that authors who want to keep
/// `while true` style can do so via the explicit escape hatch.
fn is_literal_true(expr: &ast_types::Expr) -> bool {
    matches!(
        expr,
        ast_types::Expr::Literal(shared_types::Literal::Boolean(true), _)
    )
}

/// True when the attribute list contains `@allow(<warning>)` for the given
/// warning code.
fn attr_allows(attributes: &[Attribute], code: WarningCode) -> bool {
    let allow_id = code.allow_identifier();
    attributes
        .iter()
        .filter(|attr| attr.name.name == "allow")
        .any(|attr| attr.args.iter().any(|arg| arg.name == allow_id))
}
