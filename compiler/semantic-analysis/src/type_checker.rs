// NEURO Programming Language - Semantic Analysis
// Main type checking engine

use std::collections::HashMap;

use ast_types::{
    BinaryOp, Expr, FieldInit, FunctionDef, ImplDef, Item, SelfParam, Stmt, StructDef, UnaryOp,
};
use shared_types::{Literal, Span};

use crate::errors::TypeError;
use crate::symbol_table::SymbolTable;
use crate::types::Type;

/// Type checker state
pub(crate) struct TypeChecker {
    /// Symbol table for variables
    symbols: SymbolTable,
    /// Function signatures (global scope) — includes mangled method names
    functions: HashMap<String, Type>,
    /// Struct definitions: name → ordered list of (field_name, field_type)
    struct_defs: HashMap<String, Vec<(String, Type)>>,
    /// Methods per struct: struct_name → method_name → mangled function key in `functions`
    ///
    /// The mangled key follows the convention `StructName__methodName`.
    impl_methods: HashMap<String, HashMap<String, String>>,
    /// Collected type errors
    errors: Vec<TypeError>,
    /// Current function's return type (for validating return statements)
    current_function_return_type: Option<Type>,
    /// Nesting depth of active loop statements.
    loop_depth: u32,
}

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            struct_defs: HashMap::new(),
            impl_methods: HashMap::new(),
            errors: Vec::new(),
            current_function_return_type: None,
            loop_depth: 0,
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

    /// Check if there are any errors
    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if an integer literal fits within the range of a target type
    fn check_integer_range(&self, value: i64, target_ty: &Type) -> bool {
        match target_ty {
            Type::I8 => value >= i8::MIN as i64 && value <= i8::MAX as i64,
            Type::I16 => value >= i16::MIN as i64 && value <= i16::MAX as i64,
            Type::I32 => value >= i32::MIN as i64 && value <= i32::MAX as i64,
            Type::I64 => true, // All i64 values fit in i64
            Type::U8 => value >= 0 && value <= u8::MAX as i64,
            Type::U16 => value >= 0 && value <= u16::MAX as i64,
            Type::U32 => value >= 0 && value <= u32::MAX as i64,
            Type::U64 => value >= 0, // Positive i64 values fit in u64
            _ => false,              // Not an integer type
        }
    }

    /// Infer the type of an integer literal based on expected type
    /// Returns the inferred type and whether it's valid
    fn infer_integer_type(&mut self, value: i64, expected: Option<&Type>, span: Span) -> Type {
        if let Some(exp_ty) = expected {
            // If expected type is an integer type, try to use it
            if exp_ty.is_integer() {
                if self.check_integer_range(value, exp_ty) {
                    return exp_ty.clone();
                } else {
                    // Value doesn't fit in expected type
                    self.record_error(TypeError::IntegerLiteralOutOfRange {
                        value,
                        ty: exp_ty.clone(),
                        span,
                    });
                    return Type::Unknown;
                }
            }
        }

        // No expected type or expected type is not integer: default to i32
        // Also validate that the value fits in i32
        if self.check_integer_range(value, &Type::I32) {
            Type::I32
        } else {
            // Value doesn't fit in default i32, use i64
            Type::I64
        }
    }

    /// Infer the type of a float literal based on expected type
    fn infer_float_type(&self, expected: Option<&Type>) -> Type {
        if let Some(exp_ty) = expected {
            // If expected type is a float type, use it
            if exp_ty.is_float() {
                return exp_ty.clone();
            }
        }

        // Default to f64
        Type::F64
    }

    /// Convert syntax-parsing type to semantic type.
    /// Returns None if the type is unknown (error is recorded).
    fn resolve_type(&mut self, ty: &ast_types::Type) -> Option<Type> {
        match ty {
            ast_types::Type::Named(ident) => match ident.name.as_str() {
                // Signed integers
                "i8" => Some(Type::I8),
                "i16" => Some(Type::I16),
                "i32" => Some(Type::I32),
                "i64" => Some(Type::I64),
                // Unsigned integers
                "u8" => Some(Type::U8),
                "u16" => Some(Type::U16),
                "u32" => Some(Type::U32),
                "u64" => Some(Type::U64),
                // Floating point
                "f32" => Some(Type::F32),
                "f64" => Some(Type::F64),
                // Other types
                "bool" => Some(Type::Bool),
                "string" => Some(Type::String),
                "void" => Some(Type::Void),
                name => {
                    if self.struct_defs.contains_key(name) {
                        Some(Type::Struct(name.to_string()))
                    } else {
                        self.record_error(TypeError::UnknownTypeName {
                            name: name.to_string(),
                            span: ident.span,
                        });
                        None
                    }
                }
            },
            ast_types::Type::Tensor { span, .. } => {
                // Tensor types are Phase 3, not supported in Phase 1
                self.record_error(TypeError::UnknownTypeName {
                    name: "Tensor".to_string(),
                    span: *span,
                });
                None
            }
        }
    }

    /// Type-check a plain identifier call (free function or previously registered
    /// method with a mangled name). Extracted so the `Call` arm can delegate here.
    fn check_plain_call(
        &mut self,
        func_name: &str,
        args: &[ast_types::Expr],
        span: shared_types::Span,
    ) -> Option<Type> {
        let func_ty = if let Some(ty) = self.functions.get(func_name) {
            ty.clone()
        } else {
            self.record_error(TypeError::UndefinedFunction {
                name: func_name.to_string(),
                span,
            });
            return Some(Type::Unknown);
        };

        let (param_types, return_type) = match func_ty {
            Type::Function { params, ret } => (params, *ret),
            _ => {
                self.record_error(TypeError::NotCallable { ty: func_ty, span });
                return Some(Type::Unknown);
            }
        };

        if args.len() != param_types.len() {
            self.record_error(TypeError::ArgumentCountMismatch {
                expected: param_types.len(),
                found: args.len(),
                span,
            });
        }

        for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                if !arg_ty.is_compatible_with(expected_ty) {
                    self.record_error(TypeError::Mismatch {
                        expected: expected_ty.clone(),
                        found: arg_ty,
                        span: arg.span(),
                    });
                }
            }
        }

        Some(return_type)
    }

    /// Check an expression and return its type.
    /// Returns None if there was an error (which has been recorded).
    /// Use this for better error recovery - checking can continue with Unknown type.
    ///
    /// # Parameters
    /// - `expr`: The expression to type check
    /// - `expected`: Optional expected type for contextual type inference
    fn check_expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Option<Type> {
        match expr {
            Expr::Literal(lit, span) => match lit {
                Literal::Integer(value) => {
                    // Use contextual type inference for integer literals
                    Some(self.infer_integer_type(*value, expected, *span))
                }
                Literal::Float(_) => {
                    // Use contextual type inference for float literals
                    Some(self.infer_float_type(expected))
                }
                Literal::Boolean(_) => Some(Type::Bool),
                Literal::String(_) => Some(Type::String), // String literals have string type
            },

            Expr::Identifier(ident) => {
                // Identifiers return their stored type, ignoring expected type
                if let Some(symbol_info) = self.symbols.lookup(&ident.name) {
                    Some(symbol_info.ty.clone())
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
            } => {
                // Check both operands even if one fails, for better error reporting
                // For binary operations, operands must match each other
                // First check left without expected type to get its natural type
                let left_ty = self.check_expr(left, None).unwrap_or(Type::Unknown);
                // Then check right with left's type as expected (for symmetric type inference)
                let right_ty = self
                    .check_expr(right, Some(&left_ty))
                    .unwrap_or(Type::Unknown);

                // If either operand is Unknown (error), propagate Unknown
                if matches!(left_ty, Type::Unknown) || matches!(right_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                match op {
                    // Arithmetic operators: require numeric types, return same type
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => {
                        if !left_ty.is_numeric() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty.clone(),
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        Some(left_ty)
                    }

                    // Comparison operators: require compatible types, return bool
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::Greater
                    | BinaryOp::LessEqual
                    | BinaryOp::GreaterEqual => {
                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty,
                                found: right_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(Type::Bool)
                    }

                    // Logical operators: require bool types, return bool
                    BinaryOp::And | BinaryOp::Or => {
                        let mut has_error = false;

                        if !left_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty,
                                right: right_ty.clone(),
                                span: *span,
                            });
                            has_error = true;
                        }

                        if !right_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: Type::Bool,
                                right: right_ty,
                                span: *span,
                            });
                            has_error = true;
                        }

                        if has_error {
                            Some(Type::Unknown)
                        } else {
                            Some(Type::Bool)
                        }
                    }
                }
            }

            Expr::Unary { op, operand, span } => {
                // For unary operations, propagate expected type to operand if appropriate
                let expected_operand = match op {
                    UnaryOp::Negate => expected.filter(|t| t.is_numeric()),
                    UnaryOp::Not => None, // Not requires bool, no flexibility
                };

                let operand_ty = self
                    .check_expr(operand, expected_operand)
                    .unwrap_or(Type::Unknown);

                if matches!(operand_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                match op {
                    UnaryOp::Negate => {
                        if !operand_ty.is_numeric() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(operand_ty)
                    }
                    UnaryOp::Not => {
                        if !operand_ty.is_bool() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(Type::Bool)
                    }
                }
            }

            Expr::Cast {
                expr,
                target_type,
                span,
            } => {
                let from_type = self.check_expr(expr, None)?;
                if matches!(from_type, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                let to_type = self.resolve_type(target_type)?;
                if matches!(to_type, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                if to_type.is_valid_cast(&from_type) {
                    Some(to_type)
                } else {
                    self.record_error(TypeError::Mismatch {
                        expected: to_type.clone(),
                        found: from_type,
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }

            Expr::Call { func, args, span } => {
                match &**func {
                    Expr::Identifier(ident) => self.check_plain_call(&ident.name, args, *span),

                    // Method call: `instance.method(args)`
                    // The object type determines which struct's methods to search.
                    Expr::FieldAccess {
                        object,
                        field,
                        span: fa_span,
                    } => {
                        let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                        if matches!(obj_ty, Type::Unknown) {
                            return Some(Type::Unknown);
                        }
                        let struct_name = match &obj_ty {
                            Type::Struct(n) => n.clone(),
                            other => {
                                self.record_error(TypeError::MethodNotFound {
                                    struct_name: other.to_string(),
                                    method_name: field.name.clone(),
                                    span: *fa_span,
                                });
                                return Some(Type::Unknown);
                            }
                        };

                        let mangled = match self
                            .impl_methods
                            .get(&struct_name)
                            .and_then(|m| m.get(&field.name))
                        {
                            Some(k) => k.clone(),
                            None => {
                                self.record_error(TypeError::MethodNotFound {
                                    struct_name,
                                    method_name: field.name.clone(),
                                    span: *fa_span,
                                });
                                return Some(Type::Unknown);
                            }
                        };

                        // The mangled function's first parameter is `self` (the struct).
                        // Callers provide only the non-self arguments, so we skip param[0]
                        // when checking arity and types.
                        let func_ty = self.functions.get(&mangled).cloned();
                        let (param_types, return_type) = match func_ty {
                            Some(Type::Function { params, ret }) => (params, *ret),
                            _ => return Some(Type::Unknown),
                        };

                        // param_types[0] is the implicit `self`; user-visible params start at [1]
                        let visible_params = if param_types.is_empty() {
                            &param_types[..]
                        } else {
                            &param_types[1..]
                        };

                        if args.len() != visible_params.len() {
                            self.record_error(TypeError::ArgumentCountMismatch {
                                expected: visible_params.len(),
                                found: args.len(),
                                span: *span,
                            });
                        }

                        for (arg, expected_ty) in args.iter().zip(visible_params.iter()) {
                            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                                if !arg_ty.is_compatible_with(expected_ty) {
                                    self.record_error(TypeError::Mismatch {
                                        expected: expected_ty.clone(),
                                        found: arg_ty,
                                        span: arg.span(),
                                    });
                                }
                            }
                        }

                        Some(return_type)
                    }

                    // Associated function call: `TypeName::func(args)`
                    Expr::Path {
                        type_name,
                        member,
                        span: path_span,
                    } => {
                        if !self.struct_defs.contains_key(&type_name.name) {
                            self.record_error(TypeError::UnknownPathType {
                                type_name: type_name.name.clone(),
                                member: member.name.clone(),
                                span: *path_span,
                            });
                            return Some(Type::Unknown);
                        }

                        let mangled = format!("{}__{}", type_name.name, member.name);
                        let func_ty = if let Some(ty) = self.functions.get(&mangled) {
                            ty.clone()
                        } else {
                            self.record_error(TypeError::UnknownAssociatedFunction {
                                type_name: type_name.name.clone(),
                                member: member.name.clone(),
                                span: *path_span,
                            });
                            return Some(Type::Unknown);
                        };

                        let (param_types, return_type) = match func_ty {
                            Type::Function { params, ret } => (params, *ret),
                            _ => return Some(Type::Unknown),
                        };

                        if args.len() != param_types.len() {
                            self.record_error(TypeError::ArgumentCountMismatch {
                                expected: param_types.len(),
                                found: args.len(),
                                span: *span,
                            });
                        }

                        for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
                            if let Some(arg_ty) = self.check_expr(arg, Some(expected_ty)) {
                                if !arg_ty.is_compatible_with(expected_ty) {
                                    self.record_error(TypeError::Mismatch {
                                        expected: expected_ty.clone(),
                                        found: arg_ty,
                                        span: arg.span(),
                                    });
                                }
                            }
                        }

                        Some(return_type)
                    }

                    _ => {
                        let expr_ty = self.check_expr(func, None).unwrap_or(Type::Unknown);
                        self.record_error(TypeError::NotCallable {
                            ty: expr_ty,
                            span: *span,
                        });
                        Some(Type::Unknown)
                    }
                }
            }

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                // Standalone path expression (not used as a call target).
                // Validate the struct and member exist; the type is a function type.
                if !self.struct_defs.contains_key(&type_name.name) {
                    self.record_error(TypeError::UnknownPathType {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                }
                let mangled = format!("{}__{}", type_name.name, member.name);
                if let Some(ty) = self.functions.get(&mangled) {
                    Some(ty.clone())
                } else {
                    self.record_error(TypeError::UnknownAssociatedFunction {
                        type_name: type_name.name.clone(),
                        member: member.name.clone(),
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }

            Expr::Paren(inner, _) => {
                // Propagate expected type through parentheses
                self.check_expr(inner, expected)
            }

            Expr::StructLiteral { name, fields, span } => {
                let def = if let Some(d) = self.struct_defs.get(&name.name).cloned() {
                    d
                } else {
                    self.record_error(TypeError::UnknownStruct {
                        name: name.name.clone(),
                        span: name.span,
                    });
                    return None;
                };

                // Track which fields have been provided to detect duplicates and missing fields
                let mut seen: HashMap<String, Span> = HashMap::new();
                for FieldInit {
                    name: fname,
                    value,
                    span: fspan,
                } in fields
                {
                    if let Some(prev_span) = seen.insert(fname.name.clone(), *fspan) {
                        let _ = prev_span;
                        self.record_error(TypeError::DuplicateStructField {
                            field_name: fname.name.clone(),
                            span: *fspan,
                        });
                        continue;
                    }

                    let expected_field_ty = def
                        .iter()
                        .find(|(n, _)| n == &fname.name)
                        .map(|(_, t)| t.clone());

                    if let Some(ref expected_ty) = expected_field_ty {
                        if let Some(actual_ty) = self.check_expr(value, Some(expected_ty)) {
                            if !actual_ty.is_compatible_with(expected_ty) {
                                self.record_error(TypeError::Mismatch {
                                    expected: expected_ty.clone(),
                                    found: actual_ty,
                                    span: value.span(),
                                });
                            }
                        }
                    } else {
                        self.record_error(TypeError::UnknownField {
                            struct_name: name.name.clone(),
                            field_name: fname.name.clone(),
                            span: *fspan,
                        });
                        // Still check the value expression for cascaded errors
                        let _ = self.check_expr(value, None);
                    }
                }

                // Report any fields that are in the definition but missing from the literal
                for (field_name, _) in &def {
                    if !seen.contains_key(field_name) {
                        self.record_error(TypeError::MissingStructField {
                            struct_name: name.name.clone(),
                            field_name: field_name.clone(),
                            span: *span,
                        });
                    }
                }

                Some(Type::Struct(name.name.clone()))
            }

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                let obj_ty = self.check_expr(object, None).unwrap_or(Type::Unknown);
                if matches!(obj_ty, Type::Unknown) {
                    return Some(Type::Unknown);
                }

                let struct_name = match &obj_ty {
                    Type::Struct(n) => n.clone(),
                    other => {
                        self.record_error(TypeError::UnknownField {
                            struct_name: other.to_string(),
                            field_name: field.name.clone(),
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                let def = self.struct_defs.get(&struct_name).cloned();
                if let Some(def) = def {
                    if let Some((_, field_ty)) = def.iter().find(|(n, _)| n == &field.name) {
                        Some(field_ty.clone())
                    } else {
                        self.record_error(TypeError::UnknownField {
                            struct_name,
                            field_name: field.name.clone(),
                            span: field.span,
                        });
                        Some(Type::Unknown)
                    }
                } else {
                    self.record_error(TypeError::UnknownStruct {
                        name: struct_name,
                        span: *span,
                    });
                    Some(Type::Unknown)
                }
            }
        }
    }

    /// Check a statement.
    /// Returns None if there was a fatal error, Some(()) otherwise.
    /// Non-fatal errors are recorded and checking continues.
    fn check_stmt(&mut self, stmt: &Stmt) -> Option<()> {
        match stmt {
            Stmt::VarDecl {
                name,
                ty,
                init,
                mutable,
                span,
            } => {
                // Resolve declared type if present
                let declared_ty = if let Some(ty) = ty {
                    self.resolve_type(ty)
                } else {
                    None
                };

                // Check initializer type with expected type hint
                // If declared type exists, pass it as expected for type inference
                let init_ty = if let Some(init_expr) = init {
                    self.check_expr(init_expr, declared_ty.as_ref())
                } else {
                    None
                };

                // Determine final type
                let final_ty = match (declared_ty, init_ty) {
                    (Some(decl), Some(init)) => {
                        // Both declared and initialized: types must match
                        if !init.is_compatible_with(&decl) {
                            self.record_error(TypeError::Mismatch {
                                expected: decl.clone(),
                                found: init,
                                span: *span,
                            });
                            // Use declared type to avoid cascading errors
                        }
                        decl
                    }
                    (Some(decl), None) => {
                        // Only declared: use declared type
                        decl
                    }
                    (None, Some(init)) => {
                        // Only initialized: infer from initializer (Phase 1: simple inference)
                        init
                    }
                    (None, None) => {
                        // Neither declared nor initialized: error
                        self.record_error(TypeError::UninitializedVariable {
                            name: name.name.clone(),
                            span: *span,
                        });
                        return None;
                    }
                };

                // Skip Unknown types to avoid cascading errors
                if matches!(final_ty, Type::Unknown) {
                    return Some(());
                }

                // Define variable in current scope
                if let Err(duplicate_name) =
                    self.symbols.define(name.name.clone(), final_ty, *mutable)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: name.span,
                    });
                    return None;
                }

                Some(())
            }

            Stmt::Assignment {
                target,
                value,
                span,
            } => {
                // Lookup the target variable first to get expected type
                let expected_ty = self.symbols.lookup(&target.name).map(|s| s.ty.clone());

                // Check the value expression with expected type hint
                let value_ty = self
                    .check_expr(value, expected_ty.as_ref())
                    .unwrap_or(Type::Unknown);

                // Lookup the target variable again for validation
                if let Some(symbol_info) = self.symbols.lookup(&target.name) {
                    // Check if variable is mutable
                    if !symbol_info.mutable {
                        self.record_error(TypeError::AssignToImmutable {
                            name: target.name.clone(),
                            span: target.span,
                        });
                        return None;
                    }

                    // Check type compatibility (skip if value type is unknown)
                    if !matches!(value_ty, Type::Unknown)
                        && !value_ty.is_compatible_with(&symbol_info.ty)
                    {
                        self.record_error(TypeError::Mismatch {
                            expected: symbol_info.ty.clone(),
                            found: value_ty,
                            span: *span,
                        });
                    }

                    Some(())
                } else {
                    // Variable not defined
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    None
                }
            }

            Stmt::Return { value, span } => {
                // Check return value with expected return type hint
                // Clone the expected type to avoid borrow checker issues
                let expected_return = self.current_function_return_type.clone();
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr, expected_return.as_ref())
                        .unwrap_or(Type::Unknown)
                } else {
                    Type::Void
                };

                // Check against expected return type (skip if return type is unknown)
                if let Some(expected) = &self.current_function_return_type {
                    if !matches!(return_ty, Type::Unknown)
                        && !return_ty.is_compatible_with(expected)
                    {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: expected.clone(),
                            found: return_ty,
                            span: *span,
                        });
                    }
                }

                Some(())
            }

            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span: _,
            } => {
                // Check condition is boolean - no type inference needed (must be bool)
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool)) {
                    if !cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: cond_ty,
                            span: condition.span(),
                        });
                    }
                }

                // Check then block
                self.symbols.push_scope();
                for stmt in then_block {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();

                // Check else-if blocks
                for (else_if_cond, else_if_stmts) in else_if_blocks {
                    if let Some(cond_ty) = self.check_expr(else_if_cond, Some(&Type::Bool)) {
                        if !cond_ty.is_bool() {
                            self.record_error(TypeError::Mismatch {
                                expected: Type::Bool,
                                found: cond_ty,
                                span: else_if_cond.span(),
                            });
                        }
                    }

                    self.symbols.push_scope();
                    for stmt in else_if_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                }

                // Check else block
                if let Some(else_stmts) = else_block {
                    self.symbols.push_scope();
                    for stmt in else_stmts {
                        let _ = self.check_stmt(stmt);
                    }
                    self.symbols.pop_scope();
                }

                Some(())
            }

            Stmt::While {
                condition,
                body,
                span: _,
            } => {
                if let Some(cond_ty) = self.check_expr(condition, Some(&Type::Bool)) {
                    if !cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: cond_ty,
                            span: condition.span(),
                        });
                    }
                }

                self.loop_depth += 1;
                self.symbols.push_scope();
                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }
                self.symbols.pop_scope();
                self.loop_depth -= 1;

                Some(())
            }

            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive: _,
                body,
                span: _,
            } => {
                let start_ty = self.check_expr(start, None).unwrap_or(Type::Unknown);
                if !matches!(start_ty, Type::Unknown) && !start_ty.is_integer() {
                    self.record_error(TypeError::InvalidForRangeType {
                        found: start_ty.clone(),
                        span: start.span(),
                    });
                }

                let end_ty = self
                    .check_expr(end, Some(&start_ty))
                    .unwrap_or(Type::Unknown);
                if !matches!(end_ty, Type::Unknown) && !end_ty.is_integer() {
                    self.record_error(TypeError::InvalidForRangeType {
                        found: end_ty.clone(),
                        span: end.span(),
                    });
                }

                if !matches!(start_ty, Type::Unknown)
                    && !matches!(end_ty, Type::Unknown)
                    && !end_ty.is_compatible_with(&start_ty)
                {
                    self.record_error(TypeError::Mismatch {
                        expected: start_ty.clone(),
                        found: end_ty,
                        span: end.span(),
                    });
                }

                self.loop_depth += 1;
                self.symbols.push_scope();

                if !matches!(start_ty, Type::Unknown) {
                    if let Err(duplicate_name) =
                        self.symbols.define(iterator.name.clone(), start_ty, false)
                    {
                        self.record_error(TypeError::VariableAlreadyDefined {
                            name: duplicate_name,
                            span: iterator.span,
                        });
                    }
                }

                for stmt in body {
                    let _ = self.check_stmt(stmt);
                }

                self.symbols.pop_scope();
                self.loop_depth -= 1;

                Some(())
            }

            Stmt::Break { span } => {
                if self.loop_depth == 0 {
                    self.record_error(TypeError::BreakOutsideLoop { span: *span });
                }

                Some(())
            }

            Stmt::Continue { span } => {
                if self.loop_depth == 0 {
                    self.record_error(TypeError::ContinueOutsideLoop { span: *span });
                }

                Some(())
            }

            Stmt::FieldAssignment {
                object,
                field,
                value,
                span,
            } => {
                let symbol = if let Some(s) = self.symbols.lookup(&object.name) {
                    s.clone()
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: object.name.clone(),
                        span: object.span,
                    });
                    return None;
                };

                if !symbol.mutable {
                    self.record_error(TypeError::AssignToImmutableField {
                        var_name: object.name.clone(),
                        field_name: field.name.clone(),
                        span: *span,
                    });
                    return None;
                }

                let struct_name = match &symbol.ty {
                    Type::Struct(n) => n.clone(),
                    other => {
                        self.record_error(TypeError::UnknownField {
                            struct_name: other.to_string(),
                            field_name: field.name.clone(),
                            span: field.span,
                        });
                        return None;
                    }
                };

                let field_ty = {
                    let def = self.struct_defs.get(&struct_name).cloned();
                    if let Some(def) = def {
                        def.iter()
                            .find(|(n, _)| n == &field.name)
                            .map(|(_, t)| t.clone())
                    } else {
                        None
                    }
                };

                if let Some(expected_ty) = field_ty {
                    if let Some(actual_ty) = self.check_expr(value, Some(&expected_ty)) {
                        if !actual_ty.is_compatible_with(&expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty,
                                found: actual_ty,
                                span: *span,
                            });
                        }
                    }
                } else {
                    self.record_error(TypeError::UnknownField {
                        struct_name,
                        field_name: field.name.clone(),
                        span: field.span,
                    });
                    return None;
                }

                Some(())
            }

            Stmt::Expr(expr) => {
                // Expression statements have no expected type context
                let _ = self.check_expr(expr, None);
                Some(())
            }
        }
    }

    /// Check a function definition
    fn check_function(&mut self, func: &FunctionDef) -> Option<()> {
        // Check for duplicate parameter names
        use std::collections::HashSet;
        let mut param_names = HashSet::new();
        for param in &func.params {
            if !param_names.insert(&param.name.name) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: param.name.name.clone(),
                    span: param.name.span,
                });
            }
        }

        // Resolve parameter types
        let mut param_types = Vec::new();
        for param in &func.params {
            if let Some(param_ty) = self.resolve_type(&param.ty) {
                param_types.push(param_ty);
            } else {
                // Skip this parameter if type resolution failed
                param_types.push(Type::Unknown);
            }
        }

        // Resolve return type (default to Void if not specified)
        let return_type = if let Some(ret_ty) = &func.return_type {
            self.resolve_type(ret_ty).unwrap_or(Type::Void)
        } else {
            Type::Void
        };

        // Register function signature
        let func_ty = Type::Function {
            params: param_types.clone(),
            ret: Box::new(return_type.clone()),
        };

        if self.functions.contains_key(&func.name.name) {
            self.record_error(TypeError::FunctionAlreadyDefined {
                name: func.name.name.clone(),
                span: func.name.span,
            });
            return None;
        }

        self.functions.insert(func.name.name.clone(), func_ty);

        // Enter function scope
        self.symbols.push_scope();
        self.current_function_return_type = Some(return_type.clone());

        // Define parameters in function scope (parameters are immutable by default)
        for (param, param_ty) in func.params.iter().zip(param_types.iter()) {
            // Skip Unknown types to avoid cascading errors
            if matches!(param_ty, Type::Unknown) {
                continue;
            }

            if let Err(duplicate_name) = self.symbols.define(
                param.name.name.clone(),
                param_ty.clone(),
                false, // Function parameters are immutable
            ) {
                self.record_error(TypeError::VariableAlreadyDefined {
                    name: duplicate_name,
                    span: param.name.span,
                });
            }
        }

        // Check function body
        for stmt in &func.body {
            let _ = self.check_stmt(stmt);
        }

        // Validate trailing expressions for expression-based returns
        // If the last statement is an expression, it must match the return type
        if !matches!(return_type, Type::Void) && !func.body.is_empty() {
            if let Some(Stmt::Expr(expr)) = func.body.last() {
                // Trailing expression - validate it matches return type with type inference
                if let Some(expr_type) = self.check_expr(expr, Some(&return_type)) {
                    if !expr_type.is_compatible_with(&return_type) {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: return_type.clone(),
                            found: expr_type,
                            span: expr.span(),
                        });
                    }
                }
                // Note: If check_expr failed, the error is already recorded
            }
            // Note: Other statement types at the end are allowed - LLVM will catch missing returns
        }

        // Exit function scope
        self.symbols.pop_scope();
        self.current_function_return_type = None;

        Some(())
    }

    /// Register a struct definition without checking field initializers.
    /// Called in the pre-registration pass so that structs can be referenced
    /// by functions and other structs defined later in the file.
    fn register_struct(&mut self, def: &StructDef) -> Option<()> {
        if self.struct_defs.contains_key(&def.name.name) {
            self.record_error(TypeError::StructAlreadyDefined {
                name: def.name.name.clone(),
                span: def.name.span,
            });
            return None;
        }

        let mut fields: Vec<(String, Type)> = Vec::new();
        for field in &def.fields {
            if let Some(ty) = self.resolve_type(&field.ty) {
                fields.push((field.name.name.clone(), ty));
            }
        }

        self.struct_defs.insert(def.name.name.clone(), fields);
        Some(())
    }

    /// Register all method signatures from an `impl` block into the global
    /// function table under mangled names (`StructName__methodName`).
    ///
    /// Unsupported self-param variants (`&mut self`, consuming `self`) are
    /// rejected here so they never reach codegen.
    fn register_impl(&mut self, def: &ImplDef) -> Option<()> {
        if !self.struct_defs.contains_key(&def.type_name.name) {
            self.record_error(TypeError::UnknownStruct {
                name: def.type_name.name.clone(),
                span: def.type_name.span,
            });
            return None;
        }

        let struct_name = def.type_name.name.clone();

        // Accumulate (method_name, mangled_key) to insert into impl_methods after
        // all mutable borrows of `self` for type resolution are finished.
        let mut method_entries: Vec<(String, String)> = Vec::new();

        for method in &def.methods {
            // Reject self-param variants that require ownership semantics.
            match &method.self_param {
                Some(SelfParam::RefMut) => {
                    self.errors.push(TypeError::UnsupportedSelfParam {
                        type_name: struct_name.clone(),
                        self_param: "&mut self".to_string(),
                        span: method.span,
                    });
                    continue;
                }
                Some(SelfParam::Owned) => {
                    self.errors.push(TypeError::UnsupportedSelfParam {
                        type_name: struct_name.clone(),
                        self_param: "self".to_string(),
                        span: method.span,
                    });
                    continue;
                }
                _ => {}
            }

            let mangled = format!("{}__{}", struct_name, method.name.name);

            // Build the full parameter type list: implicit `self` first for instance methods.
            let mut param_types: Vec<Type> = Vec::new();
            if method.self_param.is_some() {
                param_types.push(Type::Struct(struct_name.clone()));
            }
            for param in &method.params {
                if let Some(ty) = self.resolve_type(&param.ty) {
                    param_types.push(ty);
                } else {
                    param_types.push(Type::Unknown);
                }
            }

            let return_type = if let Some(ret_ty) = &method.return_type {
                self.resolve_type(ret_ty).unwrap_or(Type::Void)
            } else {
                Type::Void
            };

            let func_ty = Type::Function {
                params: param_types,
                ret: Box::new(return_type),
            };

            if self.functions.contains_key(&mangled) {
                self.record_error(TypeError::FunctionAlreadyDefined {
                    name: mangled.clone(),
                    span: method.name.span,
                });
                continue;
            }

            self.functions.insert(mangled.clone(), func_ty);
            method_entries.push((method.name.name.clone(), mangled));
        }

        // Insert collected entries now that all borrows of `self` are released.
        let method_map = self.impl_methods.entry(struct_name).or_default();
        for (name, mangled) in method_entries {
            method_map.insert(name, mangled);
        }

        Some(())
    }

    /// Type-check the body of each method in an `impl` block.
    fn check_impl(&mut self, def: &ImplDef) {
        let struct_name = def.type_name.name.clone();

        for method in &def.methods {
            // Skip methods that were already rejected during registration.
            if matches!(
                method.self_param,
                Some(SelfParam::RefMut) | Some(SelfParam::Owned)
            ) {
                continue;
            }

            let mangled = format!("{}__{}", struct_name, method.name.name);

            let func_ty = match self.functions.get(&mangled).cloned() {
                Some(ty) => ty,
                None => continue,
            };

            let (param_types, return_type) = match func_ty {
                Type::Function { params, ret } => (params, *ret),
                _ => continue,
            };

            self.symbols.push_scope();
            self.current_function_return_type = Some(return_type.clone());

            // Bind `self` as an immutable variable of the struct type (for &self methods).
            if method.self_param.is_some() {
                let self_ty = Type::Struct(struct_name.clone());
                let _ = self.symbols.define("self".to_string(), self_ty, false);
            }

            // Bind remaining parameters (skip param[0] which is the implicit self).
            let non_self_params = if method.self_param.is_some() && !param_types.is_empty() {
                &param_types[1..]
            } else {
                &param_types[..]
            };

            for (param, param_ty) in method.params.iter().zip(non_self_params.iter()) {
                if matches!(param_ty, Type::Unknown) {
                    continue;
                }
                if let Err(dup) =
                    self.symbols
                        .define(param.name.name.clone(), param_ty.clone(), false)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: dup,
                        span: param.name.span,
                    });
                }
            }

            for stmt in &method.body {
                let _ = self.check_stmt(stmt);
            }

            // Validate trailing expression return (same rule as free functions).
            if !matches!(return_type, Type::Void) && !method.body.is_empty() {
                if let Some(Stmt::Expr(expr)) = method.body.last() {
                    if let Some(expr_type) = self.check_expr(expr, Some(&return_type)) {
                        if !expr_type.is_compatible_with(&return_type) {
                            self.record_error(TypeError::ReturnTypeMismatch {
                                expected: return_type.clone(),
                                found: expr_type,
                                span: expr.span(),
                            });
                        }
                    }
                }
            }

            self.symbols.pop_scope();
            self.current_function_return_type = None;
        }
    }

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Pass 1: register struct definitions so type names resolve in method signatures.
        for item in items {
            if let Item::Struct(def) = item {
                let _ = self.register_struct(def);
            }
        }

        // Pass 2: register impl method signatures (uses struct_defs from pass 1).
        for item in items {
            if let Item::Impl(def) = item {
                let _ = self.register_impl(def);
            }
        }

        // Pass 3: check function and method bodies.
        for item in items {
            match item {
                Item::Function(func) => {
                    let _ = self.check_function(func);
                }
                Item::Impl(def) => self.check_impl(def),
                Item::Struct(_) => {}
            }
        }

        if self.has_errors() {
            Err(())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::TypeError;
    use ast_types::{Expr, FunctionDef, Parameter, Stmt};
    use shared_types::{Identifier, Span};

    fn make_ident(name: &str) -> Identifier {
        Identifier {
            name: name.to_string(),
            span: Span::new(0, 0),
        }
    }

    fn make_type(name: &str) -> ast_types::Type {
        ast_types::Type::Named(make_ident(name))
    }

    /// Helper to create a simple function for testing
    fn make_function(
        name: &str,
        params: Vec<(String, String)>,
        return_type: Option<String>,
        body: Vec<Stmt>,
    ) -> FunctionDef {
        FunctionDef {
            name: make_ident(name),
            params: params
                .into_iter()
                .map(|(pname, pty)| Parameter {
                    name: make_ident(&pname),
                    ty: make_type(&pty),
                    span: Span::new(0, 0),
                })
                .collect(),
            return_type: return_type.map(|rt| make_type(&rt)),
            body,
            span: Span::new(0, 0),
        }
    }

    #[test]
    fn test_integer_literal_infers_from_variable_declaration() {
        // val x: i64 = 42
        // The literal 42 should infer as i64, not i32
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("i64")),
            init: Some(Expr::Literal(Literal::Integer(42), Span::new(0, 2))),
            mutable: false,
            span: Span::new(0, 10),
        };

        assert!(checker.check_stmt(&stmt).is_some());
        assert!(!checker.has_errors());

        // Verify variable has correct type
        let symbol_info = checker.symbols.lookup("x").unwrap();
        assert_eq!(symbol_info.ty, Type::I64);
    }

    #[test]
    fn test_integer_literal_infers_from_function_parameter() {
        // func foo(x: u32) {}
        // foo(42) - the literal 42 should infer as u32
        let mut checker = TypeChecker::new();

        // Define function
        let func = make_function(
            "foo",
            vec![("x".to_string(), "u32".to_string())],
            None,
            vec![],
        );

        checker.check_function(&func);
        assert!(!checker.has_errors());

        // Call with literal
        let call_expr = Expr::Call {
            func: Box::new(Expr::Identifier(make_ident("foo"))),
            args: vec![Expr::Literal(Literal::Integer(42), Span::new(0, 2))],
            span: Span::new(0, 10),
        };

        let result_ty = checker.check_expr(&call_expr, None);
        assert_eq!(result_ty, Some(Type::Void));
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_integer_literal_out_of_range_i8() {
        // val x: i8 = 300  - should error (i8 max is 127)
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("i8")),
            init: Some(Expr::Literal(Literal::Integer(300), Span::new(0, 3))),
            mutable: false,
            span: Span::new(0, 10),
        };

        checker.check_stmt(&stmt);
        assert!(checker.has_errors());

        let errors = checker.into_errors();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
                assert_eq!(*value, 300);
                assert_eq!(*ty, Type::I8);
            }
            _ => panic!("Expected IntegerLiteralOutOfRange error"),
        }
    }

    #[test]
    fn test_integer_literal_negative_u32() {
        // val x: u32 = -42  - should error (u32 can't be negative)
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("u32")),
            init: Some(Expr::Literal(Literal::Integer(-42), Span::new(0, 3))),
            mutable: false,
            span: Span::new(0, 10),
        };

        checker.check_stmt(&stmt);
        assert!(checker.has_errors());

        let errors = checker.into_errors();
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            TypeError::IntegerLiteralOutOfRange { value, ty, .. } => {
                assert_eq!(*value, -42);
                assert_eq!(*ty, Type::U32);
            }
            _ => panic!("Expected IntegerLiteralOutOfRange error"),
        }
    }

    #[test]
    fn test_float_literal_infers_f32() {
        // val x: f32 = 2.5
        // The literal 2.5 should infer as f32, not f64
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("f32")),
            init: Some(Expr::Literal(Literal::Float(2.5), Span::new(0, 3))),
            mutable: false,
            span: Span::new(0, 10),
        };

        assert!(checker.check_stmt(&stmt).is_some());
        assert!(!checker.has_errors());

        // Verify variable has correct type
        let symbol_info = checker.symbols.lookup("x").unwrap();
        assert_eq!(symbol_info.ty, Type::F32);
    }

    #[test]
    fn test_literal_inference_in_return() {
        // func foo() -> i16 { 42 }
        // The literal 42 should infer as i16
        let mut checker = TypeChecker::new();

        let func = make_function(
            "foo",
            vec![],
            Some("i16".to_string()),
            vec![Stmt::Expr(Expr::Literal(
                Literal::Integer(42),
                Span::new(0, 2),
            ))],
        );

        checker.check_function(&func);
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_literal_inference_in_assignment() {
        // mut x: u64 = 100
        // x = 200
        // The literal 200 should infer as u64
        let mut checker = TypeChecker::new();

        let decl = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("u64")),
            init: Some(Expr::Literal(Literal::Integer(100), Span::new(0, 3))),
            mutable: true,
            span: Span::new(0, 15),
        };

        checker.check_stmt(&decl);
        assert!(!checker.has_errors());

        let assign = Stmt::Assignment {
            target: make_ident("x"),
            value: Expr::Literal(Literal::Integer(200), Span::new(0, 3)),
            span: Span::new(0, 7),
        };

        checker.check_stmt(&assign);
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_literal_defaults_to_i32_without_context() {
        // val x = 42 (no type annotation)
        // Should default to i32
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: None,
            init: Some(Expr::Literal(Literal::Integer(42), Span::new(0, 2))),
            mutable: false,
            span: Span::new(0, 10),
        };

        checker.check_stmt(&stmt);
        assert!(!checker.has_errors());

        // Verify variable has i32 type
        let symbol_info = checker.symbols.lookup("x").unwrap();
        assert_eq!(symbol_info.ty, Type::I32);
    }

    #[test]
    fn test_literal_defaults_to_f64_without_context() {
        // val x = 2.5 (no type annotation)
        // Should default to f64
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: None,
            init: Some(Expr::Literal(Literal::Float(2.5), Span::new(0, 3))),
            mutable: false,
            span: Span::new(0, 10),
        };

        checker.check_stmt(&stmt);
        assert!(!checker.has_errors());

        // Verify variable has f64 type
        let symbol_info = checker.symbols.lookup("x").unwrap();
        assert_eq!(symbol_info.ty, Type::F64);
    }

    #[test]
    fn test_literal_inference_in_binary_operation() {
        // val x: i16 = 10
        // val y: i16 = x + 5
        // The literal 5 should infer as i16 from x
        let mut checker = TypeChecker::new();

        let decl_x = Stmt::VarDecl {
            name: make_ident("x"),
            ty: Some(make_type("i16")),
            init: Some(Expr::Literal(Literal::Integer(10), Span::new(0, 2))),
            mutable: false,
            span: Span::new(0, 10),
        };

        checker.check_stmt(&decl_x);
        assert!(!checker.has_errors());

        let decl_y = Stmt::VarDecl {
            name: make_ident("y"),
            ty: Some(make_type("i16")),
            init: Some(Expr::Binary {
                left: Box::new(Expr::Identifier(make_ident("x"))),
                op: BinaryOp::Add,
                right: Box::new(Expr::Literal(Literal::Integer(5), Span::new(0, 1))),
                span: Span::new(0, 5),
            }),
            mutable: false,
            span: Span::new(0, 15),
        };

        checker.check_stmt(&decl_y);
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_large_literal_auto_promotes_to_i64() {
        // val x = 5000000000  (too large for i32)
        // Should automatically use i64
        let mut checker = TypeChecker::new();

        let stmt = Stmt::VarDecl {
            name: make_ident("x"),
            ty: None,
            init: Some(Expr::Literal(
                Literal::Integer(5000000000),
                Span::new(0, 10),
            )),
            mutable: false,
            span: Span::new(0, 15),
        };

        checker.check_stmt(&stmt);
        assert!(!checker.has_errors());

        // Verify variable has i64 type
        let symbol_info = checker.symbols.lookup("x").unwrap();
        assert_eq!(symbol_info.ty, Type::I64);
    }

    #[test]
    fn test_for_range_accepts_integer_bounds() {
        let mut checker = TypeChecker::new();

        let stmt = Stmt::ForRange {
            iterator: make_ident("i"),
            start: Expr::Literal(Literal::Integer(0), Span::new(0, 1)),
            end: Expr::Literal(Literal::Integer(5), Span::new(4, 5)),
            inclusive: false,
            body: vec![Stmt::Continue {
                span: Span::new(8, 16),
            }],
            span: Span::new(0, 16),
        };

        checker.check_stmt(&stmt);
        assert!(!checker.has_errors());
    }

    #[test]
    fn test_for_range_rejects_non_integer_bound() {
        let mut checker = TypeChecker::new();

        let stmt = Stmt::ForRange {
            iterator: make_ident("i"),
            start: Expr::Literal(Literal::Boolean(true), Span::new(0, 4)),
            end: Expr::Literal(Literal::Integer(5), Span::new(7, 8)),
            inclusive: false,
            body: vec![],
            span: Span::new(0, 12),
        };

        checker.check_stmt(&stmt);
        assert!(checker.has_errors());

        let errors = checker.into_errors();
        assert!(errors
            .iter()
            .any(|error| matches!(error, TypeError::InvalidForRangeType { .. })));
    }
}
