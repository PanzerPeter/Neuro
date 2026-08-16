//! Rewriting: verify each qualifier against the module that owns the name, then erase it.
//!
//! After this pass no name carries a `::` module prefix, which is why semantic analysis,
//! HIR lowering, and both backends need to know nothing about modules.

use ast_types::Expr;
use shared_types::Identifier;

use crate::loader::{site_segments, ModuleGraph};
use crate::walk::{walk_items, Site};
use crate::ModuleError;

pub(crate) fn strip_qualifiers(graph: &mut ModuleGraph) -> Result<(), ModuleError> {
    for id in 0..graph.modules.len() {
        let mut items = std::mem::take(&mut graph.modules[id].items);
        let outcome = {
            let view = &*graph;
            let mut rewrite = |site: Site<'_>| resolve_site(view, id, site);
            walk_items(&mut items, &mut rewrite)
        };
        graph.modules[id].items = items;
        outcome?;
    }
    Ok(())
}

/// Resolve one qualified name written in module `from`.
///
/// A path whose head names no module is left exactly as it was: `Point::new` and
/// `Option::Some` are associated-function and enum-variant paths, and belong to the type
/// checker rather than here.
fn resolve_site(graph: &ModuleGraph, from: usize, site: Site<'_>) -> Result<(), ModuleError> {
    let segments = site_segments(&site);
    if segments.len() < 2 {
        return Ok(());
    }
    let chain = &segments[..segments.len() - 1];

    let mut module = None;
    let mut consumed = 0;
    if !graph.declares_type(from, &chain[0]) {
        for segment in chain {
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
            rewrite_bare(site, item)
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

/// Replace a fully-qualified name with the bare item name the flat namespace uses.
fn rewrite_bare(site: Site<'_>, item: &str) -> Result<(), ModuleError> {
    match site {
        Site::TypeName(name) => {
            name.name = item.to_string();
            Ok(())
        }
        Site::Expr(expr) => {
            let replacement = match &*expr {
                Expr::Path { span, .. } => Expr::Identifier(Identifier {
                    name: item.to_string(),
                    span: *span,
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
                _ => return Ok(()),
            };
            *expr = replacement;
            Ok(())
        }
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
