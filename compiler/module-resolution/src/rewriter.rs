//! Rewriting: resolve every name against the module that owns it, then erase the route.
//!
//! Two routes reach the same flat namespace. A qualifier is verified against the module it
//! names and stripped; an imported name is looked up in the importing module's table and
//! replaced by what it stands for. After this pass no name carries a `::` module prefix and
//! no name depends on an import, which is why semantic analysis, HIR lowering, and both
//! backends need to know nothing about either.

use ast_types::{EnumPatternPayload, Expr, Pattern};
use shared_types::Identifier;

use crate::imports::ImportScope;
use crate::loader::{site_segments, ModuleGraph};
use crate::walk::{walk_items, Site};
use crate::ModuleError;

pub(crate) fn strip_qualifiers(
    graph: &mut ModuleGraph,
    scopes: &[ImportScope],
) -> Result<(), ModuleError> {
    // One scope per module, in module order, so the index is the module id.
    for (id, scope) in scopes.iter().enumerate() {
        let mut items = std::mem::take(&mut graph.modules[id].items);
        let outcome = {
            let view = &*graph;
            let mut rewrite = |site: Site<'_>| resolve_site(view, scope, id, site);
            walk_items(&mut items, &mut rewrite)
        };
        graph.modules[id].items = items;
        outcome?;
    }
    Ok(())
}

/// Resolve one name written in module `from`.
///
/// A path whose head names neither a module nor an import is left exactly as it was:
/// `Point::new` and `Option::Some` are associated-function and enum-variant paths, and
/// belong to the type checker rather than here.
fn resolve_site(
    graph: &ModuleGraph,
    scope: &ImportScope,
    from: usize,
    site: Site<'_>,
) -> Result<(), ModuleError> {
    if let Site::Pattern(pattern) = site {
        return resolve_pattern(graph, scope, from, pattern);
    }

    let segments = site_segments(&site);
    match segments.len() {
        0 => Ok(()),
        1 => {
            resolve_imported_name(scope, site, &segments[0]);
            Ok(())
        }
        _ => resolve_qualified(graph, scope, from, site, &segments),
    }
}

/// Replace a bare name an import bound. A name no import mentions is left alone, so an
/// ordinary local or a plain call reaches this and passes straight through.
fn resolve_imported_name(scope: &ImportScope, site: Site<'_>, name: &str) {
    if let Some(item) = scope.rename(name) {
        let item = item.to_string();
        rewrite_bare(site, &item);
        return;
    }
    let Some((owner, variant)) = scope.variant(name) else {
        return;
    };
    // A variant used as a value: `Some` becomes the `Option::Some` path the checker reads,
    // whether it is called (`Some(42)`) or standing alone (`None`).
    let Site::Expr(expr) = site else {
        return;
    };
    let Expr::Identifier(ident) = &*expr else {
        return;
    };
    *expr = Expr::Path {
        type_name: Identifier {
            name: owner.to_string(),
            span: ident.span,
        },
        member: Identifier {
            name: variant.to_string(),
            span: ident.span,
        },
        span: ident.span,
    };
}

/// Resolve a `a::b(::c)` path against the modules and module aliases in scope.
fn resolve_qualified(
    graph: &ModuleGraph,
    scope: &ImportScope,
    from: usize,
    site: Site<'_>,
    segments: &[String],
) -> Result<(), ModuleError> {
    let chain = &segments[..segments.len() - 1];

    let mut module = None;
    let mut consumed = 0;
    if !graph.declares_type(from, &chain[0]) {
        // An `as` alias renames the head only; the rest of the chain descends as usual.
        if let Some(id) = scope.module(&chain[0]) {
            module = Some(id);
            consumed = 1;
        }
        for segment in &chain[consumed..] {
            match graph.resolve_segment(from, module, segment) {
                Some(id) => {
                    module = Some(id);
                    consumed += 1;
                }
                None => break,
            }
        }
    }

    let path = segments.join("::");
    let Some(module) = module else {
        // Two segments with no module prefix is the ordinary `Type::member` shape. Three
        // or more can only have been meant as a module path, so a silent pass-through
        // would surface as a baffling "unknown type `a::b`" later.
        if segments.len() > 2 {
            return Err(ModuleError::UnknownModule {
                path,
                from: graph.display(from).to_string(),
                head: segments[0].clone(),
            });
        }
        return Ok(());
    };

    let rest = &segments[consumed..];
    match rest {
        [item] => {
            if !graph.declares(module, item) {
                return Err(ModuleError::UndeclaredItem {
                    module: graph.path_of(module).to_string(),
                    item: item.clone(),
                    from: graph.display(from).to_string(),
                });
            }
            rewrite_bare(site, item);
            Ok(())
        }
        [ty, member] => {
            if !graph.declares_type(module, ty) {
                return Err(ModuleError::UndeclaredItem {
                    module: graph.path_of(module).to_string(),
                    item: ty.clone(),
                    from: graph.display(from).to_string(),
                });
            }
            rewrite_member(site, ty, member, &path, graph.display(from))
        }
        _ => Err(ModuleError::PathTooDeep {
            path,
            from: graph.display(from).to_string(),
        }),
    }
}

/// Resolve a pattern that names a variant.
///
/// A payload-less variant (`None`) is written exactly like a binding, so the import table
/// is the only thing that tells them apart; a payload-carrying one (`Some(n)`) is
/// unambiguous and is an error when no import accounts for it.
fn resolve_pattern(
    graph: &ModuleGraph,
    scope: &ImportScope,
    from: usize,
    pattern: &mut Pattern,
) -> Result<(), ModuleError> {
    match pattern {
        Pattern::Binding(ident) => {
            let Some((owner, variant)) = scope.variant(&ident.name) else {
                return Ok(());
            };
            *pattern = enum_pattern(owner, variant, EnumPatternPayload::Unit, ident.span);
            Ok(())
        }
        Pattern::UnqualifiedEnum {
            variant,
            payload,
            span,
        } => {
            let Some((owner, name)) = scope.variant(&variant.name) else {
                return Err(ModuleError::UnimportedVariant {
                    variant: variant.name.clone(),
                    from: graph.display(from).to_string(),
                });
            };
            *pattern = enum_pattern(owner, name, payload.clone(), *span);
            Ok(())
        }
        Pattern::Enum { enum_name, .. } => {
            if let Some(item) = scope.rename(&enum_name.name) {
                enum_name.name = item.to_string();
            }
            Ok(())
        }
        Pattern::Wildcard(_) | Pattern::Literal(_, _) | Pattern::Range { .. } => Ok(()),
    }
}

fn enum_pattern(
    owner: &str,
    variant: &str,
    payload: EnumPatternPayload,
    span: shared_types::Span,
) -> Pattern {
    Pattern::Enum {
        enum_name: Identifier {
            name: owner.to_string(),
            span,
        },
        variant: Identifier {
            name: variant.to_string(),
            span,
        },
        payload,
        span,
    }
}

/// Replace a resolved name with the bare item name the flat namespace uses.
fn rewrite_bare(site: Site<'_>, item: &str) {
    match site {
        Site::TypeName(name) => name.name = item.to_string(),
        Site::Expr(expr) => {
            let replacement = match &*expr {
                Expr::Path { span, .. } => Expr::Identifier(Identifier {
                    name: item.to_string(),
                    span: *span,
                }),
                Expr::Identifier(ident) => Expr::Identifier(Identifier {
                    name: item.to_string(),
                    span: ident.span,
                }),
                // `geometry::Point { x: 1.0 }` parses as a struct-variant construction —
                // the brace form is indistinguishable from `Shape::Circle { .. }` until the
                // qualifier is known to name a module.
                Expr::EnumStructLiteral {
                    variant,
                    fields,
                    span,
                    ..
                } => Expr::StructLiteral {
                    name: Identifier {
                        name: item.to_string(),
                        span: variant.span,
                    },
                    fields: fields.clone(),
                    base: None,
                    span: *span,
                },
                _ => return,
            };
            *expr = replacement;
        }
        Site::Pattern(_) => {}
    }
}

/// Replace a qualified `module::Type::member` with the bare `Type::member` path.
fn rewrite_member(
    site: Site<'_>,
    ty: &str,
    member: &str,
    path: &str,
    from: &str,
) -> Result<(), ModuleError> {
    match site {
        // A type annotation names one type; `mod::Type::member` in type position has no
        // meaning to give it.
        Site::TypeName(_) => Err(ModuleError::PathTooDeep {
            path: path.to_string(),
            from: from.to_string(),
        }),
        Site::Pattern(_) => Ok(()),
        Site::Expr(expr) => {
            let replacement = match &*expr {
                Expr::Path {
                    type_name,
                    member: last,
                    span,
                    ..
                } => Expr::Path {
                    type_name: Identifier {
                        name: ty.to_string(),
                        span: type_name.span,
                    },
                    member: Identifier {
                        name: member.to_string(),
                        span: last.span,
                    },
                    span: *span,
                },
                Expr::EnumStructLiteral {
                    enum_name,
                    variant,
                    fields,
                    span,
                } => Expr::EnumStructLiteral {
                    enum_name: Identifier {
                        name: ty.to_string(),
                        span: enum_name.span,
                    },
                    variant: Identifier {
                        name: member.to_string(),
                        span: variant.span,
                    },
                    fields: fields.clone(),
                    span: *span,
                },
                _ => return Ok(()),
            };
            *expr = replacement;
            Ok(())
        }
    }
}
