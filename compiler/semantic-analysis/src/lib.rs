// NEURO Programming Language - Semantic Analysis
// Feature slice for type checking and semantic validation

use shared_types::Span;
use std::collections::HashMap;
use syntax_parsing::{BinaryOp, Expr, FunctionDef, Item, Stmt, UnaryOp};
use thiserror::Error;

/// Type representation for semantic analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I32,
    I64,
    F32,
    F64,
    Bool,
    Void,
    Function { params: Vec<Type>, ret: Box<Type> },
    Unknown,
}

impl Type {
    /// Check if this type is compatible with another type
    pub(crate) fn is_compatible_with(&self, other: &Type) -> bool {
        match (self, other) {
            // Exact matches
            (Type::I32, Type::I32)
            | (Type::I64, Type::I64)
            | (Type::F32, Type::F32)
            | (Type::F64, Type::F64)
            | (Type::Bool, Type::Bool)
            | (Type::Void, Type::Void) => true,

            // Function types must match exactly
            (
                Type::Function {
                    params: p1,
                    ret: r1,
                },
                Type::Function {
                    params: p2,
                    ret: r2,
                },
            ) => {
                p1.len() == p2.len()
                    && p1
                        .iter()
                        .zip(p2.iter())
                        .all(|(a, b)| a.is_compatible_with(b))
                    && r1.is_compatible_with(r2)
            }

            // Unknown type for error recovery
            (Type::Unknown, _) | (_, Type::Unknown) => true,

            _ => false,
        }
    }

    /// Check if this is a numeric type
    pub(crate) fn is_numeric(&self) -> bool {
        matches!(self, Type::I32 | Type::I64 | Type::F32 | Type::F64)
    }

    /// Check if this is an integer type
    #[allow(dead_code)] // Reserved for future use in Phase 2+
    pub(crate) fn is_integer(&self) -> bool {
        matches!(self, Type::I32 | Type::I64)
    }

    /// Check if this is a boolean type
    pub(crate) fn is_bool(&self) -> bool {
        matches!(self, Type::Bool)
    }
}

/// Type checking errors with source location information
#[derive(Debug, Error, Clone, PartialEq)]
pub enum TypeError {
    #[error("type mismatch at {span:?}: expected {expected:?}, found {found:?}")]
    Mismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("undefined variable '{name}' at {span:?}")]
    UndefinedVariable { name: String, span: Span },

    #[error("undefined function '{name}' at {span:?}")]
    UndefinedFunction { name: String, span: Span },

    #[error("variable '{name}' already defined in this scope at {span:?}")]
    VariableAlreadyDefined { name: String, span: Span },

    #[error("function '{name}' already defined at {span:?}")]
    FunctionAlreadyDefined { name: String, span: Span },

    #[error("incorrect number of arguments at {span:?}: expected {expected}, found {found}")]
    ArgumentCountMismatch {
        expected: usize,
        found: usize,
        span: Span,
    },

    #[error("cannot apply operator {op} to type {ty:?} at {span:?}")]
    InvalidOperator { op: String, ty: Type, span: Span },

    #[error("cannot apply binary operator {op} to types {left:?} and {right:?} at {span:?}")]
    InvalidBinaryOperator {
        op: String,
        left: Type,
        right: Type,
        span: Span,
    },

    #[error("return type mismatch at {span:?}: expected {expected:?}, found {found:?}")]
    ReturnTypeMismatch {
        expected: Type,
        found: Type,
        span: Span,
    },

    #[error("missing return statement in function returning {expected:?}")]
    MissingReturn { expected: Type },

    #[error("unknown type name '{name}' at {span:?}")]
    UnknownTypeName { name: String, span: Span },

    #[error("cannot call non-function type {ty:?} at {span:?}")]
    NotCallable { ty: Type, span: Span },

    #[error("variable '{name}' used without initialization at {span:?}")]
    UninitializedVariable { name: String, span: Span },
}

/// Symbol table with lexical scoping support
#[derive(Debug)]
pub(crate) struct SymbolTable {
    /// Stack of scopes (innermost scope is last)
    scopes: Vec<HashMap<String, Type>>,
}

impl SymbolTable {
    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Enter a new scope (e.g., function body, block)
    pub(crate) fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Exit the current scope
    pub(crate) fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a variable in the current scope
    pub(crate) fn define(&mut self, name: String, ty: Type) -> Result<(), String> {
        if let Some(current_scope) = self.scopes.last_mut() {
            if current_scope.contains_key(&name) {
                return Err(name);
            }
            current_scope.insert(name, ty);
            Ok(())
        } else {
            Err(name)
        }
    }

    /// Look up a variable in all scopes (innermost to outermost)
    pub(crate) fn lookup(&self, name: &str) -> Option<&Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

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
                "i32" => Ok(Type::I32),
                "i64" => Ok(Type::I64),
                "f32" => Ok(Type::F32),
                "f64" => Ok(Type::F64),
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
            Expr::Literal(lit, _span) => {
                use shared_types::Literal;
                match lit {
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
                }
            }

            Expr::Identifier(ident) => {
                if let Some(ty) = self.symbols.lookup(&ident.name) {
                    Ok(ty.clone())
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
                mutable: _,
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
                if let Err(duplicate_name) = self.symbols.define(name.name.clone(), final_ty) {
                    self.record_error(TypeError::VariableAlreadyDefined {
                        name: duplicate_name,
                        span: name.span,
                    });
                    return Err(());
                }

                Ok(())
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

        // Define parameters in function scope
        for (param, param_ty) in func.params.iter().zip(param_types.iter()) {
            if let Err(duplicate_name) = self
                .symbols
                .define(param.name.name.clone(), param_ty.clone())
            {
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

/// Type check a NEURO program.
///
/// This function performs semantic analysis on a parsed NEURO program, validating:
/// - Type correctness of expressions and statements
/// - Variable and function declarations
/// - Function signatures and call sites
/// - Control flow (if/else) conditions
/// - Return type matching
///
/// # Phase 1 Support
///
/// Currently supports:
/// - Primitive types: `i32`, `i64`, `f32`, `f64`, `bool`
/// - Function definitions with parameters and return types
/// - Variable declarations (`val` and `mut`)
/// - Binary operators (arithmetic, comparison, logical)
/// - Unary operators (negation, logical not)
/// - Function calls
/// - If/else statements
/// - Return statements
/// - Lexical scoping
///
/// # Arguments
///
/// * `items` - A slice of AST items (functions) from the parser
///
/// # Returns
///
/// * `Ok(())` - Program is type-correct
/// * `Err(Vec<TypeError>)` - One or more type errors were found
///
/// # Examples
///
/// ```ignore
/// use syntax_parsing::parse;
/// use semantic_analysis::type_check;
///
/// let source = r#"
///     func add(a: i32, b: i32) -> i32 {
///         return a + b
///     }
/// "#;
///
/// let ast = parse(source).unwrap();
/// match type_check(&ast) {
///     Ok(()) => println!("Program is type-correct"),
///     Err(errors) => {
///         for error in errors {
///             eprintln!("Type error: {}", error);
///         }
///     }
/// }
/// ```
///
/// # Error Handling
///
/// This function collects multiple errors in a single pass (fail-slow approach)
/// to provide comprehensive feedback to the user. All errors include source
/// location information (spans) for precise error reporting.
pub fn type_check(items: &[Item]) -> Result<(), Vec<TypeError>> {
    let mut checker = TypeChecker::new();
    if checker.check_program(items).is_err() {
        Err(checker.into_errors())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_compatibility() {
        assert!(Type::I32.is_compatible_with(&Type::I32));
        assert!(Type::Bool.is_compatible_with(&Type::Bool));
        assert!(!Type::I32.is_compatible_with(&Type::Bool));
        assert!(!Type::F64.is_compatible_with(&Type::I32));
    }

    #[test]
    fn type_predicates() {
        assert!(Type::I32.is_numeric());
        assert!(Type::F64.is_numeric());
        assert!(!Type::Bool.is_numeric());

        assert!(Type::I32.is_integer());
        assert!(!Type::F64.is_integer());

        assert!(Type::Bool.is_bool());
        assert!(!Type::I32.is_bool());
    }

    #[test]
    fn symbol_table_scoping() {
        let mut table = SymbolTable::new();

        // Define in global scope
        assert!(table.define("x".to_string(), Type::I32).is_ok());
        assert_eq!(table.lookup("x"), Some(&Type::I32));

        // Define in nested scope
        table.push_scope();
        assert!(table.define("y".to_string(), Type::Bool).is_ok());
        assert_eq!(table.lookup("y"), Some(&Type::Bool));
        assert_eq!(table.lookup("x"), Some(&Type::I32)); // Can still see outer scope

        // Shadow variable
        assert!(table.define("x".to_string(), Type::F64).is_ok());
        assert_eq!(table.lookup("x"), Some(&Type::F64)); // Sees inner definition

        // Pop scope
        table.pop_scope();
        assert_eq!(table.lookup("x"), Some(&Type::I32)); // Back to outer definition
        assert_eq!(table.lookup("y"), None); // Inner variable gone
    }

    #[test]
    fn symbol_table_duplicate_definition() {
        let mut table = SymbolTable::new();
        assert!(table.define("x".to_string(), Type::I32).is_ok());
        assert!(table.define("x".to_string(), Type::Bool).is_err());
    }

    #[test]
    fn function_type_compatibility() {
        let func1 = Type::Function {
            params: vec![Type::I32, Type::Bool],
            ret: Box::new(Type::I32),
        };

        let func2 = Type::Function {
            params: vec![Type::I32, Type::Bool],
            ret: Box::new(Type::I32),
        };

        let func3 = Type::Function {
            params: vec![Type::I32],
            ret: Box::new(Type::I32),
        };

        assert!(func1.is_compatible_with(&func2));
        assert!(!func1.is_compatible_with(&func3));
    }

    // Integration tests with actual programs
    #[test]
    fn type_check_simple_function() {
        let source = r#"func add(a: i32, b: i32) -> i32 {
            return a + b
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(
            result.is_ok(),
            "Expected successful type check, got: {:?}",
            result
        );
    }

    #[test]
    fn type_check_function_with_variable() {
        let source = r#"func calculate(x: i32) -> i32 {
            val result: i32 = x * 2
            return result
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    #[test]
    fn type_check_function_call() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                val result: i32 = add(5, 3)
                return result
            }
        "#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    #[test]
    fn type_check_if_statement() {
        let source = r#"func test(x: i32) -> i32 {
            if x > 0 {
                return 1
            } else {
                return -1
            }
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    #[test]
    fn type_check_boolean_operators() {
        let source = r#"func test(a: bool, b: bool) -> bool {
            return a && b || !a
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    #[test]
    fn type_check_nested_scopes() {
        let source = r#"func test() -> i32 {
            val x: i32 = 1
            if true {
                val y: i32 = 2
                return x + y
            }
            return x
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    #[test]
    fn type_check_variable_shadowing() {
        let source = r#"func test() -> i32 {
            val x: i32 = 1
            if true {
                val x: i32 = 2
                return x
            }
            return x
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_ok());
    }

    // Error cases
    #[test]
    fn error_undefined_variable() {
        let source = r#"func test() -> i32 {
            return undefined_var
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], TypeError::UndefinedVariable { .. }));
    }

    #[test]
    fn error_type_mismatch() {
        let source = r#"func test() -> i32 {
            val x: i32 = true
            return x
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })));
    }

    #[test]
    fn error_wrong_operator_type() {
        let source = r#"func test() -> i32 {
            return true + false
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::InvalidBinaryOperator { .. })));
    }

    #[test]
    fn error_return_type_mismatch() {
        let source = r#"func test() -> i32 {
            return true
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })));
    }

    #[test]
    fn error_argument_count_mismatch() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                return add(5)
            }
        "#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::ArgumentCountMismatch { .. })));
    }

    #[test]
    fn error_argument_type_mismatch() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                return add(5, true)
            }
        "#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })));
    }

    #[test]
    fn error_undefined_function() {
        let source = r#"func main() -> i32 {
            return undefined_func()
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::UndefinedFunction { .. })));
    }

    #[test]
    fn error_if_condition_not_bool() {
        let source = r#"func test() -> i32 {
            if 42 {
                return 1
            }
            return 0
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::Mismatch { .. })));
    }

    #[test]
    fn error_duplicate_variable() {
        let source = r#"func test() -> i32 {
            val x: i32 = 1
            val x: i32 = 2
            return x
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::VariableAlreadyDefined { .. })));
    }

    #[test]
    fn error_duplicate_function() {
        let source = r#"
            func test() -> i32 {
                return 1
            }

            func test() -> i32 {
                return 2
            }
        "#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::FunctionAlreadyDefined { .. })));
    }

    #[test]
    fn error_unknown_type_name() {
        let source = r#"func test(x: unknown_type) -> i32 {
            return 0
        }"#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, TypeError::UnknownTypeName { .. })));
    }

    // Milestone test: The example from roadmap.md
    #[test]
    fn type_check_milestone_program() {
        let source = r#"
            func add(a: i32, b: i32) -> i32 {
                return a + b
            }

            func main() -> i32 {
                val result: i32 = add(5, 3)
                return result
            }
        "#;
        let items = syntax_parsing::parse(source).unwrap();
        let result = type_check(&items);
        assert!(
            result.is_ok(),
            "Milestone program should type check successfully, got: {:?}",
            result
        );
    }
}
