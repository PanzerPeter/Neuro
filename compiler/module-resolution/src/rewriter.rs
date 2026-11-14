//! Rewriting: resolve every name against the scope it is written in, and replace it with
//! the canonical name of the declaration it reaches.
//!
//! Every module is a namespace, and the canonical name says which one: `geometry::Point`
//! for a `Point` declared in `geometry.nr`, bare `Point` for one in the root file. After
//! this pass no name depends on an import or on the module it was written in, which is
//! why semantic analysis, HIR lowering, and both backends need to know nothing about
//! either: two modules' `helper`s reach them as two different names.

use ast_types::{EnumPatternPayload, Expr, Pattern};
use shared_types::Identifier;

use crate::imports::ImportScope;
use crate::loader::{ModuleGraph, site_segments, split_segments};
use crate::walk::{Site, walk_items};
use crate::{ModuleError, StandardLibrary};

/// The module every program starts from, whose names stay bare.
const ROOT: usize = 0;

/// The type name a path may open with inside an `impl` or a trait, which names no module.
const SELF_TYPE: &str = "Self";

pub(crate) fn resolve_names(
    graph: &mut ModuleGraph,
    scopes: &[ImportScope],
    std: &StandardLibrary,
) -> Result<(), ModuleError> {
    for id in 0..graph.modules.len() {
        let mut items = std::mem::take(&mut graph.modules[id].items);
        let outcome = {
            let resolver = Resolver {
                graph,
                scopes,
                std,
                from: id,
            };
            let mut rewrite = |site: Site<'_>| resolver.site(site);
            walk_items(&mut items, &mut rewrite)
        };
        graph.modules[id].items = items;
        outcome?;
    }
    Ok(())
}

/// What a bare name stands for in the scope it is written in.
enum Binding {
    /// A declaration, by its canonical name.
    Item(String),
    /// An enum variant: the enum's canonical name and the variant's own.
    Variant(String, String),
    Module(usize),
}

struct Resolver<'a> {
    graph: &'a ModuleGraph,
    scopes: &'a [ImportScope],
    std: &'a StandardLibrary,
    from: usize,
}

impl Resolver<'_> {
    /// What `name` stands for in module `from`: an import or a declaration of the module
    /// itself, then of each module enclosing it, then `std::prelude`'s declarations and its
    /// enums' variants. `None` for a name nothing in scope declares, which is left to the
    /// type checker: a builtin, or a type the compiler implements.
    fn lookup(&self, name: &str) -> Option<Binding> {
        let mut current = Some(self.from);
        while let Some(id) = current {
            let scope = &self.scopes[id];
            if let Some(module) = scope.module(name) {
                return Some(Binding::Module(module));
            }
            if let Some(target) = scope.item(name) {
                return Some(Binding::Item(
                    self.graph.canonical(target.module, &target.item),
                ));
            }
            if let Some((owner, variant)) = scope.variant(name) {
                return Some(Binding::Variant(owner.to_string(), variant.to_string()));
            }
            if self.graph.modules[id].declared.contains(name) {
                return Some(Binding::Item(self.graph.canonical(id, name)));
            }
            if let Some(block) = self.graph.inline_child(id, name) {
                return Some(Binding::Module(block));
            }
            current = self.graph.modules[id].parent;
        }
        let opted_out = self.graph.no_prelude(self.from);
        if let Some(prelude) = self.graph.prelude()
            && self.graph.declares(prelude, name)
            && (!opted_out || self.std.language_items.iter().any(|item| item == name))
        {
            return Some(Binding::Item(self.graph.canonical(prelude, name)));
        }
        if opted_out {
            return None;
        }
        self.std
            .variants
            .iter()
            .find(|entry| entry.variant == name)
            .map(|entry| Binding::Variant(entry.owner.clone(), entry.variant.clone()))
    }

    /// Reject a `std::prelude` name written bare in a file that opted out of the prelude.
    ///
    /// The declaration is in the program whatever the file says, so without this the name
    /// would still reach it through the type checker.
    fn reject_opted_out(&self, name: &str) -> Result<(), ModuleError> {
        if !self.graph.no_prelude(self.from) {
            return Ok(());
        }
        let path = if self.graph.prelude_declares(name) {
            format!("std::prelude::{name}")
        } else if let Some(entry) = self.std.variants.iter().find(|entry| entry.variant == name) {
            format!("std::prelude::{}::{name}", entry.owner)
        } else {
            return Ok(());
        };
        Err(ModuleError::OptedOutOfPrelude {
            name: name.to_string(),
            path,
            from: self.graph.display(self.from).to_string(),
        })
    }

    /// Reject a bare name nothing in scope accounts for when the root module declares it.
    ///
    /// The root's declarations keep their bare names, so such a name would otherwise land on
    /// the root's item from a module that never imported it, private or not.
    fn reject_root_leak(&self, name: &str) -> Result<(), ModuleError> {
        if self.graph.encloses(ROOT, self.from) || !self.graph.modules[ROOT].declared.contains(name)
        {
            return Ok(());
        }
        Err(ModuleError::NotInScope {
            name: name.to_string(),
            from: self.graph.display(self.from).to_string(),
            root: self.graph.display(ROOT).to_string(),
        })
    }

    fn site(&self, site: Site<'_>) -> Result<(), ModuleError> {
        if let Site::Pattern(pattern) = site {
            return self.pattern(pattern);
        }
        let segments = site_segments(&site);
        match segments.len() {
            0 => Ok(()),
            1 => self.bare(site, &segments[0]),
            _ => self.qualified(site, &segments),
        }
    }

    /// Resolve a name written without a qualifier.
    fn bare(&self, site: Site<'_>, name: &str) -> Result<(), ModuleError> {
        match self.lookup(name) {
            Some(Binding::Item(canonical)) => {
                rewrite_bare(site, &canonical);
                Ok(())
            }
            Some(Binding::Variant(owner, variant)) => {
                // A variant used as a value: `Some` becomes the `Option::Some` path the
                // checker reads, whether it is called (`Some(42)`) or standing alone.
                if let Site::Expr(expr) = site
                    && let Expr::Identifier(ident) = &*expr
                {
                    let span = ident.span;
                    *expr = Expr::Path {
                        type_name: Identifier { name: owner, span },
                        member: Identifier {
                            name: variant,
                            span,
                        },
                        span,
                    };
                }
                Ok(())
            }
            Some(Binding::Module(_)) => Ok(()),
            None => {
                self.reject_root_leak(name)?;
                self.reject_opted_out(name)
            }
        }
    }

    /// Resolve an `a::b(::c)` path. Its head is looked up like a bare name; a head nothing
    /// in scope declares starts at the program's root directory.
    fn qualified(&self, site: Site<'_>, segments: &[String]) -> Result<(), ModuleError> {
        let head = &segments[0];
        if head == SELF_TYPE {
            return Ok(());
        }
        let path = segments.join("::");
        let from = self.graph.display(self.from).to_string();

        let start = match self.lookup(head) {
            Some(Binding::Module(module)) => Some(module),
            // A type in scope opens a type path: `Point::new`, or `Pt::new` after
            // `import geometry::{Point as Pt}`.
            Some(Binding::Item(canonical)) => {
                if segments.len() > 2 {
                    return Err(ModuleError::UnknownModule {
                        path,
                        from,
                        head: head.clone(),
                    });
                }
                rename_head(site, &canonical);
                return Ok(());
            }
            Some(Binding::Variant(..)) => return Ok(()),
            None => self.graph.resolve_head(self.from, None, head),
        };

        let Some(mut module) = start else {
            self.reject_root_leak(head)?;
            self.reject_opted_out(head)?;
            if self.graph.has_inline_block_named(head) {
                return Err(ModuleError::UnreachableInlineModule {
                    path,
                    from,
                    head: head.clone(),
                });
            }
            // Two segments with no module prefix is the ordinary `Type::member` shape of a
            // type nothing here declares (`Option::Some`, `Vec::new`). Three or more can
            // only have been meant as a module path, so a silent pass-through would
            // surface as a baffling "unknown type `a::b`" later.
            if segments.len() > 2 {
                return Err(ModuleError::UnknownModule {
                    path,
                    from,
                    head: head.clone(),
                });
            }
            return Ok(());
        };
        self.graph.check_block_reachable(self.from, module)?;

        let mut consumed = 1;
        while consumed + 1 < segments.len() {
            match self.graph.resolve_child(module, &segments[consumed]) {
                Some(child) => {
                    self.graph.check_block_reachable(self.from, child)?;
                    module = child;
                    consumed += 1;
                }
                None => break,
            }
        }

        match &segments[consumed..] {
            [item] => {
                if !self.graph.declares(module, item) {
                    return Err(ModuleError::UndeclaredItem {
                        module: self.graph.path_of(module).to_string(),
                        item: item.clone(),
                        from,
                    });
                }
                self.graph.check_visible(self.from, module, item)?;
                rewrite_bare(site, &self.graph.canonical_through(module, item));
                Ok(())
            }
            [ty, member] => {
                if !self.graph.declares_type(module, ty) {
                    return Err(ModuleError::UndeclaredItem {
                        module: self.graph.path_of(module).to_string(),
                        item: ty.clone(),
                        from,
                    });
                }
                // The member is a method or a variant, which carry the type's visibility:
                // only the type itself is gated.
                self.graph.check_visible(self.from, module, ty)?;
                rewrite_member(
                    site,
                    &self.graph.canonical_through(module, ty),
                    member,
                    &path,
                    &from,
                )
            }
            _ => Err(ModuleError::PathTooDeep { path, from }),
        }
    }

    /// Resolve a pattern that names a variant.
    ///
    /// A payload-less variant (`None`) is written exactly like a binding, so the scope is
    /// the only thing that tells them apart; a payload-carrying one (`Some(n)`) is
    /// unambiguous and is an error when nothing in scope accounts for it. A qualified
    /// pattern's enum is a type name like any other.
    fn pattern(&self, pattern: &mut Pattern) -> Result<(), ModuleError> {
        match pattern {
            Pattern::Binding(ident) => {
                if let Some(Binding::Variant(owner, variant)) = self.lookup(&ident.name) {
                    *pattern = enum_pattern(owner, variant, EnumPatternPayload::Unit, ident.span);
                }
                Ok(())
            }
            Pattern::UnqualifiedEnum {
                variant,
                payload,
                span,
            } => {
                let Some(Binding::Variant(owner, name)) = self.lookup(&variant.name) else {
                    return Err(ModuleError::UnimportedVariant {
                        variant: variant.name.clone(),
                        from: self.graph.display(self.from).to_string(),
                    });
                };
                *pattern = enum_pattern(owner, name, payload.clone(), *span);
                Ok(())
            }
            Pattern::Enum { enum_name, .. } => self.site(Site::TypeName(enum_name)),
            Pattern::Wildcard(_)
            | Pattern::Literal(_, _)
            | Pattern::Range { .. }
            | Pattern::At { .. }
            | Pattern::Array { .. } => Ok(()),
        }
    }
}

fn enum_pattern(
    owner: String,
    variant: String,
    payload: EnumPatternPayload,
    span: shared_types::Span,
) -> Pattern {
    Pattern::Enum {
        enum_name: Identifier { name: owner, span },
        variant: Identifier {
            name: variant,
            span,
        },
        payload,
        span,
    }
}

/// Replace a resolved name with the canonical name of what it reaches.
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
                // `geometry::Point { x: 1.0 }` parses as a struct-variant construction:
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

/// Replace the head of a `Type::member` path with the canonical name of the type in scope.
fn rename_head(site: Site<'_>, item: &str) {
    match site {
        Site::Expr(Expr::Path { type_name, .. }) => type_name.name = item.to_string(),
        Site::Expr(Expr::EnumStructLiteral { enum_name, .. }) => enum_name.name = item.to_string(),
        // `Pt::Item` in a type position: only the head is a name in scope.
        Site::TypeName(name) => {
            let mut segments = split_segments(&name.name);
            segments[0] = item.to_string();
            name.name = segments.join("::");
        }
        Site::Expr(_) | Site::Pattern(_) => {}
    }
}

/// Replace a qualified `module::Type::member` with the canonical `Type::member` path.
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
