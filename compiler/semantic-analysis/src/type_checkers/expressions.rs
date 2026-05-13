use super::TypeChecker;
use crate::errors::TypeError;
use crate::types::Type;
use ast_types::FieldInit;
use ast_types::{BinaryOp, Expr, UnaryOp};
use shared_types::{Literal, Span};
use std::collections::HashMap;

impl TypeChecker {
    /// Type-check a plain identifier call (free function or previously registered
    /// method with a mangled name). Extracted so the `Call` arm can delegate here.
    pub(crate) fn check_plain_call(
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
                Literal::Float(_) => {
                    // Use contextual type inference for float literals
                    Some(self.infer_float_type(expected))
                }
                Literal::Boolean(_) => Some(Type::Bool),
                Literal::String(_) => Some(Type::String), // String literals have string type
            },

            Expr::Identifier(ident) => {
                // Variables take priority; constants are a fallback so locals can shadow consts.
                if let Some(symbol_info) = self.symbols.lookup(&ident.name) {
                    Some(symbol_info.ty.clone())
                } else if let Some(const_ty) = self.constants.get(&ident.name).cloned() {
                    Some(const_ty)
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
                    BinaryOp::Equal | BinaryOp::NotEqual => {
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

                    // Inequality operators: require numeric types (int/float), return bool
                    BinaryOp::Less
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

                        if !left_ty.is_numeric() {
                            self.record_error(TypeError::InvalidBinaryOperator {
                                op: op.to_string(),
                                left: left_ty.clone(),
                                right: right_ty.clone(),
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }

                        Some(Type::Bool)
                    }

                    // Bitwise operators: require integer types, return same type
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl => {
                        if !left_ty.is_integer() {
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
                    UnaryOp::Not => None,
                    UnaryOp::BitNot => expected.filter(|t| t.is_integer()),
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
                    UnaryOp::BitNot => {
                        if !operand_ty.is_integer() {
                            self.record_error(TypeError::InvalidOperator {
                                op: op.to_string(),
                                ty: operand_ty,
                                span: *span,
                            });
                            return Some(Type::Unknown);
                        }
                        Some(operand_ty)
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

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
            } => {
                let cond_ty = self
                    .check_expr(condition, Some(&Type::Bool))
                    .unwrap_or(Type::Unknown);
                if !matches!(cond_ty, Type::Unknown) && !cond_ty.is_bool() {
                    self.record_error(TypeError::Mismatch {
                        expected: Type::Bool,
                        found: cond_ty,
                        span: condition.span(),
                    });
                }

                // Collect arm types: then + each else-if + optional else
                let then_ty = self.check_block_expr_type(then_block);

                let mut arm_types: Vec<Type> = vec![then_ty.clone()];

                for (elif_cond, elif_block) in else_if_blocks {
                    let elif_cond_ty = self
                        .check_expr(elif_cond, Some(&Type::Bool))
                        .unwrap_or(Type::Unknown);
                    if !matches!(elif_cond_ty, Type::Unknown) && !elif_cond_ty.is_bool() {
                        self.record_error(TypeError::Mismatch {
                            expected: Type::Bool,
                            found: elif_cond_ty,
                            span: elif_cond.span(),
                        });
                    }
                    arm_types.push(self.check_block_expr_type(elif_block));
                }

                if let Some(else_stmts) = else_block {
                    arm_types.push(self.check_block_expr_type(else_stmts));
                } else {
                    return Some(Type::Void);
                }

                // All arms must agree on type
                let result_ty = arm_types[0].clone();
                for arm_ty in &arm_types[1..] {
                    if !arm_ty.is_compatible_with(&result_ty) {
                        self.record_error(TypeError::Mismatch {
                            expected: result_ty.clone(),
                            found: arm_ty.clone(),
                            span: *span,
                        });
                        return Some(Type::Unknown);
                    }
                }
                Some(result_ty)
            }

            Expr::Block { stmts, .. } => {
                self.symbols.push_scope();
                let ty = self.check_block_expr_type(stmts);
                self.symbols.pop_scope();
                Some(ty)
            }
        }
    }

    /// Check all stmts in a block and return the type of the trailing expression, or Void.
    fn check_block_expr_type(&mut self, stmts: &[ast_types::Stmt]) -> Type {
        self.symbols.push_scope();
        let mut result = Type::Void;
        for (i, stmt) in stmts.iter().enumerate() {
            if i == stmts.len() - 1 {
                if let ast_types::Stmt::Expr(expr) = stmt {
                    result = self.check_expr(expr, None).unwrap_or(Type::Unknown);
                    self.symbols.pop_scope();
                    return result;
                }
            }
            let _ = self.check_stmt(stmt);
        }
        self.symbols.pop_scope();
        result
    }
}
