use ast_types::*;
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::types::Type;

use super::context::{resolve_builtin_method, CodegenContext};

impl<'ctx> CodegenContext<'ctx> {
    /// Store type information for expressions (needed for codegen)
    pub(crate) fn store_expr_types(
        &mut self,
        items: &[Item],
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // Pre-pass: collect module-level const types so identifiers referencing them
        // resolve correctly inside function bodies (visit_function_for_types clears
        // type_env before each function; global_const_types is re-seeded each time).
        for item in items {
            if let Item::Const(def) = item {
                let ty = crate::types::Type::from_ast(&def.ty);
                self.global_const_types.insert(def.name.name.clone(), ty);
            }
        }

        for item in items {
            match item {
                Item::Function(func_def) => {
                    self.visit_function_for_types(func_def, func_types)?;
                }
                Item::Impl(impl_def) => {
                    self.visit_impl_for_types(impl_def, func_types)?;
                }
                Item::Const(_) | Item::Struct(_) => {}
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
        for (name, ty) in &self.global_const_types.clone() {
            self.type_env.insert(name.clone(), ty.clone());
        }

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
        self.record_tail_if_type(&method.body);
        Ok(())
    }

    pub(crate) fn visit_function_for_types(
        &mut self,
        func_def: &FunctionDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        self.type_env.clear();
        // Re-seed with module-level consts so references to them resolve inside the body.
        for (name, ty) in &self.global_const_types.clone() {
            self.type_env.insert(name.clone(), ty.clone());
        }

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
        self.record_tail_if_type(&func_def.body);
        Ok(())
    }

    /// A statement-position `if` parses to `Stmt::If`, so the type pass never records
    /// a result type at its span the way it does for an `Expr::If`. When such an `if`
    /// is the body's tail (the implicit return), `codegen_if_expr` needs that type to
    /// allocate the result slot — record it here, mirroring the `Expr::If` arm.
    fn record_tail_if_type(&mut self, body: &[Stmt]) {
        if let Some(Stmt::If {
            then_block,
            else_block,
            span,
            ..
        }) = body.last()
        {
            if else_block.is_some() {
                let result_ty = match then_block.last() {
                    Some(Stmt::Expr(e)) => self
                        .expr_types
                        .get(&e.span().start)
                        .cloned()
                        .unwrap_or(Type::Void),
                    _ => Type::Void,
                };
                self.expr_types.insert(span.start, result_ty);
            }
        }
    }

    pub(crate) fn visit_stmt_for_types(
        &mut self,
        stmt: &Stmt,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, ty, init, .. } => {
                if let Some(expr) = init {
                    self.visit_expr_for_types(expr, func_types)?;
                    // Prefer the declared type annotation for the variable's type in
                    // downstream expression inference.  Without this, `val x: i64 = 42`
                    // would record i32 (from the literal's default) so later uses of `x`
                    // would load an i32 alloca instead of an i64 one.
                    let var_ty = if let Some(declared) = ty {
                        crate::types::Type::from_ast(declared)
                    } else {
                        self.expr_types
                            .get(&expr.span().start)
                            .cloned()
                            .ok_or_else(|| {
                                CodegenError::InternalError(
                                    "missing type for variable initializer".to_string(),
                                )
                            })?
                    };
                    self.type_env.insert(name.name.clone(), var_ty);
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
            Stmt::Const {
                name, ty, value, ..
            } => {
                self.visit_expr_for_types(value, func_types)?;
                let const_ty = crate::types::Type::from_ast(ty);
                self.type_env.insert(name.name.clone(), const_ty);
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
                let inner_ty = self
                    .expr_types
                    .get(&inner.span().start)
                    .ok_or_else(|| {
                        CodegenError::InternalError(
                            "cast inner expression type not found".to_string(),
                        )
                    })?
                    .clone();
                let llvm_ty = crate::types::Type::from_ast(target_type);
                self.expr_types.insert(span.start, llvm_ty);
                // Store inner type safely for codegen
                self.expr_types.insert(span.start + 1, inner_ty);
            }
            Expr::Literal(lit, span) => {
                let ty = match lit {
                    shared_types::Literal::Integer(_, suffix_opt) => {
                        use shared_types::IntSuffix;
                        match suffix_opt {
                            None | Some(IntSuffix::I32) | Some(IntSuffix::U32) => Type::I32,
                            Some(IntSuffix::I8) | Some(IntSuffix::U8) => Type::I8,
                            Some(IntSuffix::I16) | Some(IntSuffix::U16) => Type::I16,
                            Some(IntSuffix::I64) | Some(IntSuffix::U64) => Type::I64,
                        }
                    }
                    shared_types::Literal::Float(_, suffix_opt) => {
                        use shared_types::FloatSuffix;
                        match suffix_opt {
                            Some(FloatSuffix::F32) => Type::F32,
                            None | Some(FloatSuffix::F64) => Type::F64,
                        }
                    }
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
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl => {
                        left_ty.clone()
                    }
                    BinaryOp::NullCoalesce => {
                        return Err(CodegenError::InternalError(
                            "operator '??' reached the codegen type pass; semantic analysis must reject it (Phase 2 feature)"
                                .into(),
                        ));
                    }
                };

                self.expr_types.insert(span.start, result_ty);
                // Store the left-operand type for binary codegen in a dedicated, full-span-keyed
                // map. A `span.start + 1` slot in `expr_types` would collide: this node and its
                // leftmost descendant share `span.start`, so the parent (e.g. `&&`, left type
                // Bool) would clobber the child comparison's left type (e.g. i32).
                self.binary_left_types
                    .insert((span.start, span.end), left_ty);
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
                        // Panic-family builtins (§1.2) are not in `func_types`; they yield unit
                        // and are lowered specially. A user function of the same name shadows
                        // the builtin, matching `codegen_expr` and the semantic resolver.
                        if Self::is_panic_builtin(&ident.name)
                            && !func_types.contains_key(&ident.name)
                        {
                            self.expr_types.insert(span.start, Type::Void);
                        } else {
                            let func_type = func_types.get(&ident.name).ok_or_else(|| {
                                CodegenError::UndefinedFunction(ident.name.clone())
                            })?;
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
                    }

                    // Method call: `instance.method(args)`
                    Expr::FieldAccess { object, field, .. } => {
                        self.visit_expr_for_types(object, func_types)?;
                        let recv_ty = self.expr_types.get(&object.span().start).cloned();
                        match recv_ty {
                            Some(Type::Struct(struct_name)) => {
                                let mangled = format!("{}__{}", struct_name, field.name);
                                let func_type = func_types.get(&mangled).ok_or_else(|| {
                                    CodegenError::UndefinedFunction(mangled.clone())
                                })?;
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
                            Some(recv) => match resolve_builtin_method(&recv, &field.name) {
                                Some((kind, ret_ty)) => {
                                    self.expr_types.insert(span.start, ret_ty);
                                    self.builtin_methods
                                        .insert((span.start, span.end), (kind, recv));
                                }
                                None => {
                                    return Err(CodegenError::InternalError(
                                        "unrecognised builtin method during type collection"
                                            .to_string(),
                                    ))
                                }
                            },
                            None => {
                                return Err(CodegenError::InternalError(
                                    "missing receiver type for method call during type collection"
                                        .to_string(),
                                ))
                            }
                        }
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

            Expr::StructLiteral {
                name,
                fields,
                base,
                span,
            } => {
                for field_init in fields {
                    self.visit_expr_for_types(&field_init.value, func_types)?;
                }
                if let Some(base) = base {
                    self.visit_expr_for_types(base, func_types)?;
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

            Expr::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                span,
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
                let result_ty = if else_block.is_none() {
                    Type::Void
                } else {
                    match then_block.last() {
                        Some(Stmt::Expr(e)) => self
                            .expr_types
                            .get(&e.span().start)
                            .cloned()
                            .unwrap_or(Type::Void),
                        _ => Type::Void,
                    }
                };
                self.expr_types.insert(span.start, result_ty);
            }

            Expr::Block { stmts, span } | Expr::Unsafe { stmts, span } => {
                for stmt in stmts {
                    self.visit_stmt_for_types(stmt, func_types)?;
                }
                let result_ty = match stmts.last() {
                    Some(Stmt::Expr(e)) => self
                        .expr_types
                        .get(&e.span().start)
                        .cloned()
                        .unwrap_or(Type::Void),
                    _ => Type::Void,
                };
                self.expr_types.insert(span.start, result_ty);
            }
        }
        Ok(())
    }
}
