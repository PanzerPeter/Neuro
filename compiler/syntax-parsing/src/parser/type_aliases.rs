// Type-alias declarations: `type Name = TargetType`.
//
// A `type` alias is *transparent*: the alias and its target are interchangeable
// and no new nominal type is introduced. We therefore resolve aliases entirely at
// parse time by substituting every aliased type annotation with its target type,
// exactly as compound assignment desugars before reaching later stages. The result
// is that semantic analysis and codegen never observe an alias: an unknown target
// name is reported by the existing semantic `UnknownTypeName` check against the
// real type, with the diagnostic pointing at the alias *use* site.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use lexical_analysis::TokenKind;
use shared_types::{Identifier, Span};

use crate::errors::{ParseError, ParseResult};
use ast_types::{
    EnumPatternPayload, Expr, GenericArg, GenericParam, GenericParamKind, ImplDef, InterpPart,
    Item, Pattern, Place, Stmt, TensorIndexArg, Type,
};

use super::Parser;
use super::types::SELF_TYPE_NAME;

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

    let mut direct: HashMap<&str, (&Type, Span)> = HashMap::new();
    for decl in &decls {
        if BUILTIN_TYPE_NAMES.contains(&decl.name.name.as_str()) {
            return Err(ParseError::TypeAliasShadowsBuiltin {
                name: decl.name.name.clone(),
                span: decl.name.span,
            });
        }
        if direct
            .insert(&decl.name.name, (&decl.target, decl.name.span))
            .is_some()
        {
            return Err(ParseError::DuplicateTypeAlias {
                name: decl.name.name.clone(),
                span: decl.name.span,
            });
        }
    }

    // Declaration order, so a program with two cycles always reports the same one.
    let mut resolved: HashMap<String, Type> = HashMap::new();
    for decl in &decls {
        resolve_alias(&decl.name.name, &direct, &mut resolved)?;
    }

    for item in items.iter_mut() {
        rewrite_item(item, &resolved);
    }
    Ok(())
}

/// Replace `Self` inside every `impl` block with the type the block extends.
///
/// Within one block `Self` is an alias for that type, so this is alias expansion scoped
/// to the block: type positions take the whole type (`Wrapper<T>` for
/// `impl<T> Wrapper<T>`), and the struct-literal, path and pattern positions take its
/// name. It runs after trait defaults are injected, so a default body naming `Self`
/// reads as each implementor. A `Self` left anywhere else is the trait's own, or an
/// error, and the type checker answers both.
pub(crate) fn expand_self(items: &mut [Item]) {
    for item in items.iter_mut() {
        if let Item::Module(def) = item {
            expand_self(&mut def.items);
            continue;
        }
        let Item::Impl(def) = item else { continue };
        let resolved = HashMap::from([(SELF_TYPE_NAME.to_string(), impl_self_type(def))]);
        rewrite_item(item, &resolved);
    }
}

fn impl_self_type(def: &ImplDef) -> Type {
    if def.type_args.is_empty() {
        return Type::Named(def.type_name.clone());
    }
    Type::Generic {
        name: def.type_name.clone(),
        args: def
            .type_args
            .iter()
            .cloned()
            .map(GenericArg::Type)
            .collect(),
        span: def.type_name.span,
    }
}

/// Point a `Self` in a name position (a struct literal, a path, a pattern) at the
/// extended type's name. Only [`expand_self`] binds `Self`, since an alias cannot be
/// named after a keyword, so alias expansion passes every such name through.
fn rename_self(name: &mut Identifier, resolved: &HashMap<String, Type>) {
    if name.name != SELF_TYPE_NAME {
        return;
    }
    if let Some(Type::Named(target) | Type::Generic { name: target, .. }) =
        resolved.get(SELF_TYPE_NAME)
    {
        name.name = target.name.clone();
    }
}

fn rename_self_in_pattern(pattern: &mut Pattern, resolved: &HashMap<String, Type>) {
    let payload = match pattern {
        Pattern::Enum {
            enum_name, payload, ..
        } => {
            rename_self(enum_name, resolved);
            payload
        }
        Pattern::UnqualifiedEnum { payload, .. } => payload,
        Pattern::Wildcard(_)
        | Pattern::Binding(_)
        | Pattern::Literal(_, _)
        | Pattern::Range { .. } => return,
    };
    match payload {
        EnumPatternPayload::Unit => {}
        EnumPatternPayload::Tuple(subs) => {
            for sub in subs {
                rename_self_in_pattern(sub, resolved);
            }
        }
        EnumPatternPayload::Struct(fields) => {
            for field in fields {
                rename_self_in_pattern(&mut field.pattern, resolved);
            }
        }
    }
}

/// Resolve `start` and every alias its target mentions into `resolved`, each one
/// fully expanded: an alias inside a target (`type Pair = (Elem, Elem)`) is
/// substituted as well, not only an alias that is the whole target.
///
/// A depth-first walk with an explicit stack, so a long alias chain costs no call
/// depth. An alias reached again while it is still being expanded names an infinite
/// type and is reported as the cycle.
fn resolve_alias(
    start: &str,
    direct: &HashMap<&str, (&Type, Span)>,
    resolved: &mut HashMap<String, Type>,
) -> ParseResult<()> {
    let mut in_progress: HashSet<&str> = HashSet::new();
    let mut stack: Vec<(&str, bool)> = vec![(start, false)];
    while let Some((name, references_done)) = stack.pop() {
        if resolved.contains_key(name) {
            continue;
        }
        let Some(&(target, _)) = direct.get(name) else {
            continue;
        };
        if references_done {
            in_progress.remove(name);
            let mut expanded = target.clone();
            rewrite_type(&mut expanded, resolved);
            resolved.insert(name.to_string(), expanded);
            continue;
        }
        in_progress.insert(name);
        stack.push((name, true));
        let mut references = Vec::new();
        type_names(target, &mut references);
        for reference in references {
            let Some((&key, &(_, span))) = direct.get_key_value(reference) else {
                continue;
            };
            if in_progress.contains(key) {
                return Err(ParseError::CyclicTypeAlias {
                    name: key.to_string(),
                    span,
                });
            }
            if !resolved.contains_key(key) {
                stack.push((key, false));
            }
        }
    }
    Ok(())
}

/// Every name `ty` mentions in the positions [`rewrite_type`] substitutes.
fn type_names<'a>(ty: &'a Type, out: &mut Vec<&'a str>) {
    match ty {
        Type::Named(ident) => out.push(&ident.name),
        Type::Reference { inner, .. } => type_names(inner, out),
        Type::Array { element, .. } | Type::Slice { element, .. } => type_names(element, out),
        Type::Tuple { elements, .. } => {
            for element in elements {
                type_names(element, out);
            }
        }
        Type::Generic { args, .. } => {
            for arg in args {
                if let ast_types::GenericArg::Type(inner) = arg {
                    type_names(inner, out);
                }
            }
        }
        Type::Tensor { element_type, .. } => type_names(element_type, out),
        Type::Function { params, ret, .. } => {
            for param in params {
                type_names(param, out);
            }
            type_names(ret, out);
        }
        Type::ImplTrait { .. } | Type::DynTrait { .. } => {}
    }
}

/// The aliases in force inside an item declaring `generics`: a generic parameter
/// shadows an alias of the same name, as an inner name shadows an outer one.
fn without_shadowed<'a>(
    resolved: &'a HashMap<String, Type>,
    generics: &[GenericParam],
) -> Cow<'a, HashMap<String, Type>> {
    if generics.iter().any(|g| resolved.contains_key(&g.name.name)) {
        let mut visible = resolved.clone();
        for generic in generics {
            visible.remove(&generic.name.name);
        }
        Cow::Owned(visible)
    } else {
        Cow::Borrowed(resolved)
    }
}

/// Rewrite the types a generic parameter list carries: a const parameter's type and
/// the `<Assoc = T>` bindings of each bound.
fn rewrite_generics(generics: &mut [GenericParam], resolved: &HashMap<String, Type>) {
    for generic in generics {
        if let GenericParamKind::Const(ty) = &mut generic.kind {
            rewrite_type(ty, resolved);
        }
        for bound in &mut generic.bounds {
            for (_, ty) in &mut bound.assoc_bindings {
                rewrite_type(ty, resolved);
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
                match ty {
                    Type::Named(new_ident) => new_ident.span = use_span,
                    Type::Generic { span, .. } => *span = use_span,
                    _ => {}
                }
            }
        }
        Type::Reference { inner, .. } => rewrite_type(inner, resolved),
        Type::Array { element, .. } | Type::Slice { element, .. } => {
            rewrite_type(element, resolved)
        }
        Type::Tuple { elements, .. } => {
            for element in elements {
                rewrite_type(element, resolved);
            }
        }
        Type::Generic { args, .. } => {
            for arg in args {
                if let ast_types::GenericArg::Type(inner) = arg {
                    rewrite_type(inner, resolved);
                }
            }
        }
        Type::Tensor { element_type, .. } => rewrite_type(element_type, resolved),
        Type::Function { params, ret, .. } => {
            for param in params.iter_mut() {
                rewrite_type(param, resolved);
            }
            rewrite_type(ret, resolved);
        }
        // `impl Trait` / `dyn Trait` name a trait, not a type alias, so a type
        // alias never substitutes into them.
        Type::ImplTrait { .. } | Type::DynTrait { .. } => {}
    }
}

fn rewrite_item(item: &mut Item, resolved: &HashMap<String, Type>) {
    match item {
        Item::Function(func) => {
            let resolved = &*without_shadowed(resolved, &func.generics);
            rewrite_generics(&mut func.generics, resolved);
            for predicate in &mut func.where_predicates {
                rewrite_expr(predicate, resolved);
            }
            for param in &mut func.params {
                rewrite_type(&mut param.ty, resolved);
            }
            if let Some(ret) = &mut func.return_type {
                rewrite_type(ret, resolved);
            }
            rewrite_block(&mut func.body, resolved);
        }
        Item::Struct(def) => {
            let resolved = &*without_shadowed(resolved, &def.generics);
            rewrite_generics(&mut def.generics, resolved);
            for predicate in &mut def.where_predicates {
                rewrite_expr(predicate, resolved);
            }
            for field in &mut def.fields {
                rewrite_type(&mut field.ty, resolved);
            }
        }
        Item::Enum(def) => {
            let resolved = &*without_shadowed(resolved, &def.generics);
            rewrite_generics(&mut def.generics, resolved);
            for variant in &mut def.variants {
                match &mut variant.payload {
                    ast_types::VariantPayload::Unit => {}
                    ast_types::VariantPayload::Tuple(tys) => {
                        for ty in tys.iter_mut() {
                            rewrite_type(ty, resolved);
                        }
                    }
                    ast_types::VariantPayload::Struct(fields) => {
                        for field in fields.iter_mut() {
                            rewrite_type(&mut field.ty, resolved);
                        }
                    }
                }
            }
        }
        Item::Impl(def) => {
            let resolved = &*without_shadowed(resolved, &def.generics);
            rewrite_generics(&mut def.generics, resolved);
            for ty in &mut def.type_args {
                rewrite_type(ty, resolved);
            }
            for predicate in &mut def.where_predicates {
                rewrite_expr(predicate, resolved);
            }
            for (_, ty) in &mut def.assoc_types {
                rewrite_type(ty, resolved);
            }
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
        // A trait's method signatures may reference aliased types. Default bodies
        // were already copied into the impls that omit them, which are rewritten on
        // their own, and nothing downstream reads the trait's copy.
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
        // Neither an import nor the `@no_prelude` marker names a type annotation.
        Item::Import(_) | Item::NoPrelude(_) => {}
        // Aliases stay file-scoped: they are expanded at parse time, so one declared
        // beside a `module` block still reads inside it.
        Item::Module(def) => {
            for item in &mut def.items {
                rewrite_item(item, resolved);
            }
        }
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
        Stmt::Assign { place, value, .. } => {
            rewrite_place(place, resolved);
            rewrite_expr(value, resolved);
        }
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
        Stmt::ForRange {
            start,
            end,
            step,
            body,
            ..
        } => {
            rewrite_expr(start, resolved);
            rewrite_expr(end, resolved);
            if let Some(step) = step {
                rewrite_expr(step, resolved);
            }
            rewrite_block(body, resolved);
        }
        Stmt::ForEach { iterable, body, .. } => {
            rewrite_expr(iterable, resolved);
            rewrite_block(body, resolved);
        }
        Stmt::ValElse {
            pattern,
            value,
            else_block,
            ..
        } => {
            rename_self_in_pattern(pattern, resolved);
            rewrite_expr(value, resolved);
            rewrite_block(else_block, resolved);
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
        Expr::Call {
            func,
            type_args,
            args,
            ..
        } => {
            rewrite_expr(func, resolved);
            for arg in type_args.iter_mut() {
                if let ast_types::GenericArg::Type(ty) = arg {
                    rewrite_type(ty, resolved);
                }
            }
            for arg in args.iter_mut() {
                rewrite_expr(arg, resolved);
            }
        }
        Expr::Unary { operand, .. } => rewrite_expr(operand, resolved),
        Expr::Try { operand, .. } => rewrite_expr(operand, resolved),
        Expr::Paren(inner, _) => rewrite_expr(inner, resolved),
        Expr::StructLiteral {
            name, fields, base, ..
        } => {
            rename_self(name, resolved);
            for field in fields.iter_mut() {
                rewrite_expr(&mut field.value, resolved);
            }
            if let Some(base) = base {
                rewrite_expr(base, resolved);
            }
        }
        Expr::FieldAccess { object, .. } => rewrite_expr(object, resolved),
        Expr::InterpString { parts, .. } => {
            for part in parts.iter_mut() {
                if let InterpPart::Formatted { expr, .. } = part {
                    rewrite_expr(expr, resolved);
                }
            }
        }
        Expr::EnumStructLiteral {
            enum_name, fields, ..
        } => {
            rename_self(enum_name, resolved);
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
        Expr::Pool { stmts, .. } => rewrite_block(stmts, resolved),
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
        Expr::TensorIndex {
            object, indices, ..
        } => {
            rewrite_expr(object, resolved);
            for index in indices.iter_mut() {
                rewrite_index_arg(index, resolved);
            }
        }
        Expr::TupleLiteral { elements, .. } => {
            for el in elements.iter_mut() {
                rewrite_expr(el, resolved);
            }
        }
        Expr::TupleIndex { object, .. } => rewrite_expr(object, resolved),
        Expr::ArrayRest { array, .. } => rewrite_expr(array, resolved),
        // Patterns carry no type annotations; only a `Self::Variant` head is renamed.
        Expr::Match {
            scrutinee, arms, ..
        } => {
            rewrite_expr(scrutinee, resolved);
            for arm in arms.iter_mut() {
                for pattern in arm.patterns.iter_mut() {
                    rename_self_in_pattern(pattern, resolved);
                }
                if let Some(guard) = &mut arm.guard {
                    rewrite_expr(guard, resolved);
                }
                rewrite_expr(&mut arm.body, resolved);
            }
        }
        Expr::Closure {
            params, ret, body, ..
        } => {
            for param in params.iter_mut() {
                if let Some(ty) = &mut param.ty {
                    rewrite_type(ty, resolved);
                }
            }
            if let Some(ret) = ret {
                rewrite_type(ret, resolved);
            }
            rewrite_expr(body, resolved);
        }
        Expr::Path { type_name, .. } => rename_self(type_name, resolved),
        Expr::Literal(_, _) | Expr::Identifier(_) | Expr::Compose { .. } => {}
    }
}

/// Rewrite the aliased types inside the expressions an assignment place reaches
/// through.
fn rewrite_place(place: &mut Place, resolved: &HashMap<String, Type>) {
    match place {
        Place::Var(_) => {}
        Place::Field { object, .. }
        | Place::Deref {
            pointer: object, ..
        } => rewrite_expr(object, resolved),
        Place::Index { object, index, .. } => {
            rewrite_expr(object, resolved);
            rewrite_expr(index, resolved);
        }
        Place::TensorIndex {
            object, indices, ..
        } => {
            rewrite_expr(object, resolved);
            for index in indices {
                rewrite_index_arg(index, resolved);
            }
        }
    }
}

/// Rewrite the aliased types inside one tensor index argument. A full-axis `..` names
/// no expression, so only the other two forms carry anything to rewrite.
fn rewrite_index_arg(index: &mut TensorIndexArg, resolved: &HashMap<String, Type>) {
    match index {
        TensorIndexArg::Position(expr) => rewrite_expr(expr, resolved),
        TensorIndexArg::Range {
            start, end, step, ..
        } => {
            rewrite_expr(start, resolved);
            rewrite_expr(end, resolved);
            if let Some(step) = step {
                rewrite_expr(step, resolved);
            }
        }
        TensorIndexArg::FullAxis(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::errors::ParseError;
    use crate::parse;
    use ast_types::{Item, Stmt, Type};

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
            Type::Slice { .. } => "<slice>",
            Type::Tuple { .. } => "<tuple>",
            Type::Generic { .. } => "<generic>",
            Type::Tensor { .. } => "<tensor>",
            Type::ImplTrait { .. } => "<impl trait>",
            Type::DynTrait { .. } => "<dyn trait>",
            Type::Function { .. } => "<function>",
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

    fn function<'a>(items: &'a [Item], name: &str) -> &'a ast_types::FunctionDef {
        items
            .iter()
            .find_map(|i| match i {
                Item::Function(f) if f.name.name == name => Some(f),
                _ => None,
            })
            .expect("function present")
    }

    /// The names inside a tuple type, in order.
    fn tuple_names(ty: &Type) -> Vec<&str> {
        let Type::Tuple { elements, .. } = ty else {
            panic!("expected a tuple type, found {ty:?}");
        };
        elements.iter().map(named).collect()
    }

    #[test]
    fn alias_inside_an_alias_target_expands_in_either_order() {
        // `type Pair = (Elem, Elem)` used to substitute the tuple with `Elem` still in
        // it, so the checker reported `Elem` as an unknown type.
        for src in [
            "type Elem = i32\ntype Pair = (Elem, Elem)\nfunc f(p: Pair) {}",
            "type Pair = (Elem, Elem)\ntype Elem = i32\nfunc f(p: Pair) {}",
            "type Pair = (Inner, i32)\ntype Inner = Elem\ntype Elem = i32\nfunc f(p: Pair) {}",
        ] {
            let items = parse(src).expect("parses");
            assert_eq!(
                tuple_names(&function(&items, "f").params[0].ty),
                ["i32", "i32"],
                "{src}"
            );
        }
    }

    #[test]
    fn cycle_through_a_compound_target_is_rejected() {
        for src in [
            "type A = (B, i32)\ntype B = (A, i32)\nfunc main() {}",
            "type L = Vec<L>\nfunc main() {}",
            "type R = &R\nfunc main() {}",
        ] {
            let err = parse(src).expect_err("an infinite type is rejected");
            assert!(
                matches!(err, ParseError::CyclicTypeAlias { .. }),
                "{src}: {err}"
            );
        }
    }

    #[test]
    fn cycle_is_reported_against_an_alias_on_the_cycle() {
        // `Start` only leads into the cycle; the diagnostic names a member of it.
        let src = "type Start = B\ntype B = C\ntype C = B\nfunc main() {}";
        let err = parse(src).expect_err("cycle rejected");
        let ParseError::CyclicTypeAlias { name, .. } = err else {
            panic!("expected a cycle error, found {err:?}");
        };
        assert!(name == "B" || name == "C", "{name}");
    }

    #[test]
    fn long_alias_chain_resolves() {
        // Each alias used to re-walk the whole chain behind it with a linear visited
        // scan, cubic in the chain length.
        let n = 3000;
        let mut src: String = (0..n)
            .map(|i| format!("type T{i} = T{}\n", i + 1))
            .collect();
        src.push_str(&format!("type T{n} = i32\nfunc f(x: T0) {{}}"));
        let items = parse(&src).expect("parses");
        assert_eq!(named(&function(&items, "f").params[0].ty), "i32");
    }

    #[test]
    fn alias_expands_in_turbofish_and_generic_positions() {
        let src = "type Num = i32\ntype Len = u32\n\
            func f<const N: Len, T: Source<Item = Num>>(x: T) where N > (0 as Len) {\n\
                val y = id::<Num>(1)\n\
            }";
        let items = parse(src).expect("parses");
        let func = function(&items, "f");
        let ast_types::GenericParamKind::Const(len) = &func.generics[0].kind else {
            panic!("expected a const parameter");
        };
        assert_eq!(named(len), "u32");
        assert_eq!(
            named(&func.generics[1].bounds[0].assoc_bindings[0].1),
            "i32"
        );
        let ast_types::Expr::Binary { right, .. } = &func.where_predicates[0] else {
            panic!("expected a comparison predicate");
        };
        let ast_types::Expr::Paren(cast, _) = &**right else {
            panic!("expected a parenthesized cast");
        };
        let ast_types::Expr::Cast { target_type, .. } = &**cast else {
            panic!("expected a cast");
        };
        assert_eq!(named(target_type), "u32");
        let Stmt::VarDecl {
            init: Some(ast_types::Expr::Call { type_args, .. }),
            ..
        } = &func.body[0]
        else {
            panic!("expected a turbofish call");
        };
        let ast_types::GenericArg::Type(arg) = &type_args[0] else {
            panic!("expected a type argument");
        };
        assert_eq!(named(arg), "i32");
    }

    #[test]
    fn alias_expands_in_impl_bindings_and_type_arguments() {
        let src = "type Num = i32\nimpl Add for Wrap<Num> {\n    type Output = Num\n}";
        let items = parse(src).expect("parses");
        let Item::Impl(imp) = &items[0] else {
            panic!("expected an impl");
        };
        assert_eq!(named(&imp.type_args[0]), "i32");
        assert_eq!(named(&imp.assoc_types[0].1), "i32");
    }

    #[test]
    fn generic_parameter_shadows_an_alias_of_the_same_name() {
        let src = "type T = bool\n\
            func id<T>(x: T) -> T { x }\n\
            struct Boxed<T> { v: T }\n\
            func plain(x: T) {}";
        let items = parse(src).expect("parses");
        let id = function(&items, "id");
        assert_eq!(named(&id.params[0].ty), "T");
        assert_eq!(named(id.return_type.as_ref().expect("return type")), "T");
        let Some(Item::Struct(boxed)) = items.get(1) else {
            panic!("expected the struct");
        };
        assert_eq!(named(&boxed.fields[0].ty), "T");
        // Outside the generic item the alias still applies.
        assert_eq!(named(&function(&items, "plain").params[0].ty), "bool");
    }
}
