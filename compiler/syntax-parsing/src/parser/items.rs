use lexical_analysis::TokenKind;
use shared_types::Identifier;

use crate::ast::{
    Attribute, ConstDef, GenericParam, GenericParamKind, Item, MethodDef, NewtypeDef, TraitMethod,
    Type,
};
use crate::errors::{ParseError, ParseResult};
use crate::precedence::Precedence;

use super::type_aliases::{expand_type_aliases, TypeAliasDecl};
use super::Parser;

impl Parser {
    /// Parse top-level items: function, struct, impl, const, or type-alias definitions.
    ///
    /// Type aliases are transparent and are resolved here: each declaration
    /// is collected, then every aliased type annotation in the remaining items is
    /// rewritten to its target type before the program is returned. No alias item
    /// reaches semantic analysis or codegen.
    pub(crate) fn parse_program(&mut self) -> ParseResult<Vec<Item>> {
        let mut items = Vec::new();
        let mut alias_decls: Vec<TypeAliasDecl> = Vec::new();

        self.skip_newlines();
        while !self.is_at_end() {
            let attributes = self.parse_attributes()?;
            self.skip_newlines();

            if self.check(&TokenKind::Func) {
                let func = self.parse_function(attributes)?;
                items.push(Item::Function(func));
            } else if self.check(&TokenKind::Struct) {
                let s = self.parse_struct_def(attributes)?;
                items.push(Item::Struct(s));
            } else if !attributes.is_empty() {
                // Attributes attach only to functions and structs today; rejecting here
                // gives an actionable diagnostic instead of silently dropping them.
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function or struct definition after attribute".to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function or struct definition after attribute".to_string(),
                    span: token.span,
                });
            } else if self.check(&TokenKind::Enum) {
                let e = self.parse_enum_def()?;
                items.push(Item::Enum(e));
            } else if self.check(&TokenKind::Trait) {
                let trait_def = self.parse_trait_def()?;
                items.push(Item::Trait(trait_def));
            } else if self.check(&TokenKind::Impl) {
                let impl_def = self.parse_impl_def()?;
                items.push(Item::Impl(impl_def));
            } else if self.check(&TokenKind::Const) {
                let c = self.parse_const_def()?;
                items.push(Item::Const(c));
            } else if self.check(&TokenKind::Type) {
                alias_decls.push(self.parse_type_alias()?);
            } else if self.check(&TokenKind::Newtype) {
                let nt = self.parse_newtype_def()?;
                items.push(Item::Newtype(nt));
            } else {
                let token = self.peek().ok_or(ParseError::UnexpectedEof {
                    expected: "function, struct, enum, impl, const, type, or newtype definition"
                        .to_string(),
                })?;
                return Err(ParseError::UnexpectedToken {
                    found: token.kind.clone(),
                    expected: "function, struct, enum, impl, const, type, or newtype definition"
                        .to_string(),
                    span: token.span,
                });
            }
            self.skip_newlines();
        }

        // Inject trait default methods before alias expansion so the copied bodies are
        // alias-expanded along with the rest of each impl.
        inject_trait_defaults(&mut items);
        expand_type_aliases(&mut items, alias_decls)?;
        Ok(items)
    }

    /// Parse zero or more `@name` / `@name(arg, ...)` attributes attached to the
    /// following item. Stops at the first token that is not `@`.
    pub(crate) fn parse_attributes(&mut self) -> ParseResult<Vec<Attribute>> {
        let mut attributes = Vec::new();
        loop {
            self.skip_newlines();
            if !self.check(&TokenKind::At) {
                break;
            }
            attributes.push(self.parse_attribute()?);
        }
        Ok(attributes)
    }

    /// Parse a single `@name` or `@name(arg, ...)` attribute. Assumes the
    /// current token is `@`.
    pub(super) fn parse_attribute(&mut self) -> ParseResult<Attribute> {
        let at = self.consume(TokenKind::At, "'@'")?;

        let name_token = self.consume(TokenKind::Identifier(String::new()), "attribute name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "attribute name".to_string(),
                span: name_token.span,
            });
        };

        let mut args: Vec<Identifier> = Vec::new();
        let mut end_span = name.span;

        if self.check(&TokenKind::LeftParen) {
            self.advance(); // consume '('
            self.skip_newlines();

            if !self.check(&TokenKind::RightParen) {
                loop {
                    let arg_token =
                        self.consume(TokenKind::Identifier(String::new()), "attribute argument")?;
                    let arg = if let TokenKind::Identifier(n) = arg_token.kind {
                        Identifier {
                            name: n,
                            span: arg_token.span,
                        }
                    } else {
                        return Err(ParseError::UnexpectedToken {
                            found: arg_token.kind,
                            expected: "attribute argument".to_string(),
                            span: arg_token.span,
                        });
                    };
                    args.push(arg);
                    self.skip_newlines();
                    if !self.check(&TokenKind::Comma) {
                        break;
                    }
                    self.advance(); // consume ','
                    self.skip_newlines();
                }
            }

            let close = self.consume(TokenKind::RightParen, "')'")?;
            end_span = close.span;
        }

        Ok(Attribute {
            name,
            args,
            span: at.span.merge(end_span),
        })
    }

    /// Parse a module-level constant: `const NAME: Type = expr`
    pub(crate) fn parse_const_def(&mut self) -> ParseResult<ConstDef> {
        let start = self.consume(TokenKind::Const, "'const'")?;
        self.skip_newlines();

        let name_token = self.consume(TokenKind::Identifier(String::new()), "constant name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            Identifier {
                name: n,
                span: name_token.span,
            }
        } else {
            return Err(ParseError::UnexpectedToken {
                found: name_token.kind,
                expected: "constant name".to_string(),
                span: name_token.span,
            });
        };

        self.skip_newlines();
        self.consume(TokenKind::Colon, "':'")?;
        self.skip_newlines();

        let ty = self.parse_type()?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let value = self.parse_expr(Precedence::Lowest)?;
        let span = start.span.merge(value.span());

        Ok(ConstDef {
            name,
            ty,
            value,
            span,
        })
    }

    /// Parse a newtype declaration: `newtype Name = InnerType`.
    ///
    /// Unlike a `type` alias, a newtype is a distinct nominal type, so it is kept as
    /// an `Item::Newtype` for semantic analysis rather than expanded at parse time.
    pub(crate) fn parse_newtype_def(&mut self) -> ParseResult<NewtypeDef> {
        let start = self.consume(TokenKind::Newtype, "'newtype'")?;
        self.skip_newlines();

        let name = self.consume_identifier("newtype name")?;

        self.skip_newlines();
        self.consume(TokenKind::Equal, "'='")?;
        self.skip_newlines();

        let inner = self.parse_type()?;
        let span = start.span.merge(inner.span());

        Ok(NewtypeDef { name, inner, span })
    }
}

/// Inject each trait's default methods into the `impl Trait for Type` blocks that
/// omit them, a whole-program parse-time desugar (like type-alias expansion).
///
/// After this pass every trait impl carries a concrete method for each trait method it
/// is expected to provide, so semantic analysis and HIR lowering treat trait methods as
/// ordinary inherent methods — traits are fully erased. A method the implementor writes
/// explicitly is left untouched (it overrides the default).
fn inject_trait_defaults(items: &mut [Item]) {
    use std::collections::HashMap;

    // Map trait name -> its default (bodied) methods.
    let mut defaults: HashMap<String, Vec<TraitMethod>> = HashMap::new();
    for item in items.iter() {
        if let Item::Trait(def) = item {
            let bodied: Vec<TraitMethod> = def
                .methods
                .iter()
                .filter(|m| m.default_body.is_some())
                .cloned()
                .collect();
            defaults.insert(def.name.name.clone(), bodied);
        }
    }
    if defaults.is_empty() {
        return;
    }

    for item in items.iter_mut() {
        let Item::Impl(imp) = item else { continue };
        let Some(trait_name) = &imp.trait_name else {
            continue;
        };
        let Some(trait_defaults) = defaults.get(&trait_name.name) else {
            continue;
        };
        for method in trait_defaults {
            if imp.methods.iter().any(|m| m.name.name == method.name.name) {
                continue;
            }
            let Some(body) = &method.default_body else {
                continue;
            };
            imp.methods.push(MethodDef {
                name: method.name.clone(),
                self_param: method.self_param.clone(),
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                body: body.clone(),
                attributes: Vec::new(),
                span: method.span,
            });
        }
    }
}

/// Rewrite argument-position `impl Trait` into fresh trait-bounded generic
/// parameters. Each `impl Trait` occurrence — including one nested inside a reference,
/// array, tuple, or generic application — becomes a distinct anonymous type parameter
/// `__implN: Trait` appended to `generics`, and the annotation is replaced by a plain
/// named reference to it. This desugar lets static dispatch reuse the ordinary
/// monomorphized-generic machinery unchanged.
pub(super) fn desugar_impl_trait_params(
    ty: &Type,
    counter: &mut usize,
    generics: &mut Vec<GenericParam>,
) -> Type {
    match ty {
        Type::ImplTrait { trait_name, span } => {
            let name = format!("__impl{}", *counter);
            *counter += 1;
            let ident = Identifier { name, span: *span };
            generics.push(GenericParam {
                name: ident.clone(),
                kind: GenericParamKind::Type,
                bounds: vec![trait_name.clone()],
                span: *span,
            });
            Type::Named(ident)
        }
        Type::Reference {
            inner,
            mutable,
            lifetime,
            span,
        } => Type::Reference {
            inner: Box::new(desugar_impl_trait_params(inner, counter, generics)),
            mutable: *mutable,
            lifetime: lifetime.clone(),
            span: *span,
        },
        Type::Array {
            element,
            size,
            span,
        } => Type::Array {
            element: Box::new(desugar_impl_trait_params(element, counter, generics)),
            size: size.clone(),
            span: *span,
        },
        Type::Tuple { elements, span } => Type::Tuple {
            elements: elements
                .iter()
                .map(|e| desugar_impl_trait_params(e, counter, generics))
                .collect(),
            span: *span,
        },
        other => other.clone(),
    }
}
