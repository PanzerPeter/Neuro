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
    /// Enum definitions: name → ordered list of variants (§3.5). The order is the
    /// declaration order, which is also the discriminant order used by codegen.
    enum_defs: HashMap<String, Vec<EnumVariantInfo>>,
    /// Newtype definitions: name → resolved inner type (§3.15). A newtype is a
    /// distinct nominal type wrapping this inner type; construction is `Name(value)`
    /// and the inner value is read via `.0`.
    newtype_defs: HashMap<String, Type>,
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
    /// Mangled keys of `&mut self` methods (§2.5). Calling one takes an exclusive
    /// borrow of the receiver, so the receiver must be a mutable place and must not
    /// already be borrowed — checked at the call site like a `&mut place` borrow.
    mut_self_methods: HashSet<String>,
    /// Generic free-function templates (§3.8), keyed by name. A generic function is
    /// NOT placed in `functions` — calls to it route through generic inference, which
    /// substitutes concrete type arguments per call site (monomorphization).
    generic_funcs: HashMap<String, GenericFnSig>,
    /// Generic struct templates (§3.8), keyed by name. A generic struct is NOT a
    /// usable type on its own; each distinct set of type arguments is monomorphized
    /// into a distinct nominal struct registered in `struct_defs` on demand. The
    /// template's field types (carrying [`Type::Generic`] placeholders) are also kept
    /// in `struct_defs` under the base name so generic method bodies type-check.
    generic_structs: HashMap<String, ast_types::StructDef>,
    /// Generic `impl` templates keyed by base struct name (§3.8): `impl<T> Wrapper<T>`.
    /// Instantiating a generic struct also instantiates each matching impl's methods.
    generic_impls: HashMap<String, Vec<ast_types::ImplDef>>,
    /// User-declared traits keyed by name (§3.9): each carries its method signatures so
    /// `impl Trait for Type` conformance and generic-body trait-method dispatch resolve.
    traits: HashMap<String, TraitInfo>,
    /// Concrete `(trait name, implementing type name)` pairs that have an
    /// `impl Trait for Type` block (§3.9). A generic bound `T: Trait` is satisfied at a
    /// call site exactly when the concrete type argument appears here.
    trait_impls: HashSet<(String, String)>,
    /// Operator-trait dispatch (§3.10): `(struct name, binary operator)` → the value
    /// type of the right operand and the operator's result type. Present exactly when
    /// the struct has an operator-trait impl providing that operator.
    operator_binary_impls: HashMap<(String, ast_types::BinaryOp), OperatorDispatch>,
    /// Operator-trait dispatch for unary operators (`-a` via `Neg`, `~a` via `Not`).
    operator_unary_impls: HashMap<(String, ast_types::UnaryOp), Type>,
    /// Trait bounds of the type parameters in scope while a generic definition is checked
    /// (§3.8, §3.9): parameter name → declared trait names. Lets a generic body dispatch
    /// a trait method on a bounded type parameter. Empty outside a generic definition.
    pub(crate) generic_bounds: HashMap<String, Vec<String>>,
    /// Mangled names of generic-struct instantiations already materialized into
    /// `struct_defs` / `impl_methods`, so each instance is built exactly once.
    instantiated_structs: HashSet<String>,
    /// Type-parameter names in scope while checking a generic function's signature and
    /// body. A `Named` annotation matching one resolves to [`Type::Generic`] instead of
    /// erroring as an unknown type. Empty outside a generic function.
    pub(crate) generic_scope: HashSet<String>,
    /// Const (value) generic parameters in scope while checking a generic definition's
    /// signature and body (§3.8), mapping each name to its declared integer type. A value
    /// reference to one resolves to that type; an array length naming one becomes an
    /// [`crate::types::ArrayLen::Param`]. Empty outside a generic definition.
    pub(crate) const_scope: HashMap<String, Type>,
    /// Explicit lifetime parameter names in scope while checking a definition's signature
    /// and body (§2.6), e.g. `'a` from `func f<'a>(...)` (stored without the leading `'`).
    /// A reference annotation `&'a T` is well-formed only if `a` is present here. Empty
    /// outside a definition that declares lifetimes.
    pub(crate) lifetime_scope: HashSet<String>,
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

/// The construction form of an enum variant (§3.5), determining how it is built:
/// `Color::Red`, `Move(1, 2)`, or `Circle { radius: 5.0 }`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum VariantForm {
    Unit,
    Tuple,
    Struct,
}

/// A generic free-function template's resolved signature (§3.8). Parameter and
/// return types carry [`Type::Generic`] placeholders for the type parameters named
/// in `param_names` (declaration order, which is also the monomorphization and
/// turbofish order). A name present in `const_types` is a const (value) parameter of
/// that integer type; the rest are type parameters. `where_predicates` are value
/// predicates checked at each call against the concrete const values.
#[derive(Clone)]
pub(crate) struct GenericFnSig {
    pub(crate) param_names: Vec<String>,
    pub(crate) const_types: HashMap<String, Type>,
    pub(crate) params: Vec<Type>,
    pub(crate) ret: Type,
    pub(crate) where_predicates: Vec<ast_types::Expr>,
    /// Trait bounds per type parameter (§3.9), e.g. `T: Drawable`. Checked at each call
    /// site against the inferred concrete type argument.
    pub(crate) bounds: HashMap<String, Vec<String>>,
}

/// A user-declared trait's resolved method signatures (§3.9), keyed by method name.
#[derive(Clone)]
pub(crate) struct TraitInfo {
    pub(crate) methods: HashMap<String, TraitMethodSig>,
}

/// How a binary operator dispatches to an operator-trait method (§3.10).
#[derive(Clone)]
pub(crate) struct OperatorDispatch {
    /// The value type the right operand must have. For a by-reference method parameter
    /// (`rhs: &Rhs`) this is the referent, since the operand is borrowed at the call.
    pub(crate) rhs: Type,
    /// The operator's result type: the method's `Output` for arithmetic/bitwise traits,
    /// or `bool` for the comparison traits.
    pub(crate) result: Type,
}

/// One resolved trait-method signature (§3.9). `params` excludes the implicit `self`.
/// `required` is true when the trait gave no default body — an implementor must provide
/// one. Types are resolved in the trait's (non-generic) scope, so `Self`-typed and
/// associated-type positions are not supported this phase.
#[derive(Clone)]
pub(crate) struct TraitMethodSig {
    pub(crate) self_param: Option<ast_types::SelfParam>,
    pub(crate) params: Vec<Type>,
    pub(crate) ret: Type,
    pub(crate) required: bool,
}

/// A resolved enum variant: its name, construction form, and ordered payload
/// fields. Each field carries an optional name (`Some` for struct variants, `None`
/// for tuple variants) and its resolved type.
pub(crate) struct EnumVariantInfo {
    pub(crate) name: String,
    pub(crate) form: VariantForm,
    pub(crate) fields: Vec<(Option<String>, Type)>,
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
mod matches;
mod moves;
pub(crate) mod operator_traits;
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
            enum_defs: HashMap::new(),
            newtype_defs: HashMap::new(),
            copy_structs: HashSet::new(),
            clone_structs: HashSet::new(),
            impl_methods: HashMap::new(),
            mut_self_methods: HashSet::new(),
            generic_funcs: HashMap::new(),
            generic_structs: HashMap::new(),
            generic_impls: HashMap::new(),
            traits: HashMap::new(),
            trait_impls: HashSet::new(),
            operator_binary_impls: HashMap::new(),
            operator_unary_impls: HashMap::new(),
            generic_bounds: HashMap::new(),
            instantiated_structs: HashSet::new(),
            generic_scope: HashSet::new(),
            const_scope: HashMap::new(),
            lifetime_scope: HashSet::new(),
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
            // A newtype forwards `Copy` from its inner type (§3.15). Cycles are
            // rejected at registration, so this recursion terminates.
            Type::Newtype(name) => self
                .newtype_defs
                .get(name)
                .map(|inner| self.is_type_copy(inner))
                .unwrap_or(false),
            // A borrow `&T` / `&mut T` is `Copy` — copying the reference is sound
            // because it never moves the borrowed value (§2.4, §2.5). Note: aliasing
            // exclusivity for `&mut T` is enforced by the borrow checker, not here.
            Type::Reference { .. } => true,
            // An array is `Copy` exactly when its element type is `Copy` (§2.3, §3.1).
            // The element is currently restricted to Copy scalars at resolution time,
            // so this recursion is always true in practice; it keeps the rule honest
            // if the restriction is later relaxed.
            Type::Array { element, .. } => self.is_type_copy(element),
            // A tuple is `Copy` exactly when every element is `Copy` (§3.2, §490).
            // Element Copy-ness is enforced at resolution, so this is always true in
            // practice; it keeps the rule honest if that restriction is relaxed.
            Type::Tuple(elements) => elements.iter().all(|e| self.is_type_copy(e)),
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

    /// Look up a newtype's resolved inner type by name (§3.15).
    pub(crate) fn lookup_newtype_inner(&self, name: &str) -> Option<&Type> {
        self.newtype_defs.get(name)
    }

    /// Look up a variant of an enum by name, returning its resolved info (§3.5).
    pub(crate) fn lookup_enum_variant(
        &self,
        enum_name: &str,
        variant: &str,
    ) -> Option<&EnumVariantInfo> {
        self.enum_defs
            .get(enum_name)?
            .iter()
            .find(|v| v.name == variant)
    }

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Pass 0a: pre-register newtype NAMES (§3.15) so a newtype used as a struct
        // field, enum payload, or another newtype's inner resolves regardless of
        // source order. Inner types are resolved and validated in pass 1c, once every
        // nominal name is known.
        for item in items {
            if let Item::Newtype(def) = item {
                self.predeclare_newtype(def);
            }
        }

        // Pass 0: register enum definitions before structs, so an enum used as a
        // struct field type (or vice versa) resolves regardless of source order.
        for item in items {
            if let Item::Enum(def) = item {
                self.register_enum(def);
            }
        }

        // Pass 1: register struct definitions so type names resolve in method signatures.
        // This also records each struct's Copy/Clone derivation intent.
        for item in items {
            if let Item::Struct(def) = item {
                if def.generics.is_empty() {
                    let _ = self.register_struct(def);
                } else {
                    self.register_generic_struct(def);
                }
            }
        }

        // Pass 1c: resolve and validate newtype inner types now that every nominal
        // name is registered — enforces the Copy-inner restriction and rejects cycles
        // (§3.15). Runs before Copy-derive validation so a struct with a newtype field
        // sees the newtype's real Copy-ness.
        self.resolve_newtype_inners(items);

        // Pass 1b: validate `@derive(Copy)` — every field of a Copy struct must itself
        // be Copy (§2.3). Runs after all structs are registered so a Copy field that is
        // another struct resolves regardless of source order.
        for item in items {
            if let Item::Struct(def) = item {
                self.validate_copy_derive(def);
            }
        }

        // Pass 1d: register trait declarations (§3.9) before impls so `impl Trait for T`
        // conformance and generic trait bounds can resolve the trait's method signatures.
        for item in items {
            if let Item::Trait(def) = item {
                self.register_trait(def);
            }
        }

        // Pass 2: register impl method signatures (uses struct_defs from pass 1).
        for item in items {
            if let Item::Impl(def) = item {
                if def.generics.is_empty() && def.type_args.is_empty() {
                    let _ = self.register_impl(def);
                } else {
                    self.register_generic_impl(def);
                }
            }
        }

        // Pass 2b: operator-trait supertrait check (§3.10), after all impls are
        // registered so `Comparable: PartialEq` is order-independent.
        self.check_operator_supertraits(items);

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
                Item::Impl(def) => {
                    if def.generics.is_empty() && def.type_args.is_empty() {
                        self.check_impl(def);
                    } else {
                        self.check_generic_impl(def);
                    }
                }
                Item::Const(def) => {
                    let _ = self.check_const_item(def);
                }
                // Enums, newtypes, and traits carry no directly-checked bodies. Trait
                // default-method bodies are checked through the impl copies the parser
                // injects (§3.9); the trait declaration itself is validated at registration.
                Item::Struct(_) | Item::Enum(_) | Item::Newtype(_) | Item::Trait(_) => {}
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
                Item::Struct(_)
                | Item::Const(_)
                | Item::Enum(_)
                | Item::Newtype(_)
                | Item::Trait(_) => {}
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
