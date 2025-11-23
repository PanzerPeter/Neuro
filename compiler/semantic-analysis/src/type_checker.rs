// NEURO Programming Language - Semantic Analysis
// Main type checking engine

use std::collections::HashMap;

use shared_types::{Literal, Span};
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

    /// Convert syntax-parsing type to semantic type
    fn resolve_type(&mut self, ty: &syntax_parsing::Type) -> Result<Type, ()> {
        match ty {
            syntax_parsing::Type::Named(ident) => match ident.name.as_str() {
                // Signed integers
                "i8" => Ok(Type::I8),
                "i16" => Ok(Type::I16),
                "i32" => Ok(Type::I32),
                "i64" => Ok(Type::I64),
                // Unsigned integers
                "u8" => Ok(Type::U8),
                "u16" => Ok(Type::U16),
                "u32" => Ok(Type::U32),
                "u64" => Ok(Type::U64),
                // Floating point
                "f32" => Ok(Type::F32),
                "f64" => Ok(Type::F64),
                // Other types
                "bool" => Ok(Type::Bool),
                "void" => Ok(Type::Void),
                name => {
                    self.record_error(TypeError::UnknownTypeName {
                        name: name.to_string(),
                        span: ident.span,
                    });
                    Err(())
                }
            },
            syntax_parsing::Type::Tensor { .. } => {
                // Tensor types are Phase 3, not supported in Phase 1
                self.record_error(TypeError::UnknownTypeName {
                    name: "Tensor".to_string(),
                    span: Span::new(0, 0),
                });
                Err(())
            }
        }
    }

    /// Check an expression and return its type
    fn check_expr(&mut self, expr: &Expr) -> Result<Type, ()> {
        match expr {
            Expr::Literal(lit, _span) => match lit {
                Literal::Integer(_) => Ok(Type::I32), // Default integer type
                Literal::Float(_) => Ok(Type::F64),   // Default float type
                Literal::Boolean(_) => Ok(Type::Bool),
                Literal::String(_) => {
                    // String type not in Phase 1 spec, treat as error
                    self.record_error(TypeError::UnknownTypeName {
                        name: "string".to_string(),
                        span: expr.span(),
                    });
                    Err(())
                }
            },

            Expr::Identifier(ident) => {
                if let Some(symbol_info) = self.symbols.lookup(&ident.name) {
                    Ok(symbol_info.ty.clone())
                } else {
                    self.record_error(TypeError::UndefinedVariable {
                        name: ident.name.clone(),
                        span: ident.span,
                    });
                    Err(())
                }
            }

            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                let left_ty = self.check_expr(left)?;
                let right_ty = self.check_expr(right)?;

                match op {
                    // Arithmetic operators: require numeric types, return same type
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => {
                        if !left_ty.is_numeric() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: format!("{:?}", op),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Err(());
                        }

                        if !left_ty.is_compatible_with(&right_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: left_ty.clone(),
                                found: right_ty,
                                span: *span,
                            });
                            return Err(());
                        }

                        Ok(left_ty)
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
                            return Err(());
                        }
                        Ok(Type::Bool)
                    }

                    // Logical operators: require bool types, return bool
                    BinaryOp::And | BinaryOp::Or => {
                        if !left_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: format!("{:?}", op),
                                left: left_ty,
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Err(());
                        }

                        if !right_ty.is_bool() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: format!("{:?}", op),
                                left: Type::Bool,
                                right: right_ty,
                                span: *span,
                            });
                            return Err(());
                        }

                        Ok(Type::Bool)
                    }
                }
            }

            Expr::Unary { op, operand, span } => {
                let operand_ty = self.check_expr(operand)?;

                match op {
                    UnaryOp::Negate => {
                        if !operand_ty.is_numeric() {
                            self.record_error(TypeError::InvalidOperator {
                                op: "negate".to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Err(());
                        }
                        Ok(operand_ty)
                    }
                    UnaryOp::Not => {
                        if !operand_ty.is_bool() {
                            self.record_error(TypeError::InvalidOperator {
                                op: "not".to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Err(());
                        }
                        Ok(Type::Bool)
                    }
                }
            }

            Expr::Call { func, args, span } => {
                // Get function name (for Phase 1, only identifier calls supported)
                let func_name = match &**func {
                    Expr::Identifier(ident) => &ident.name,
                    _ => {
                        self.record_error(TypeError::NotCallable {
                            ty: Type::Unknown,
                            span: *span,
                        });
                        return Err(());
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
                    return Err(());
                };

                // Extract parameter types and return type
                let (param_types, return_type) = match func_ty {
                    Type::Function { params, ret } => (params, *ret),
                    _ => {
                        self.record_error(TypeError::NotCallable {
                            ty: func_ty,
                            span: *span,
                        });
                        return Err(());
                    }
                };

                // Check argument count
                if args.len() != param_types.len() {
                    self.record_error(TypeError::ArgumentCountMismatch {
                        expected: param_types.len(),
                        found: args.len(),
                        span: *span,
                    });
                    return Err(());
                }

                // Check each argument type
                for (arg, expected_ty) in args.iter().zip(param_types.iter()) {
                    if let Ok(arg_ty) = self.check_expr(arg) {
                        if !arg_ty.is_compatible_with(expected_ty) {
                            self.record_error(TypeError::Mismatch {
                                expected: expected_ty.clone(),
                                found: arg_ty,
                                span: arg.span(),
                            });
                        }
                    }
                }

                Ok(return_type)
            }

            Expr::Paren(inner, _) => self.check_expr(inner),
        }
    }

    /// Check a statement
    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), ()> {
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
                    Some(self.resolve_type(ty)?)
                } else {
                    None
                };

                // Check initializer type if present
                let init_ty = if let Some(init_expr) = init {
                    Some(self.check_expr(init_expr)?)
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
                        return Err(());
                    }
                };

                // Define variable in current scope
                if let Err(duplicate_name) =
                    self.symbols.define(name.name.clone(), final_ty, *mutable)
                {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: name.span,
                    });
                    return Err(());
                }

                Ok(())
            }

            Stmt::Assignment {
                target,
                value,
                span,
            } => {
                // Check the value expression
                let value_ty = self.check_expr(value)?;

                // Lookup the target variable
                if let Some(symbol_info) = self.symbols.lookup(&target.name) {
                    // Check if variable is mutable
                    if !symbol_info.mutable {
                        self.record_error(TypeError::AssignToImmutable {
                            name: target.name.clone(),
                            span: target.span,
                        });
                        return Err(());
                    }

                    // Check type compatibility
                    if !value_ty.is_compatible_with(&symbol_info.ty) {
                        self.record_error(TypeError::Mismatch {
                            expected: symbol_info.ty.clone(),
                            found: value_ty,
                            span: *span,
                        });
                        return Err(());
                    }

                    Ok(())
                } else {
                    // Variable not defined
                    self.record_error(TypeError::UndefinedVariable {
                        name: target.name.clone(),
                        span: target.span,
                    });
                    Err(())
                }
            }

            Stmt::Return { value, span } => {
                let return_ty = if let Some(expr) = value {
                    self.check_expr(expr)?
                } else {
                    Type::Void
                };

                // Check against expected return type
                if let Some(expected) = &self.current_function_return_type {
                    if !return_ty.is_compatible_with(expected) {
                        self.record_error(TypeError::ReturnTypeMismatch {
                            expected: expected.clone(),
                            found: return_ty,
                            span: *span,
                        });
                        return Err(());
                    }
                }

                Ok(())
            }

            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span: _,
            } => {
                // Check condition is boolean
                if let Ok(cond_ty) = self.check_expr(condition) {
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
                    if let Ok(cond_ty) = self.check_expr(else_if_cond) {
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

                Ok(())
            }

            Stmt::Expr(expr) => {
                let _ = self.check_expr(expr);
                Ok(())
            }
        }
    }

    /// Check a function definition
    fn check_function(&mut self, func: &FunctionDef) -> Result<(), ()> {
        // Resolve parameter types
        let mut param_types = Vec::new();
        for param in &func.params {
            let param_ty = self.resolve_type(&param.ty)?;
            param_types.push(param_ty);
        }

        // Resolve return type (default to Void if not specified)
        let return_type = if let Some(ret_ty) = &func.return_type {
            self.resolve_type(ret_ty)?
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
            return Err(());
        }

        self.functions.insert(func.name.name.clone(), func_ty);

        // Enter function scope
        self.symbols.push_scope();
        self.current_function_return_type = Some(return_type.clone());

        // Define parameters in function scope (parameters are immutable by default)
        for (param, param_ty) in func.params.iter().zip(param_types.iter()) {
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

        // Exit function scope
        self.symbols.pop_scope();
        self.current_function_return_type = None;

        Ok(())
    }

    /// Check a complete program
    pub(crate) fn check_program(&mut self, items: &[Item]) -> Result<(), ()> {
        // Phase 1: Only function definitions are supported
        for item in items {
            match item {
                Item::Function(func) => {
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
