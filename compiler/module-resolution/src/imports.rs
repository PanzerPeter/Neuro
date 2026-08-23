//! Turn each module's `import` declarations into the name table its body is rewritten
//! against.
//!
//! An import binds names *locally*, which is the one place this slice is not flat: two
//! modules may bind the same name to different things. The table therefore lives per
//! module and the rewriting pass consults the table of the module it is walking.

use std::collections::HashMap;

use ast_types::{ImportDef, ImportSelection};

use crate::loader::{ModuleGraph, Reexport};
use crate::{ModuleError, PreludeVariant};

/// What one module's imports bind, ready for the rewriting pass.
#[derive(Default)]
pub(crate) struct ImportScope {
    /// A qualifier written in this module → the module it names.
    modules: HashMap<String, usize>,
    /// A bare name written in this module → the item name it stands for in the flat
    /// namespace. Identity for an unrenamed import; the original for an `as` rename.
    renames: HashMap<String, String>,
    /// A bare name → the `(enum, variant)` it stands for.
    variants: HashMap<String, (String, String)>,
    /// The subset of `renames` written with `export import`, which this module makes
    /// reachable through itself as well as binding locally.
    pub(crate) reexports: HashMap<String, Reexport>,
}

impl ImportScope {
    pub(crate) fn module(&self, name: &str) -> Option<usize> {
        self.modules.get(name).copied()
    }

    pub(crate) fn rename(&self, name: &str) -> Option<&str> {
        self.renames.get(name).map(String::as_str)
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

    /// Bind `name` to the flat-namespace item `item` declared in module `origin`.
    ///
    /// Under `export import` the binding is also recorded as a re-export: the name then
    /// reaches through this module too, which is the whole of what the form buys.
    fn bind_item(
        &mut self,
        name: &str,
        item: &str,
        origin: usize,
        from: &str,
        reexport: bool,
    ) -> Result<(), ModuleError> {
        self.reject_rebind(name, from)?;
        self.renames.insert(name.to_string(), item.to_string());
        if reexport {
            self.reexports.insert(
                name.to_string(),
                Reexport {
                    module: origin,
                    item: item.to_string(),
                },
            );
        }
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

    /// Bind a prelude variant, which no diagnostic can result from: the caller has already
    /// established that nothing else in this module claims the name.
    fn seed_variant(&mut self, owner: &str, variant: &str) {
        self.variants.insert(
            variant.to_string(),
            (owner.to_string(), variant.to_string()),
        );
    }

    /// Is `name` already bound in this module by an import?
    fn binds(&self, name: &str) -> bool {
        self.modules.contains_key(name)
            || self.renames.contains_key(name)
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
/// A module and a variant are both reached through something else — a deeper path, or the
/// enum that owns them — so neither has a name this module could stand in front of.
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

/// Build one scope per module, in module order, settling re-exports first.
pub(crate) fn resolve_imports(
    graph: &mut ModuleGraph,
    prelude: &[PreludeVariant],
) -> Result<Vec<ImportScope>, ModuleError> {
    // A re-exported name only becomes reachable once the module re-exporting it has been
    // resolved, and modules resolve in id order — so a chain of re-exports settles one link
    // per round. Errors are held back until the tables stop growing: an import that fails
    // this round may be exactly the one the next round makes resolvable.
    while install_reexports(graph, prelude) {}
    build_scopes(graph, Tolerance::Report, prelude)
}

/// Add the prelude's variant bindings to a module that did not opt out.
///
/// They are the weakest bindings in the language: a name this module declares, or already
/// imported, keeps its meaning, and neither case is an error — the prelude is a fallback,
/// so shadowing it is how a module overrides what the prelude offers.
fn seed_prelude(
    graph: &ModuleGraph,
    id: usize,
    prelude: &[PreludeVariant],
    scope: &mut ImportScope,
) {
    if graph.no_prelude(id) {
        return;
    }
    for entry in prelude {
        if graph.declares(id, &entry.variant) || scope.binds(&entry.variant) {
            continue;
        }
        scope.seed_variant(&entry.owner, &entry.variant);
    }
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
            let outcome = resolve_one(graph, id, import, prelude, &mut scope);
            if tolerance == Tolerance::Report {
                outcome?;
            }
        }
        seed_prelude(graph, id, prelude, &mut scope);
        scopes.push(scope);
    }
    Ok(scopes)
}

/// Copy one round of re-export tables onto the graph, reporting whether anything was new.
fn install_reexports(graph: &mut ModuleGraph, prelude: &[PreludeVariant]) -> bool {
    // A re-export names an item, never a prelude variant, so nothing here reads the
    // seeded bindings — but `resolve_one` consults the prelude to decide whether an
    // unresolvable head could be an enum, and must reach the same verdict either round.
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
    scope: &mut ImportScope,
) -> Result<(), ModuleError> {
    let segments: Vec<&str> = import.path.iter().map(|s| s.name.as_str()).collect();
    let (module, consumed) = walk_path(graph, from, &segments);
    let owner = graph.display(from).to_string();

    // The whole path is a module: `import math`, `import ./utils::io`.
    if let (Some(module), true) = (module, consumed == segments.len()) {
        return bind_from_module(graph, from, import, module, scope);
    }

    // All but the last segment is a module, so the last names something inside it:
    // `import math::sqrt as root`, `import geometry::Shape::{Circle}`.
    if let (Some(module), true) = (module, consumed + 1 == segments.len()) {
        return bind_from_item(graph, from, import, module, segments[consumed], scope);
    }

    // No segment named a module. A single segment can still be an enum whose variants are
    // being imported — but only if an enum by that name exists: a head that names neither
    // a module nor an enum used to be read as an enum regardless, which turned a typo, and
    // any path to an out-of-scope module, into a binding that quietly meant nothing.
    if consumed == 0 && segments.len() == 1 && names_an_enum(graph, prelude, segments[0]) {
        if let ImportSelection::List(names) = &import.selection {
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                scope.bind_variant(
                    &bound.name,
                    segments[0],
                    &entry.name.name,
                    &owner,
                    import.exported,
                )?;
            }
            return Ok(());
        }
    }

    Err(unresolved_head(graph, &segments, import, owner))
}

/// Could `head` name an enum reachable from anywhere in the program?
///
/// The prelude is checked separately from the graph because it is prepended *after* this
/// pass: `Option` is declared in no loaded module yet, so only the caller's prelude list
/// accounts for it.
fn names_an_enum(graph: &ModuleGraph, prelude: &[PreludeVariant], head: &str) -> bool {
    graph.declares_enum_anywhere(head) || prelude.iter().any(|entry| entry.owner == head)
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
    scope: &mut ImportScope,
) -> Result<(), ModuleError> {
    let owner = graph.display(from).to_string();
    let last = &import.path[import.path.len() - 1].name;
    match &import.selection {
        ImportSelection::Module => scope.bind_module(last, module, &owner, import.exported),
        ImportSelection::Alias(alias) => {
            scope.bind_module(&alias.name, module, &owner, import.exported)
        }
        ImportSelection::List(names) => {
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                let name = &entry.name.name;
                // A listed name is a child module (`import ./utils::{io}`) or an item the
                // module declares — the file system settles which.
                match graph.resolve_segment(from, Some(module), name) {
                    Some(child) => {
                        scope.bind_module(&bound.name, child, &owner, import.exported)?
                    }
                    None if graph.declares(module, name) => {
                        graph.check_visible(from, module, name)?;
                        let (origin, flat) = graph.flat_origin(module, name);
                        scope.bind_item(&bound.name, &flat, origin, &owner, import.exported)?
                    }
                    None => {
                        return Err(ModuleError::UndeclaredItem {
                            module: graph.path_of(module).to_string(),
                            item: name.clone(),
                            from: owner,
                        })
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
            let (origin, flat) = graph.flat_origin(module, item);
            scope.bind_item(bound, &flat, origin, &owner, import.exported)
        }
        // `import geometry::Shape::{Circle, Square}` — the tail is a type in the module,
        // so the listed names are its variants.
        ImportSelection::List(names) => {
            if !graph.declares_type(module, item) {
                return Err(undeclared());
            }
            // The listed names are variants of `item`, and a variant carries the enum's
            // visibility — so the enum itself is the only thing to gate.
            graph.check_visible(from, module, item)?;
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                scope.bind_variant(&bound.name, item, &entry.name.name, &owner, import.exported)?;
            }
            Ok(())
        }
    }
}

/// Consume as many leading segments as name modules, returning the deepest module reached
/// and how many segments it took.
fn walk_path(graph: &ModuleGraph, from: usize, segments: &[&str]) -> (Option<usize>, usize) {
    let mut module = None;
    let mut consumed = 0;
    for segment in segments {
        match graph.resolve_segment(from, module, segment) {
            Some(id) => {
                module = Some(id);
                consumed += 1;
            }
            None => break,
        }
    }
    (module, consumed)
}
