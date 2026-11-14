//! Turn each module's `import` declarations into the name table its body is rewritten
//! against.
//!
//! An import binds names *locally*: two modules may bind the same name to different
//! things. The table therefore lives per module and the rewriting pass consults the table
//! of the module it is walking, then the tables of the modules enclosing it.

use std::collections::HashMap;

use ast_types::{ImportDef, ImportSelection};

use crate::loader::{ModuleGraph, Reexport};
use crate::{ModuleError, PreludeVariant};

/// What one module's imports bind, ready for the rewriting pass.
#[derive(Default)]
pub(crate) struct ImportScope {
    /// A qualifier written in this module → the module it names.
    modules: HashMap<String, usize>,
    /// A bare name written in this module → the declaration it stands for.
    items: HashMap<String, Reexport>,
    /// A bare name → the `(enum, variant)` it stands for, the enum by its canonical name.
    variants: HashMap<String, (String, String)>,
    /// The subset of `items` written with `export import`, which this module makes
    /// reachable through itself as well as binding locally.
    pub(crate) reexports: HashMap<String, Reexport>,
}

impl ImportScope {
    pub(crate) fn module(&self, name: &str) -> Option<usize> {
        self.modules.get(name).copied()
    }

    pub(crate) fn item(&self, name: &str) -> Option<&Reexport> {
        self.items.get(name)
    }

    pub(crate) fn variant(&self, name: &str) -> Option<(&str, &str)> {
        self.variants
            .get(name)
            .map(|(owner, variant)| (owner.as_str(), variant.as_str()))
    }

    fn bind_module(
        &mut self,
        name: &str,
        id: usize,
        from: &str,
        reexport: bool,
    ) -> Result<(), ModuleError> {
        reject_reexport(reexport, name, "a module", from)?;
        self.reject_rebind(name, from)?;
        self.modules.insert(name.to_string(), id);
        Ok(())
    }

    /// Bind `name` to the declaration `target`.
    ///
    /// Under `export import` the binding is also recorded as a re-export: the name then
    /// reaches through this module too, which is the whole of what the form buys.
    fn bind_item(
        &mut self,
        name: &str,
        target: Reexport,
        from: &str,
        reexport: bool,
    ) -> Result<(), ModuleError> {
        self.reject_rebind(name, from)?;
        if reexport {
            self.reexports.insert(name.to_string(), target.clone());
        }
        self.items.insert(name.to_string(), target);
        Ok(())
    }

    fn bind_variant(
        &mut self,
        name: &str,
        owner: &str,
        variant: &str,
        from: &str,
        reexport: bool,
    ) -> Result<(), ModuleError> {
        reject_reexport(reexport, name, "an enum variant", from)?;
        self.reject_rebind(name, from)?;
        self.variants
            .insert(name.to_string(), (owner.to_string(), variant.to_string()));
        Ok(())
    }

    /// Is `name` already bound in this module by an import?
    fn binds(&self, name: &str) -> bool {
        self.modules.contains_key(name)
            || self.items.contains_key(name)
            || self.variants.contains_key(name)
    }

    /// One name may be bound once per module. Silently keeping the last import would make
    /// two imports of the same name read as working code that means only one of them.
    fn reject_rebind(&self, name: &str, from: &str) -> Result<(), ModuleError> {
        if self.binds(name) {
            return Err(ModuleError::DuplicateImport {
                name: name.to_string(),
                from: from.to_string(),
            });
        }
        Ok(())
    }
}

/// Reject an `export import` whose bound name is not an item.
///
/// A module and a variant are both reached through something else (a deeper path, or the
/// enum that owns them), so neither has a name this module could stand in front of.
fn reject_reexport(reexport: bool, name: &str, what: &str, from: &str) -> Result<(), ModuleError> {
    if !reexport {
        return Ok(());
    }
    Err(ModuleError::ExportImportNotItem {
        name: name.to_string(),
        what: what.to_string(),
        from: from.to_string(),
    })
}

/// Reject an import binding a name the module declares itself: the declaration and the
/// import would compete for one spelling, and neither silently winning is safe.
fn reject_declared(graph: &ModuleGraph, from: usize, name: &str) -> Result<(), ModuleError> {
    if graph.declares_own(from, name) {
        return Err(ModuleError::ImportShadowsDeclaration {
            name: name.to_string(),
            from: graph.display(from).to_string(),
        });
    }
    Ok(())
}

/// Build one scope per module, in module order, settling re-exports first.
pub(crate) fn resolve_imports(
    graph: &mut ModuleGraph,
    prelude: &[PreludeVariant],
) -> Result<Vec<ImportScope>, ModuleError> {
    // A re-exported name only becomes reachable once the module re-exporting it has been
    // resolved, and modules resolve in id order, so a chain of re-exports settles one link
    // per round. Errors are held back until the tables stop growing: an import that fails
    // this round may be exactly the one the next round makes resolvable.
    while install_reexports(graph, prelude) {}
    build_scopes(graph, Tolerance::Report, prelude)
}

/// Whether an import that cannot be resolved ends the pass or is passed over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tolerance {
    Report,
    Skip,
}

fn build_scopes(
    graph: &ModuleGraph,
    tolerance: Tolerance,
    prelude: &[PreludeVariant],
) -> Result<Vec<ImportScope>, ModuleError> {
    let mut scopes = Vec::with_capacity(graph.modules.len());
    for id in 0..graph.modules.len() {
        let mut scope = ImportScope::default();
        for import in &graph.modules[id].imports {
            let outcome = resolve_one(graph, id, import, prelude, &scopes, &mut scope);
            if tolerance == Tolerance::Report {
                outcome?;
            }
        }
        scopes.push(scope);
    }
    Ok(scopes)
}

/// Copy one round of re-export tables onto the graph, reporting whether anything was new.
fn install_reexports(graph: &mut ModuleGraph, prelude: &[PreludeVariant]) -> bool {
    let Ok(scopes) = build_scopes(graph, Tolerance::Skip, prelude) else {
        return false;
    };
    let mut changed = false;
    for (id, scope) in scopes.into_iter().enumerate() {
        for (name, target) in scope.reexports {
            changed |= graph.add_reexport(id, name, target);
        }
    }
    changed
}

fn resolve_one(
    graph: &ModuleGraph,
    from: usize,
    import: &ImportDef,
    prelude: &[PreludeVariant],
    enclosing: &[ImportScope],
    scope: &mut ImportScope,
) -> Result<(), ModuleError> {
    let segments: Vec<&str> = import.path.iter().map(|s| s.name.as_str()).collect();
    let anchor = import.relative.then_some(import.parents);
    let (module, consumed) = walk_path(graph, from, anchor, &segments)?;
    let owner = graph.display(from).to_string();

    // The whole path is a module: `import math`, `import ./utils::io`.
    if let (Some(module), true) = (module, consumed == segments.len()) {
        return bind_from_module(graph, from, import, module, prelude, scope);
    }

    // All but the last segment is a module, so the last names something inside it:
    // `import math::sqrt as root`, `import geometry::Shape::{Circle}`.
    if let (Some(module), true) = (module, consumed + 1 == segments.len()) {
        return bind_from_item(graph, from, import, module, segments[consumed], scope);
    }

    // No segment named a module. The head can still be an enum in scope whose variants
    // are being imported, as a list (`import Shape::{Circle}`) or one at a time
    // (`import Shape::Circle`), but only if an enum by that name is in scope: a head that
    // names neither a module nor an enum used to be read as an enum regardless, which
    // turned a typo into a binding that quietly meant nothing.
    if consumed == 0 && !import.relative {
        let enum_owner = enum_in_scope(graph, from, enclosing, scope, segments[0]);
        match (&enum_owner, segments.len(), &import.selection) {
            (Some(enum_owner), 1, ImportSelection::List(names)) => {
                for entry in names {
                    let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                    reject_declared(graph, from, &bound.name)?;
                    scope.bind_variant(
                        &bound.name,
                        enum_owner,
                        &entry.name.name,
                        &owner,
                        import.exported,
                    )?;
                }
                return Ok(());
            }
            (Some(enum_owner), 2, ImportSelection::Module | ImportSelection::Alias(_)) => {
                let bound = match &import.selection {
                    ImportSelection::Alias(alias) => alias.name.as_str(),
                    _ => segments[1],
                };
                reject_declared(graph, from, bound)?;
                return scope.bind_variant(bound, enum_owner, segments[1], &owner, import.exported);
            }
            _ => {}
        }
    }

    Err(unresolved_head(graph, &segments, import, owner))
}

/// The canonical name of the enum `head` names in module `from`: one it or an enclosing
/// module declares, one this module already imported, one an enclosing module imported,
/// or, unless the module opted out, one of `std::prelude`'s.
///
/// `enclosing` holds the finished scope of every module numbered below `from`. A block is
/// numbered after the module that declares it, so each enclosing module's is among them.
fn enum_in_scope(
    graph: &ModuleGraph,
    from: usize,
    enclosing: &[ImportScope],
    scope: &ImportScope,
    head: &str,
) -> Option<String> {
    let mut current = Some(from);
    while let Some(id) = current {
        if graph.modules[id].declared.contains(head) && graph.declares_enum(id, head) {
            return Some(graph.canonical(id, head));
        }
        current = graph.modules[id].parent;
    }
    let mut visible = Some(scope);
    let mut current = from;
    while let Some(imports) = visible {
        if let Some(target) = imports.item(head)
            && graph.declares_enum(target.module, &target.item)
        {
            return Some(graph.canonical(target.module, &target.item));
        }
        visible = graph.modules[current].parent.and_then(|parent| {
            current = parent;
            enclosing.get(parent)
        });
    }
    let prelude = graph.prelude().filter(|_| !graph.no_prelude(from))?;
    (graph.declares(prelude, head) && graph.declares_enum(prelude, head))
        .then(|| graph.canonical(prelude, head))
}

/// The error for an import path that reached no module.
///
/// Three readings, narrowest first: a module by that name exists but is out of scope; the
/// import was shaped like a variant list, so the enum reading was tried and also failed;
/// or the head simply named no module.
fn unresolved_head(
    graph: &ModuleGraph,
    segments: &[&str],
    import: &ImportDef,
    owner: String,
) -> ModuleError {
    let head = segments[0].to_string();
    let path = segments.join("::");
    if graph.has_inline_block_named(&head) {
        return ModuleError::UnreachableInlineModule {
            path,
            from: owner,
            head,
        };
    }
    let variant_list = segments.len() == 1 && matches!(import.selection, ImportSelection::List(_));
    if variant_list {
        return ModuleError::UnknownImportHead {
            path,
            from: owner,
            head,
        };
    }
    ModuleError::UnknownModule {
        path,
        from: owner,
        head,
    }
}

/// Bind the selection of an import whose whole path resolved to `module`.
fn bind_from_module(
    graph: &ModuleGraph,
    from: usize,
    import: &ImportDef,
    module: usize,
    prelude: &[PreludeVariant],
    scope: &mut ImportScope,
) -> Result<(), ModuleError> {
    let owner = graph.display(from).to_string();
    let last = &import.path[import.path.len() - 1].name;
    match &import.selection {
        ImportSelection::Module => {
            reject_declared(graph, from, last)?;
            scope.bind_module(last, module, &owner, import.exported)
        }
        ImportSelection::Alias(alias) => {
            reject_declared(graph, from, &alias.name)?;
            scope.bind_module(&alias.name, module, &owner, import.exported)
        }
        ImportSelection::List(names) => {
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                let name = &entry.name.name;
                reject_declared(graph, from, &bound.name)?;
                // A listed name is a child module (`import ./utils::{io}`) or an item the
                // module declares: the file system settles which.
                match graph.resolve_child(module, name) {
                    Some(child) => {
                        graph.check_block_reachable(from, child)?;
                        scope.bind_module(&bound.name, child, &owner, import.exported)?
                    }
                    None if graph.declares(module, name) => {
                        graph.check_visible(from, module, name)?;
                        let (origin, item) = graph.flat_origin(module, name);
                        scope.bind_item(
                            &bound.name,
                            Reexport {
                                module: origin,
                                item,
                            },
                            &owner,
                            import.exported,
                        )?
                    }
                    // The language writes the prelude as `import std::prelude::{Option, Some, ...}`,
                    // so its variants are importable from it by their bare names.
                    None if Some(module) == graph.prelude()
                        && let Some(entry) = prelude.iter().find(|v| &v.variant == name) =>
                    {
                        scope.bind_variant(
                            &bound.name,
                            &entry.owner,
                            &entry.variant,
                            &owner,
                            import.exported,
                        )?
                    }
                    None => {
                        return Err(ModuleError::UndeclaredItem {
                            module: graph.path_of(module).to_string(),
                            item: name.clone(),
                            from: owner,
                        });
                    }
                }
            }
            Ok(())
        }
    }
}

/// Bind the selection of an import whose path named `item` inside `module`.
fn bind_from_item(
    graph: &ModuleGraph,
    from: usize,
    import: &ImportDef,
    module: usize,
    item: &str,
    scope: &mut ImportScope,
) -> Result<(), ModuleError> {
    let owner = graph.display(from).to_string();
    let undeclared = || ModuleError::UndeclaredItem {
        module: graph.path_of(module).to_string(),
        item: item.to_string(),
        from: owner.clone(),
    };

    match &import.selection {
        ImportSelection::Module | ImportSelection::Alias(_) => {
            if !graph.declares(module, item) {
                return Err(undeclared());
            }
            graph.check_visible(from, module, item)?;
            let bound = match &import.selection {
                ImportSelection::Alias(alias) => &alias.name,
                _ => item,
            };
            reject_declared(graph, from, bound)?;
            let (origin, original) = graph.flat_origin(module, item);
            scope.bind_item(
                bound,
                Reexport {
                    module: origin,
                    item: original,
                },
                &owner,
                import.exported,
            )
        }
        // `import geometry::Shape::{Circle, Square}`: the tail is a type in the module,
        // so the listed names are its variants.
        ImportSelection::List(names) => {
            if !graph.declares_type(module, item) {
                return Err(undeclared());
            }
            // The listed names are variants of `item`, and a variant carries the enum's
            // visibility, so the enum itself is the only thing to gate.
            graph.check_visible(from, module, item)?;
            let enum_owner = graph.canonical_through(module, item);
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                reject_declared(graph, from, &bound.name)?;
                scope.bind_variant(
                    &bound.name,
                    &enum_owner,
                    &entry.name.name,
                    &owner,
                    import.exported,
                )?;
            }
            Ok(())
        }
    }
}

/// Consume as many leading segments as name modules, returning the deepest module reached
/// and how many segments it took. A head the importing module has in scope as a type names
/// that type, not a same-named file, so it consumes nothing.
fn walk_path(
    graph: &ModuleGraph,
    from: usize,
    anchor: Option<u32>,
    segments: &[&str],
) -> Result<(Option<usize>, usize), ModuleError> {
    let Some(head) = segments.first() else {
        return Ok((None, 0));
    };
    if anchor.is_none() && graph.lexical_type(from, head) {
        return Ok((None, 0));
    }
    let Some(mut module) = graph.resolve_head(from, anchor, head) else {
        return Ok((None, 0));
    };
    graph.check_block_reachable(from, module)?;
    let mut consumed = 1;
    for segment in &segments[1..] {
        match graph.resolve_child(module, segment) {
            Some(child) => {
                graph.check_block_reachable(from, child)?;
                module = child;
                consumed += 1;
            }
            None => break,
        }
    }
    Ok((Some(module), consumed))
}
