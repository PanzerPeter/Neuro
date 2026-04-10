use ast_types::*;
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::CodegenContext;

impl<'ctx> CodegenContext<'ctx> {
    /// Store type information for expressions (needed for codegen)
    pub(crate) fn store_expr_types(
        &mut self,
        items: &[Item],
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        for item in items {
            match item {
                Item::Function(func_def) => {
                    self.visit_function_for_types(func_def, func_types)?;
                }
                Item::Impl(impl_def) => {
                    self.visit_impl_for_types(impl_def, func_types)?;
                }
                Item::Struct(_) => {}
            }
        }
        Ok(())
    }

    /// Walk an impl block's method bodies to populate the type-info maps.
    pub(crate) fn visit_impl_for_types(
        &mut self,
        impl_def: &ImplDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let struct_name = &impl_def.type_name.name;
        for method in &impl_def.methods {
            if matches!(
                method.self_param,
                Some(SelfParam::RefMut) | Some(SelfParam::Owned)
            ) {
                continue;
            }
            self.visit_method_for_types(method, struct_name, func_types)?;
        }
        Ok(())
    }

    pub(crate) fn visit_method_for_types(
        &mut self,
        method: &MethodDef,
        struct_name: &str,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        let mangled = format!("{}__{}", struct_name, method.name.name);
        self.type_env.clear();

        let func_type_info = func_types
            .get(&mangled)
            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;

        let param_types = match func_type_info {
            Type::Function { params, .. } => params,
            _ => {
                return Err(CodegenError::InternalError(
                    "method type is not a function type".to_string(),
                ))
            }
        };

        // param_types[0] is `self` for instance methods; bind it in type_env.
        if method.self_param.is_some() {
            self.type_env
                .insert("self".to_string(), Type::Struct(struct_name.to_string()));
        }

        let non_self_start = if method.self_param.is_some() { 1 } else { 0 };
        for (i, param) in method.params.iter().enumerate() {
            if let Some(ty) = param_types.get(non_self_start + i) {
                self.type_env.insert(param.name.name.clone(), ty.clone());
            }
        }

        for stmt in &method.body {
            self.visit_stmt_for_types(stmt, func_types)?;
        }
        Ok(())
    }

    pub(crate) fn visit_function_for_types(
        &mut self,
        func_def: &FunctionDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // Clear type environment for new function
        self.type_env.clear();

        // Populate type environment with parameter types
        let func_type_info = func_types
            .get(&func_def.name.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.name.clone()))?;

        let param_types = match func_type_info {
            Type::Function { params, .. } => params,
            _ => {
                return Err(CodegenError::InternalError(
                    "function type information is not a function type".to_string(),
                ))
            }
        };

        for (i, param) in func_def.params.iter().enumerate() {
            let param_ty = param_types.get(i).ok_or_else(|| {
                CodegenError::InternalError(format!("missing type for parameter {}", i))
            })?;
            self.type_env
                .insert(param.name.name.clone(), param_ty.clone());
        }

        for stmt in &func_def.body {
            self.visit_stmt_for_types(stmt, func_types)?;
        }
        Ok(())
    }

    pub(crate) fn visit_stmt_for_types(
        &mut self,
        stmt: &Stmt,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, init, .. } => {
                if let Some(expr) = init {
                    self.visit_expr_for_types(expr, func_types)?;
                    // Get the type of the initializer and store it for this variable
                    let var_ty = self.expr_types.get(&expr.span().start).ok_or_else(|| {
                        CodegenError::InternalError(
                            "missing type for variable initializer".to_string(),
                        )
                    })?;
                    self.type_env.insert(name.name.clone(), var_ty.clone());
                }
            }
            Stmt::Assignment { value, .. } => {
                // Visit the value expression to collect its type
                self.visit_expr_for_types(value, func_types)?;
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.visit_expr_for_types(expr, func_types)?;
                }
            }
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => {
                self.visit_expr_for_types(condition, func_types)?;
                for stmt in then_block {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
                for (cond, stmts) in else_if_blocks {
                    self.visit_expr_for_types(cond, func_types)?;
                    for stmt in stmts {
                        self.visit_stmt_for_types(stmt, func_types)?;
                    }
                }
                if let Some(stmts) = else_block {
                    for stmt in stmts {
                        self.visit_stmt_for_types(stmt, func_types)?;
                    }
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.visit_expr_for_types(condition, func_types)?;
                for stmt in body {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
            }
            Stmt::ForRange {
                iterator,
                start,
                end,
                inclusive: _,
                body,
                ..
            } => {
                self.visit_expr_for_types(start, func_types)?;
                self.visit_expr_for_types(end, func_types)?;

                if let Some(iterator_ty) = self.expr_types.get(&start.span().start).cloned() {
                    self.type_env.insert(iterator.name.clone(), iterator_ty);
                }

                for stmt in body {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::FieldAssignment { value, object, .. } => {
                self.visit_expr_for_types(value, func_types)?;
                // Ensure the object variable is in type_env so field access codegen can resolve it
                if !self.type_env.contains_key(&object.name) {
                    if let Some(ty) = self.expr_types.get(&object.span.start).cloned() {
                        self.type_env.insert(object.name.clone(), ty);
                    }
                }
            }
            Stmt::Expr(expr) => {
                self.visit_expr_for_types(expr, func_types)?;
            }
        }
        Ok(())
    }

    pub(crate) fn visit_expr_for_types(
        &mut self,
        expr: &Expr,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match expr {
            Expr::Cast {
                expr: inner,
                target_type,
                span,
            } => {
                self.visit_expr_for_types(inner, func_types)?;
                let inner_ty = self.expr_types.get(&inner.span().start).unwrap().clone();
                let llvm_ty = crate::types::Type::from_ast(target_type);
                self.expr_types.insert(span.start, llvm_ty);
                // Store inner type safely for codegen
                self.expr_types.insert(span.start + 1, inner_ty);
            }
            Expr::Literal(lit, span) => {
                let ty = match lit {
                    shared_types::Literal::Integer(_) => Type::I32,
                    shared_types::Literal::Float(_) => Type::F64,
                    shared_types::Literal::Boolean(_) => Type::Bool,
                    shared_types::Literal::String(_) => Type::String,
                };
                self.expr_types.insert(span.start, ty);
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                self.visit_expr_for_types(left, func_types)?;
                self.visit_expr_for_types(right, func_types)?;

                // Infer the result type from the operator and left operand type
                let left_ty = self
                    .expr_types
                    .get(&left.span().start)
                    .ok_or_else(|| {
                        CodegenError::InternalError("missing type for left operand".to_string())
                    })?
                    .clone();

                let result_ty = match op {
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => left_ty.clone(),
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::Greater
                    | BinaryOp::LessEqual
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or => Type::Bool,
                };

                self.expr_types.insert(span.start, result_ty);
                // Store left type for binary codegen
                self.expr_types.insert(span.start + 1, left_ty);
            }
            Expr::Unary { operand, span, .. } => {
                self.visit_expr_for_types(operand, func_types)?;
                let operand_ty = self
                    .expr_types
                    .get(&operand.span().start)
                    .ok_or_else(|| {
                        CodegenError::InternalError("missing type for operand".to_string())
                    })?
                    .clone();
                self.expr_types.insert(span.start, operand_ty.clone());
                self.expr_types.insert(span.start + 1, operand_ty);
            }
            Expr::Call { func, args, span } => {
                for arg in args {
                    self.visit_expr_for_types(arg, func_types)?;
                }

                match &**func {
                    Expr::Identifier(ident) => {
                        let func_type = func_types
                            .get(&ident.name)
                            .ok_or_else(|| CodegenError::UndefinedFunction(ident.name.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => &**ret,
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "called object is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty.clone());
                    }

                    // Method call: `instance.method(args)`
                    Expr::FieldAccess { object, field, .. } => {
                        self.visit_expr_for_types(object, func_types)?;
                        let struct_name = match self.expr_types.get(&object.span().start).cloned() {
                            Some(Type::Struct(n)) => n,
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "method call on non-struct type during type collection"
                                        .to_string(),
                                ))
                            }
                        };
                        let mangled = format!("{}__{}", struct_name, field.name);
                        let func_type = func_types
                            .get(&mangled)
                            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => (**ret).clone(),
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "method type is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty);
                        // Store struct name so codegen_expr can reconstruct the mangled name.
                        self.fa_struct_names.insert(span.start, struct_name);
                    }

                    // Associated function call: `TypeName::func(args)`
                    Expr::Path {
                        type_name, member, ..
                    } => {
                        let mangled = format!("{}__{}", type_name.name, member.name);
                        let func_type = func_types
                            .get(&mangled)
                            .ok_or_else(|| CodegenError::UndefinedFunction(mangled.clone()))?;
                        let ret_ty = match func_type {
                            Type::Function { ret, .. } => (**ret).clone(),
                            _ => {
                                return Err(CodegenError::InternalError(
                                    "associated function type is not a function".to_string(),
                                ))
                            }
                        };
                        self.expr_types.insert(span.start, ret_ty);
                    }

                    _ => {}
                }
            }

            Expr::Path {
                type_name,
                member,
                span,
            } => {
                let mangled = format!("{}__{}", type_name.name, member.name);
                if let Some(Type::Function { ret, .. }) = func_types.get(&mangled) {
                    self.expr_types.insert(span.start, (**ret).clone());
                }
            }
            Expr::Paren(inner, span) => {
                self.visit_expr_for_types(inner, func_types)?;
                // Parenthesized expressions have the same type as their inner expression
                let inner_ty = self.expr_types.get(&inner.span().start).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type for parenthesized expression".to_string(),
                    )
                })?;
                self.expr_types.insert(span.start, inner_ty.clone());
            }
            Expr::Identifier(ident) => {
                // Look up the type from the type environment
                let ty = self
                    .type_env
                    .get(&ident.name)
                    .ok_or_else(|| CodegenError::UndefinedVariable(ident.name.clone()))?;
                self.expr_types.insert(ident.span.start, ty.clone());
            }

            Expr::StructLiteral { name, fields, span } => {
                for field_init in fields {
                    self.visit_expr_for_types(&field_init.value, func_types)?;
                }
                self.expr_types
                    .insert(span.start, Type::Struct(name.name.clone()));
            }

            Expr::FieldAccess {
                object,
                field,
                span,
            } => {
                self.visit_expr_for_types(object, func_types)?;

                let struct_name = match self.expr_types.get(&object.span().start).cloned() {
                    Some(Type::Struct(n)) => n,
                    _ => {
                        return Err(CodegenError::InternalError(
                            "field access on non-struct type during type collection".to_string(),
                        ))
                    }
                };

                // Store the struct name keyed by the FieldAccess span so codegen_expr can
                // retrieve it without colliding with the object Identifier's expr_types entry
                // (both share the same span.start when the object is a bare identifier).
                self.fa_struct_names.insert(span.start, struct_name.clone());

                let field_ty = self
                    .struct_defs
                    .get(&struct_name)
                    .and_then(|def| {
                        def.iter()
                            .find(|(n, _)| n == &field.name)
                            .map(|(_, ty)| ty.clone())
                    })
                    .ok_or_else(|| {
                        CodegenError::InternalError(format!(
                            "unknown field '{}' on struct '{}'",
                            field.name, struct_name
                        ))
                    })?;

                self.expr_types.insert(span.start, field_ty);
            }
        }
        Ok(())
    }
}
