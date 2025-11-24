// NEURO Programming Language - Semantic Analysis
// Main type checking engine

use std::collections::HashMap;

use shared_types::Literal;
use syntax_parsing::{BinaryOp, Expr, FunctionDef, Item, Stmt, UnaryOp};

use crate::errors::TypeError;
use crate::symbol_table::SymbolTable;
use crate::types::Type;

/// Type checker state
pub(crate) struct TypeChecker {
    /// Symbol table for variables
    symbols: SymbolTable,
    /// Function signatures (global scope)
    functions: HashMap<String, Type>,
    /// Collected type errors
    errors: Vec<TypeError>,
    /// Current function's return type (for validating return statements)
    current_function_return_type: Option<Type>,
}

impl TypeChecker {
    pub(crate) fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            functions: HashMap::new(),
            errors: Vec::new(),
            current_function_return_type: None,
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

    /// Convert syntax-parsing type to semantic type.
    /// Returns None if the type is unknown (error is recorded).
    fn resolve_type(&mut self, ty: &syntax_parsing::Type) -> Option<Type> {
        match ty {
            syntax_parsing::Type::Named(ident) => match ident.name.as_str() {
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
                    self.record_error(TypeError::UnknownTypeName {
                        name: name.to_string(),
                        span: ident.span,
                    });
                    None
                }
            },
            syntax_parsing::Type::Tensor { span, .. } => {
                // Tensor types are Phase 3, not supported in Phase 1
                self.record_error(TypeError::UnknownTypeName {
                    name: "Tensor".to_string(),
                    span: *span,
                });
                None
            }
        }
    }

    /// Check an expression and return its type.
    /// Returns None if there was an error (which has been recorded).
    /// Use this for better error recovery - checking can continue with Unknown type.
    fn check_expr(&mut self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Literal(lit, _span) => match lit {
                Literal::Integer(_) => Some(Type::I32), // Default integer type
                Literal::Float(_) => Some(Type::F64),   // Default float type
                Literal::Boolean(_) => Some(Type::Bool),
                Literal::String(_) => Some(Type::String), // String literals have string type
            },

            Expr::Identifier(ident) => {
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
                let left_ty = self.check_expr(left).unwrap_or(Type::Unknown);
                let right_ty = self.check_expr(right).unwrap_or(Type::Unknown);

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
                let operand_ty = self.check_expr(operand).unwrap_or(Type::Unknown);

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

            Expr::Call { func, args, span } => {
                // Get function name (for Phase 1, only identifier calls supported)
                let func_name = match &**func {
                    Expr::Identifier(ident) => &ident.name,
                    _ => {
                        // Try to infer the type of the expression being called
                        let expr_ty = self.check_expr(func).unwrap_or(Type::Unknown);
                        self.record_error(TypeError::NotCallable {
                            ty: expr_ty,
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                // Look up function signature
                let func_ty = if let Some(ty) = self.functions.get(func_name) {
                    ty.clone()
                } else {
                    self.record_error(TypeError::UndefinedFunction {
                        name: func_name.clone(),
                        span: *span,
                    });
                    return Some(Type::Unknown);
                };

                // Extract parameter types and return type
                let (param_types, return_type) = match func_ty {
                    Type::Function { params, ret } => (params, *ret),
                    _ => {
                        self.record_error(TypeError::NotCallable {
                            ty: func_ty,
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                };

                // Check argument count
                if args.len() != param_types.len() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: param_types.len(),
                        found: args.len(),
                        span: *span,
                    });
                    // Continue checking argument types for better error reporting
                }

                // Check each argument type (continue even if count mismatch)
                for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
                    if let Some(arg_ty) = self.check_expr(arg) {
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

            Expr::Paren(inner, _) => self.check_expr(inner),
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

                // Check initializer type if present
                let init_ty = if let Some(init_expr) = init {
                    self.check_expr(init_expr)
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
                // Check the value expression
                let value_ty = self.check_expr(value).unwrap_or(Type::Unknown);

                // Lookup the target variable
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
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr).unwrap_or(Type::Unknown)
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
                // Check condition is boolean
                if let Some(cond_ty) = self.check_expr(condition) {
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
                    if let Some(cond_ty) = self.check_expr(else_if_cond) {
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

            Stmt::Expr(expr) => {
                let _ = self.check_expr(expr);
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
                // Trailing expression - validate it matches return type
                if let Some(expr_type) = self.check_expr(expr) {
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

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Phase 1: Only function definitions are supported
        for item in items {
            match item {
                Item::Function(func) => {
                    // Continue checking even if function has errors
                    let _ = self.check_function(func);
                }
            }
        }

        if self.has_errors() {
            Err(())
        } else {
            Ok(())
        }
    }
}
