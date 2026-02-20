// NEURO Programming Language - LLVM Backend
// Code generation context and LLVM IR generation

use ast_types::{BinaryOp, Expr, FunctionDef, Item, Stmt, UnaryOp};
use inkwell::builder::Builder;
use inkwell::context::Context as LLVMContext;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};
use std::collections::HashMap;

use crate::errors::{CodegenError, CodegenResult};
use crate::type_mapping::TypeMapper;
use crate::types::Type;

pub(crate) struct CodegenContext<'ctx> {
    context: &'ctx LLVMContext,
    pub(crate) module: Module<'ctx>,
    builder: Builder<'ctx>,
    type_mapper: TypeMapper<'ctx>,

    /// Local variables in the current function (name -> pointer to stack allocation)
    variables: HashMap<String, PointerValue<'ctx>>,

    /// Types of local variables (needed for opaque pointers)
    variable_types: HashMap<String, BasicTypeEnum<'ctx>>,

    /// Function declarations (name -> LLVM function)
    functions: HashMap<String, FunctionValue<'ctx>>,

    /// Current function being compiled (for return type checking)
    current_function: Option<FunctionValue<'ctx>>,

    /// Type information for expressions (needed for operator codegen)
    expr_types: HashMap<usize, Type>, // Maps expression span.start -> Type

    /// Variable type information during type collection (name -> Type)
    type_env: HashMap<String, Type>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub(crate) fn new(context: &'ctx LLVMContext, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        let type_mapper = TypeMapper::new(context);

        Self {
            context,
            module,
            builder,
            type_mapper,
            variables: HashMap::new(),
            variable_types: HashMap::new(),
            functions: HashMap::new(),
            current_function: None,
            expr_types: HashMap::new(),
            type_env: HashMap::new(),
        }
    }

    /// Generate code for a literal expression
    fn codegen_literal(&self, lit: &shared_types::Literal) -> CodegenResult<BasicValueEnum<'ctx>> {
        match lit {
            shared_types::Literal::Integer(val) => {
                // Default to i32 for integer literals
                Ok(self.context.i32_type().const_int(*val as u64, true).into())
            }
            shared_types::Literal::Float(val) => {
                // Default to f64 for float literals
                Ok(self.context.f64_type().const_float(*val).into())
            }
            shared_types::Literal::Boolean(val) => Ok(self
                .context
                .bool_type()
                .const_int(*val as u64, false)
                .into()),
            shared_types::Literal::String(s) => {
                // Create a global string constant
                // LLVM will automatically null-terminate the string and place it in read-only memory
                let global_string =
                    self.builder
                        .build_global_string_ptr(s, "str")
                        .map_err(|e| {
                            CodegenError::LlvmError(format!(
                                "failed to create string constant: {}",
                                e
                            ))
                        })?;

                // Return the pointer to the string
                Ok(global_string.as_pointer_value().into())
            }
        }
    }

    /// Generate code for an identifier (variable reference)
    fn codegen_identifier(&self, name: &str) -> CodegenResult<BasicValueEnum<'ctx>> {
        let ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        let var_type = self.variable_types.get(name).ok_or_else(|| {
            CodegenError::InternalError(format!("missing type for variable {}", name))
        })?;

        self.builder
            .build_load(*var_type, *ptr, name)
            .map_err(|e| CodegenError::LlvmError(format!("failed to load variable: {}", e)))
    }

    /// Generate code for a binary expression
    fn codegen_binary(
        &self,
        left: &Expr,
        op: BinaryOp,
        right: &Expr,
        left_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let lhs = self.codegen_expr(left)?;
        let rhs = self.codegen_expr(right)?;

        match op {
            // Arithmetic operators
            BinaryOp::Add => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_add(lhs.into_float_value(), rhs.into_float_value(), "addtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_add(lhs.into_int_value(), rhs.into_int_value(), "addtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Subtract => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_sub(lhs.into_float_value(), rhs.into_float_value(), "subtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_sub(lhs.into_int_value(), rhs.into_int_value(), "subtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Multiply => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_mul(lhs.into_float_value(), rhs.into_float_value(), "multmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_mul(lhs.into_int_value(), rhs.into_int_value(), "multmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Divide => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_div(lhs.into_float_value(), rhs.into_float_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer division
                    Ok(self
                        .builder
                        .build_int_unsigned_div(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "divtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer division
                    Ok(self
                        .builder
                        .build_int_signed_div(lhs.into_int_value(), rhs.into_int_value(), "divtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Modulo => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_rem(lhs.into_float_value(), rhs.into_float_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned integer modulo
                    Ok(self
                        .builder
                        .build_int_unsigned_rem(
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "modtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed integer modulo
                    Ok(self
                        .builder
                        .build_int_signed_rem(lhs.into_int_value(), rhs.into_int_value(), "modtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Comparison operators
            BinaryOp::Equal => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OEQ,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::EQ,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "eqtmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::NotEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::NE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "netmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Less => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "lttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::Greater => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGT,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGT,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "gttmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::LessEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OLE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::ULE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed less than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SLE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "letmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            BinaryOp::GreaterEqual => {
                if TypeMapper::is_float_type(left_ty) {
                    Ok(self
                        .builder
                        .build_float_compare(
                            FloatPredicate::OGE,
                            lhs.into_float_value(),
                            rhs.into_float_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else if TypeMapper::is_unsigned_int(left_ty) {
                    // Unsigned greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::UGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    // Signed greater than or equal comparison
                    Ok(self
                        .builder
                        .build_int_compare(
                            IntPredicate::SGE,
                            lhs.into_int_value(),
                            rhs.into_int_value(),
                            "getmp",
                        )
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }

            // Logical operators (short-circuit evaluation would require basic blocks, using simple AND/OR for Phase 1)
            BinaryOp::And => Ok(self
                .builder
                .build_and(lhs.into_int_value(), rhs.into_int_value(), "andtmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
            BinaryOp::Or => Ok(self
                .builder
                .build_or(lhs.into_int_value(), rhs.into_int_value(), "ortmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for a unary expression
    fn codegen_unary(
        &self,
        op: UnaryOp,
        operand: &Expr,
        operand_ty: &Type,
    ) -> CodegenResult<BasicValueEnum<'ctx>> {
        let val = self.codegen_expr(operand)?;

        match op {
            UnaryOp::Negate => {
                if TypeMapper::is_float_type(operand_ty) {
                    Ok(self
                        .builder
                        .build_float_neg(val.into_float_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                } else {
                    Ok(self
                        .builder
                        .build_int_neg(val.into_int_value(), "negtmp")
                        .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                        .into())
                }
            }
            UnaryOp::Not => Ok(self
                .builder
                .build_not(val.into_int_value(), "nottmp")
                .map_err(|e| CodegenError::LlvmError(e.to_string()))?
                .into()),
        }
    }

    /// Generate code for a function call
    fn codegen_call(&self, func_name: &str, args: &[Expr]) -> CodegenResult<BasicValueEnum<'ctx>> {
        let function = self
            .functions
            .get(func_name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_name.to_string()))?;

        let mut arg_values = Vec::new();
        for arg in args {
            let val = self.codegen_expr(arg)?;
            arg_values.push(BasicMetadataValueEnum::from(val));
        }

        let call_result = self
            .builder
            .build_call(*function, &arg_values, "calltmp")
            .map_err(|e| CodegenError::LlvmError(format!("failed to build call: {}", e)))?;

        call_result.try_as_basic_value().left().ok_or_else(|| {
            CodegenError::InternalError(
                "function call returned void when value expected".to_string(),
            )
        })
    }

    /// Generate code for an expression
    fn codegen_expr(&self, expr: &Expr) -> CodegenResult<BasicValueEnum<'ctx>> {
        match expr {
            Expr::Literal(lit, _) => self.codegen_literal(lit),
            Expr::Identifier(ident) => self.codegen_identifier(&ident.name),
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => {
                // Look up the type of the left operand (stored during type checking pass)
                let left_ty = self.expr_types.get(&span.start).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type information for expression".to_string(),
                    )
                })?;
                self.codegen_binary(left, *op, right, left_ty)
            }
            Expr::Unary { op, operand, span } => {
                let operand_ty = self.expr_types.get(&span.start).ok_or_else(|| {
                    CodegenError::InternalError(
                        "missing type information for expression".to_string(),
                    )
                })?;
                self.codegen_unary(*op, operand, operand_ty)
            }
            Expr::Call { func, args, .. } => {
                // For Phase 1, only simple identifier function calls
                if let Expr::Identifier(ident) = &**func {
                    self.codegen_call(&ident.name, args)
                } else {
                    Err(CodegenError::UnsupportedType(
                        "complex function expressions not supported in Phase 1".to_string(),
                    ))
                }
            }
            Expr::Paren(inner, _) => self.codegen_expr(inner),
        }
    }

    /// Generate code for a variable declaration statement
    fn codegen_var_decl(&mut self, name: &str, init: Option<&Expr>) -> CodegenResult<()> {
        // Get the type of the variable (we need to infer from initializer or explicit type)
        let init_val = if let Some(expr) = init {
            Some(self.codegen_expr(expr)?)
        } else {
            None
        };

        if let Some(val) = init_val {
            let val_type = val.get_type();

            // Allocate space on the stack
            let alloca = self.builder.build_alloca(val_type, name).map_err(|e| {
                CodegenError::LlvmError(format!("failed to allocate variable: {}", e))
            })?;

            // Store the initial value
            self.builder.build_store(alloca, val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store initial value: {}", e))
            })?;

            // Record the variable and its type
            self.variables.insert(name.to_string(), alloca);
            self.variable_types.insert(name.to_string(), val_type);
        }

        Ok(())
    }

    /// Generate code for an assignment statement
    fn codegen_assignment(&self, name: &str, value: &Expr) -> CodegenResult<()> {
        // Generate code for the value expression
        let val = self.codegen_expr(value)?;

        // Lookup the variable pointer (must already exist)
        let var_ptr = self
            .variables
            .get(name)
            .ok_or_else(|| CodegenError::UndefinedVariable(name.to_string()))?;

        // Store the new value into the variable
        self.builder.build_store(*var_ptr, val).map_err(|e| {
            CodegenError::LlvmError(format!("failed to store value in assignment: {}", e))
        })?;

        Ok(())
    }

    /// Generate code for a return statement
    fn codegen_return(&self, value: Option<&Expr>) -> CodegenResult<()> {
        if let Some(expr) = value {
            let ret_val = self.codegen_expr(expr)?;
            self.builder
                .build_return(Some(&ret_val))
                .map_err(|e| CodegenError::LlvmError(format!("failed to build return: {}", e)))?;
        } else {
            self.builder.build_return(None).map_err(|e| {
                CodegenError::LlvmError(format!("failed to build void return: {}", e))
            })?;
        }
        Ok(())
    }

    /// Generate code for an if/else statement
    fn codegen_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_if_blocks: &[(Expr, Vec<Stmt>)],
        else_block: &Option<Vec<Stmt>>,
    ) -> CodegenResult<()> {
        let cond_val = self.codegen_expr(condition)?;

        let parent_fn = self.current_function.ok_or_else(|| {
            CodegenError::InternalError("if statement outside function".to_string())
        })?;

        let then_bb = self.context.append_basic_block(parent_fn, "then");
        let else_bb = self.context.append_basic_block(parent_fn, "else");
        let merge_bb = self.context.append_basic_block(parent_fn, "ifcont");

        // Build conditional branch
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), then_bb, else_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        // Generate then block
        self.builder.position_at_end(then_bb);
        for stmt in then_block {
            self.codegen_stmt(stmt)?;
        }
        // Only add branch if the block doesn't already end with a terminator
        if then_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        // Generate else-if and else blocks
        self.builder.position_at_end(else_bb);
        if !else_if_blocks.is_empty() || else_block.is_some() {
            // For simplicity in Phase 1, chain else-if blocks as nested ifs
            for (else_if_cond, else_if_stmts) in else_if_blocks {
                self.codegen_if(else_if_cond, else_if_stmts, &[], &None)?;
            }

            if let Some(else_stmts) = else_block {
                for stmt in else_stmts {
                    self.codegen_stmt(stmt)?;
                }
            }
        }
        // Only add branch if the block doesn't already end with a terminator
        if else_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        // Continue at merge block
        self.builder.position_at_end(merge_bb);

        Ok(())
    }

    /// Generate code for a while statement
    fn codegen_while(&mut self, condition: &Expr, body: &[Stmt]) -> CodegenResult<()> {
        let parent_fn = self
            .current_function
            .ok_or_else(|| CodegenError::InternalError("no current function".to_string()))?;

        let cond_bb = self.context.append_basic_block(parent_fn, "while.cond");
        let body_bb = self.context.append_basic_block(parent_fn, "while.body");
        let exit_bb = self.context.append_basic_block(parent_fn, "while.exit");

        let current_bb = self.builder.get_insert_block().ok_or_else(|| {
            CodegenError::InternalError("no insert block before while".to_string())
        })?;

        if current_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(cond_bb);
        let cond_val = self.codegen_expr(condition)?;
        self.builder
            .build_conditional_branch(cond_val.into_int_value(), body_bb, exit_bb)
            .map_err(|e| {
                CodegenError::LlvmError(format!("failed to build conditional branch: {}", e))
            })?;

        self.builder.position_at_end(body_bb);
        for stmt in body {
            self.codegen_stmt(stmt)?;
        }

        if body_bb.get_terminator().is_none() {
            self.builder
                .build_unconditional_branch(cond_bb)
                .map_err(|e| CodegenError::LlvmError(format!("failed to build branch: {}", e)))?;
        }

        self.builder.position_at_end(exit_bb);
        Ok(())
    }

    /// Generate code for a statement
    fn codegen_stmt(&mut self, stmt: &Stmt) -> CodegenResult<()> {
        match stmt {
            Stmt::VarDecl { name, init, .. } => self.codegen_var_decl(&name.name, init.as_ref()),
            Stmt::Assignment { target, value, .. } => self.codegen_assignment(&target.name, value),
            Stmt::Return { value, .. } => self.codegen_return(value.as_ref()),
            Stmt::If {
                condition,
                then_block,
                else_if_blocks,
                else_block,
                ..
            } => self.codegen_if(condition, then_block, else_if_blocks, else_block),
            Stmt::While {
                condition, body, ..
            } => self.codegen_while(condition, body),
            Stmt::Expr(expr) => {
                self.codegen_expr(expr)?;
                Ok(())
            }
        }
    }

    /// Generate code for a function definition
    pub(crate) fn codegen_function(
        &mut self,
        func_def: &FunctionDef,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // Get function type information
        let func_type_info = func_types
            .get(&func_def.name.name)
            .ok_or_else(|| CodegenError::UndefinedFunction(func_def.name.name.clone()))?;

        let (param_types, return_type) = match func_type_info {
            Type::Function { params, ret } => (params, &**ret),
            _ => {
                return Err(CodegenError::InternalError(
                    "function type information is not a function type".to_string(),
                ))
            }
        };

        // Map parameter types to LLVM types
        let mut llvm_param_types = Vec::new();
        for param_ty in param_types {
            let llvm_ty = self.type_mapper.map_type(param_ty)?;
            llvm_param_types.push(BasicMetadataTypeEnum::from(llvm_ty));
        }

        // Map return type to LLVM type
        let llvm_ret_type = if matches!(return_type, Type::Void) {
            self.context.void_type().fn_type(&llvm_param_types, false)
        } else {
            let ret_basic_type = self.type_mapper.map_type(return_type)?;
            ret_basic_type.fn_type(&llvm_param_types, false)
        };

        // Create the function
        let function = self
            .module
            .add_function(&func_def.name.name, llvm_ret_type, None);

        // Record the function for later calls
        self.functions.insert(func_def.name.name.clone(), function);

        // Create entry basic block
        let entry = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry);

        // Set current function for return statements
        self.current_function = Some(function);

        // Clear variables for new function scope
        self.variables.clear();
        self.variable_types.clear();

        // Allocate and store parameters
        for (i, param) in func_def.params.iter().enumerate() {
            let param_val = function
                .get_nth_param(i as u32)
                .ok_or_else(|| CodegenError::InternalError(format!("missing parameter {}", i)))?;

            let param_type = param_val.get_type();

            let alloca = self
                .builder
                .build_alloca(param_type, &param.name.name)
                .map_err(|e| {
                    CodegenError::LlvmError(format!("failed to allocate parameter: {}", e))
                })?;

            self.builder.build_store(alloca, param_val).map_err(|e| {
                CodegenError::LlvmError(format!("failed to store parameter: {}", e))
            })?;

            self.variables.insert(param.name.name.clone(), alloca);
            self.variable_types
                .insert(param.name.name.clone(), param_type);
        }

        // Generate function body
        // Handle expression-based returns: if the last statement is an expression
        // and the function has a non-void return type, treat it as an implicit return
        let has_implicit_return = !matches!(return_type, Type::Void)
            && !func_def.body.is_empty()
            && matches!(func_def.body.last(), Some(Stmt::Expr(_)));

        if has_implicit_return {
            // Generate all statements except the last one
            for stmt in &func_def.body[..func_def.body.len() - 1] {
                self.codegen_stmt(stmt)?;
            }

            // Generate implicit return from the last expression
            if let Some(Stmt::Expr(expr)) = func_def.body.last() {
                let ret_val = self.codegen_expr(expr)?;
                self.builder.build_return(Some(&ret_val)).map_err(|e| {
                    CodegenError::LlvmError(format!("failed to build implicit return: {}", e))
                })?;
            }
        } else {
            // Generate all statements normally
            for stmt in &func_def.body {
                self.codegen_stmt(stmt)?;
            }

            // Ensure function has a return if it's non-void
            if !matches!(return_type, Type::Void) {
                let current_bb = self.builder.get_insert_block().ok_or_else(|| {
                    CodegenError::InternalError("no insert block after function body".to_string())
                })?;

                if current_bb.get_terminator().is_none() {
                    return Err(CodegenError::MissingReturn);
                }
            } else if let Some(current_bb) = self.builder.get_insert_block() {
                // Add void return if missing
                if current_bb.get_terminator().is_none() {
                    self.builder.build_return(None).map_err(|e| {
                        CodegenError::LlvmError(format!("failed to build void return: {}", e))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Store type information for expressions (needed for codegen)
    pub(crate) fn store_expr_types(
        &mut self,
        items: &[Item],
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        // We need to run type inference to get expression types
        // For Phase 1, we'll use a simple visitor pattern
        for item in items {
            let Item::Function(func_def) = item;
            self.visit_function_for_types(func_def, func_types)?;
        }
        Ok(())
    }

    fn visit_function_for_types(
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

    fn visit_stmt_for_types(
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
            Stmt::Expr(expr) => {
                self.visit_expr_for_types(expr, func_types)?;
            }
        }
        Ok(())
    }

    fn visit_expr_for_types(
        &mut self,
        expr: &Expr,
        func_types: &HashMap<String, Type>,
    ) -> CodegenResult<()> {
        match expr {
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
                // Get the return type of the function call
                if let Expr::Identifier(ident) = &**func {
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
        }
        Ok(())
    }
}
