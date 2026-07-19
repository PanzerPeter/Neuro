// Type-alias declarations (§3.14): `type Name = TargetType`.
//
// A `type` alias is *transparent* — the alias and its target are interchangeable
// and no new nominal type is introduced. We therefore resolve aliases entirely at
// parse time by substituting every aliased type annotation with its target type,
// exactly as compound assignment desugars before reaching later stages. The result
// is that semantic analysis and codegen never observe an alias: an unknown target
// name is reported by the existing semantic `UnknownTypeName` check against the
// real type, with the diagnostic pointing at the alias *use* site.

use std::collections::HashMap;

use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{Expr, Item, Stmt, Type};
use crate::errors::{ParseError, ParseResult};

use super::Parser;

/// A parsed `type Name = Target` declaration awaiting expansion.
pub(crate) struct TypeAliasDecl {
    pub(crate) name: Identifier,
    pub(crate) target: Type,
}

/// Built-in type names an alias may not shadow. Shadowing one would silently
/// reinterpret a primitive everywhere it appears, which is a footgun rather than
/// a useful abstraction, so it is rejected up front.
const BUILTIN_TYPE_NAMES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f16", "bf16", "f32", "f64", "bool",
    "string", "char", "void",
];

impl Parser {
    /// Parse a single `type Name = TargetType` declaration. Assumes the current
    /// token is `type`.
    pub(crate) fn parse_type_alias(&mut self) -> ParseResult<TypeAliasDecl> {
        self.consume(TokenKind::Type, "'type'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "type alias name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "type alias name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let target = self.parse_type()?;

        Ok(TypeAliasDecl { name, target })
    }
}

/// Resolve and substitute all type aliases across the program.
///
/// Validates for duplicates, built-in shadowing, and cycles, then rewrites every
/// type annotation in `items` whose name refers to an alias with the alias's
/// fully-resolved target type. The use-site span is preserved so downstream
/// diagnostics point at the reference rather than the declaration.
pub(crate) fn expand_type_aliases(
    items: &mut [Item],
    decls: Vec<TypeAliasDecl>,
) -> ParseResult<()> {
    if decls.is_empty() {
        return Ok(());
    }

    let mut direct: HashMap<String, Type> = HashMap::new();
    let mut spans: HashMap<String, shared_types::Span> = HashMap::new();
    for decl in &decls {
        if BUILTIN_TYPE_NAMES.contains(&decl.name.name.as_str()) {
            return Err(ParseError::TypeAliasShadowsBuiltin {
                name: decl.name.name.clone(),
                span: decl.name.span,
            });
        }
        if direct.contains_key(&decl.name.name) {
            return Err(ParseError::DuplicateTypeAlias {
                name: decl.name.name.clone(),
                span: decl.name.span,
            });
        }
        direct.insert(decl.name.name.clone(), decl.target.clone());
        spans.insert(decl.name.name.clone(), decl.name.span);
    }

    let mut resolved: HashMap<String, Type> = HashMap::new();
    for (name, span) in &spans {
        let ultimate = resolve_alias(name, *span, &direct)?;
        resolved.insert(name.clone(), ultimate);
    }

    for item in items.iter_mut() {
        rewrite_item(item, &resolved);
    }
    Ok(())
}

/// Follow an alias chain to its ultimate non-alias target. A name that revisits
/// itself is a cycle, reported against the chain's starting alias.
fn resolve_alias(
    start: &str,
    start_span: shared_types::Span,
    direct: &HashMap<String, Type>,
) -> ParseResult<Type> {
    let mut current = start.to_string();
    let mut visited: Vec<String> = Vec::new();
    loop {
        if visited.iter().any(|v| v == &current) {
            return Err(ParseError::CyclicTypeAlias {
                name: start.to_string(),
                span: start_span,
            });
        }
        visited.push(current.clone());

        match direct.get(&current) {
            Some(Type::Named(ident)) if direct.contains_key(&ident.name) => {
                current = ident.name.clone();
            }
            Some(other) => return Ok(other.clone()),
            // `current` is always an alias key on entry and is only reassigned to
            // another alias key, so this arm is unreachable; resolve to the name
            // itself as a terminal type rather than panicking.
            None => {
                return Ok(Type::Named(Identifier {
                    name: current,
                    span: start_span,
                }))
            }
        }
    }
}

fn rewrite_type(ty: &mut Type, resolved: &HashMap<String, Type>) {
    match ty {
        Type::Named(ident) => {
            if let Some(target) = resolved.get(&ident.name) {
                let use_span = ident.span;
                *ty = target.clone();
                // Keep the diagnostic anchored at the reference, not the alias decl.
                if let Type::Named(new_ident) = ty {
                    new_ident.span = use_span;
                }
            }
        }
        Type::Reference { inner, .. } => rewrite_type(inner, resolved),
        Type::Array { element, .. } => rewrite_type(element, resolved),
        Type::Tuple { elements, .. } => {
            for element in elements {
                rewrite_type(element, resolved);
            }
        }
        Type::Generic { args, .. } => {
            for arg in args {
                if let crate::ast::GenericArg::Type(inner) = arg {
                    rewrite_type(inner, resolved);
                }
            }
        }
        Type::Tensor { element_type, .. } => rewrite_type(element_type, resolved),
        // `impl Trait` / `dyn Trait` (§3.17) name a trait, not a type alias, so a type
        // alias never substitutes into them.
        Type::ImplTrait { .. } | Type::DynTrait { .. } => {}
    }
}

fn rewrite_item(item: &mut Item, resolved: &HashMap<String, Type>) {
    match item {
        Item::Function(func) => {
            for param in &mut func.params {
                rewrite_type(&mut param.ty, resolved);
            }
            if let Some(ret) = &mut func.return_type {
                rewrite_type(ret, resolved);
            }
            rewrite_block(&mut func.body, resolved);
        }
        Item::Struct(def) => {
            for field in &mut def.fields {
                rewrite_type(&mut field.ty, resolved);
            }
        }
        Item::Enum(def) => {
            for variant in &mut def.variants {
                match &mut variant.payload {
                    crate::ast::VariantPayload::Unit => {}
                    crate::ast::VariantPayload::Tuple(tys) => {
                        for ty in tys.iter_mut() {
                            rewrite_type(ty, resolved);
                        }
                    }
                    crate::ast::VariantPayload::Struct(fields) => {
                        for field in fields.iter_mut() {
                            rewrite_type(&mut field.ty, resolved);
                        }
                    }
                }
            }
        }
        Item::Impl(def) => {
            for method in &mut def.methods {
                for param in &mut method.params {
                    rewrite_type(&mut param.ty, resolved);
                }
                if let Some(ret) = &mut method.return_type {
                    rewrite_type(ret, resolved);
                }
                rewrite_block(&mut method.body, resolved);
            }
        }
        Item::Const(def) => {
            rewrite_type(&mut def.ty, resolved);
            rewrite_expr(&mut def.value, resolved);
        }
        // A trait's method signatures may reference aliased types (§3.9); default
        // bodies are expanded when the defaults are injected into impls, which run
        // after this pass — so only the signatures are rewritten here.
        Item::Trait(def) => {
            for method in &mut def.methods {
                for param in &mut method.params {
                    rewrite_type(&mut param.ty, resolved);
                }
                if let Some(ret) = &mut method.return_type {
                    rewrite_type(ret, resolved);
                }
            }
        }
        // A newtype's inner may itself be written via a `type` alias, so expand it.
        Item::Newtype(def) => rewrite_type(&mut def.inner, resolved),
    }
}

fn rewrite_block(stmts: &mut [Stmt], resolved: &HashMap<String, Type>) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(stmt, resolved);
    }
}

fn rewrite_stmt(stmt: &mut Stmt, resolved: &HashMap<String, Type>) {
    match stmt {
        Stmt::VarDecl { ty, init, .. } => {
            if let Some(ty) = ty {
                rewrite_type(ty, resolved);
            }
            if let Some(init) = init {
                rewrite_expr(init, resolved);
            }
        }
        Stmt::Assignment { value, .. } => rewrite_expr(value, resolved),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                rewrite_expr(value, resolved);
            }
        }
        Stmt::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            rewrite_expr(condition, resolved);
            rewrite_block(then_block, resolved);
            for (cond, block) in else_if_blocks.iter_mut() {
                rewrite_expr(cond, resolved);
                rewrite_block(block, resolved);
            }
            if let Some(block) = else_block {
                rewrite_block(block, resolved);
            }
        }
        Stmt::While {
            condition, body, ..
        } => {
            rewrite_expr(condition, resolved);
            rewrite_block(body, resolved);
        }
        Stmt::Loop { body, .. } => {
            rewrite_block(body, resolved);
        }
        Stmt::ForRange {
            start, end, body, ..
        } => {
            rewrite_expr(start, resolved);
            rewrite_expr(end, resolved);
            rewrite_block(body, resolved);
        }
        Stmt::ForEach { iterable, body, .. } => {
            rewrite_expr(iterable, resolved);
            rewrite_block(body, resolved);
        }
        Stmt::IndexAssignment { index, value, .. } => {
            rewrite_expr(index, resolved);
            rewrite_expr(value, resolved);
        }
        Stmt::FieldAssignment { value, .. } => rewrite_expr(value, resolved),
        Stmt::DerefAssignment { pointer, value, .. } => {
            rewrite_expr(pointer, resolved);
            rewrite_expr(value, resolved);
        }
        Stmt::Const { ty, value, .. } => {
            rewrite_type(ty, resolved);
            rewrite_expr(value, resolved);
        }
        Stmt::Expr(expr) => rewrite_expr(expr, resolved),
        Stmt::Break { value, .. } => {
            if let Some(value) = value {
                rewrite_expr(value, resolved);
            }
        }
        Stmt::Continue { .. } => {}
    }
}

fn rewrite_expr(expr: &mut Expr, resolved: &HashMap<String, Type>) {
    match expr {
        Expr::Binary { left, right, .. } => {
            rewrite_expr(left, resolved);
            rewrite_expr(right, resolved);
        }
        Expr::Call { func, args, .. } => {
            rewrite_expr(func, resolved);
            for arg in args.iter_mut() {
                rewrite_expr(arg, resolved);
            }
        }
        Expr::Unary { operand, .. } => rewrite_expr(operand, resolved),
        Expr::Paren(inner, _) => rewrite_expr(inner, resolved),
        Expr::StructLiteral { fields, base, .. } => {
            for field in fields.iter_mut() {
                rewrite_expr(&mut field.value, resolved);
            }
            if let Some(base) = base {
                rewrite_expr(base, resolved);
            }
        }
        Expr::FieldAccess { object, .. } => rewrite_expr(object, resolved),
        Expr::EnumStructLiteral { fields, .. } => {
            for field in fields.iter_mut() {
                rewrite_expr(&mut field.value, resolved);
            }
        }
        Expr::Cast {
            expr, target_type, ..
        } => {
            rewrite_expr(expr, resolved);
            rewrite_type(target_type, resolved);
        }
        Expr::If {
            condition,
            then_block,
            else_if_blocks,
            else_block,
            ..
        } => {
            rewrite_expr(condition, resolved);
            rewrite_block(then_block, resolved);
            for (cond, block) in else_if_blocks.iter_mut() {
                rewrite_expr(cond, resolved);
                rewrite_block(block, resolved);
            }
            if let Some(block) = else_block {
                rewrite_block(block, resolved);
            }
        }
        Expr::Block { stmts, .. } => rewrite_block(stmts, resolved),
        Expr::Loop { body, .. } => rewrite_block(body, resolved),
        Expr::Unsafe { stmts, .. } => rewrite_block(stmts, resolved),
        Expr::Reference { operand, .. } => rewrite_expr(operand, resolved),
        Expr::Deref { operand, .. } => rewrite_expr(operand, resolved),
        Expr::Range { start, end, .. } => {
            rewrite_expr(start, resolved);
            rewrite_expr(end, resolved);
        }
        Expr::ArrayLiteral { elements, .. } => {
            for el in elements.iter_mut() {
                rewrite_expr(el, resolved);
            }
        }
        Expr::Index { object, index, .. } => {
            rewrite_expr(object, resolved);
            rewrite_expr(index, resolved);
        }
        Expr::TupleLiteral { elements, .. } => {
            for el in elements.iter_mut() {
                rewrite_expr(el, resolved);
            }
        }
        Expr::TupleIndex { object, .. } => rewrite_expr(object, resolved),
        Expr::ArrayRest { array, .. } => rewrite_expr(array, resolved),
        // Patterns carry no type annotations, so only the scrutinee, guards, and
        // bodies can host an aliased cast target.
        Expr::Match {
            scrutinee, arms, ..
        } => {
            rewrite_expr(scrutinee, resolved);
            for arm in arms.iter_mut() {
                if let Some(guard) = &mut arm.guard {
                    rewrite_expr(guard, resolved);
                }
                rewrite_expr(&mut arm.body, resolved);
            }
        }
        Expr::Literal(_, _) | Expr::Identifier(_) | Expr::Path { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::{Item, Stmt, Type};
    use crate::errors::ParseError;
    use crate::parse;

    /// Pull the declared type of the first `val` in the first function body.
    fn first_var_type(items: &[Item]) -> Option<Type> {
        for item in items {
            if let Item::Function(func) = item {
                for stmt in &func.body {
                    if let Stmt::VarDecl { ty, .. } = stmt {
                        return ty.clone();
                    }
                }
            }
        }
        None
    }

    fn named(ty: &Type) -> &str {
        match ty {
            Type::Named(ident) => ident.name.as_str(),
            Type::Reference { .. } => "<reference>",
            Type::Array { .. } => "<array>",
            Type::Tuple { .. } => "<tuple>",
            Type::Generic { .. } => "<generic>",
            Type::Tensor { .. } => "<tensor>",
            Type::ImplTrait { .. } => "<impl trait>",
            Type::DynTrait { .. } => "<dyn trait>",
        }
    }

    #[test]
    fn alias_expands_in_var_annotation() {
        let src = "type Meters = f64\nfunc main() -> i32 { val d: Meters = 3.0\n return 0 }";
        let items = parse(src).expect("parses");
        // The alias item must not survive into the program.
        assert_eq!(items.len(), 1);
        let ty = first_var_type(&items).expect("has a var decl");
        assert_eq!(named(&ty), "f64");
    }

    #[test]
    fn alias_chain_resolves_to_ultimate_target() {
        let src = "type A = B\ntype B = i32\nfunc main() -> i32 { val x: A = 1\n return 0 }";
        let items = parse(src).expect("parses");
        let ty = first_var_type(&items).expect("has a var decl");
        assert_eq!(named(&ty), "i32");
    }

    #[test]
    fn alias_expands_in_param_and_return() {
        let src = "type Id = i64\nfunc echo(x: Id) -> Id { x }";
        let items = parse(src).expect("parses");
        let func = items
            .iter()
            .find_map(|i| match i {
                Item::Function(f) => Some(f),
                _ => None,
            })
            .expect("function present");
        assert_eq!(named(&func.params[0].ty), "i64");
        assert_eq!(named(func.return_type.as_ref().unwrap()), "i64");
    }

    #[test]
    fn alias_expands_in_cast_target() {
        let src = "type Real = f64\nfunc main() -> i32 { val x = 1 as Real\n return 0 }";
        let items = parse(src).expect("parses");
        // The cast target must have been rewritten to f64; if not, semantic would
        // later fail. Here we just assert the program parses and the alias is gone.
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn duplicate_alias_is_rejected() {
        let src = "type A = i32\ntype A = f64\nfunc main() -> i32 { return 0 }";
        let err = parse(src).expect_err("duplicate rejected");
        assert!(matches!(err, ParseError::DuplicateTypeAlias { .. }));
    }

    #[test]
    fn alias_shadowing_builtin_is_rejected() {
        let src = "type i32 = f64\nfunc main() -> i32 { return 0 }";
        let err = parse(src).expect_err("builtin shadow rejected");
        assert!(matches!(err, ParseError::TypeAliasShadowsBuiltin { .. }));
    }

    #[test]
    fn cyclic_alias_is_rejected() {
        let src = "type A = B\ntype B = A\nfunc main() -> i32 { return 0 }";
        let err = parse(src).expect_err("cycle rejected");
        assert!(matches!(err, ParseError::CyclicTypeAlias { .. }));
    }
}
