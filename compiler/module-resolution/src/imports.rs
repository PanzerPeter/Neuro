//! Turn each module's `import` declarations into the name table its body is rewritten
//! against.
//!
//! An import binds names *locally*, which is the one place this slice is not flat: two
//! modules may bind the same name to different things. The table therefore lives per
//! module and the rewriting pass consults the table of the module it is walking.

use std::collections::HashMap;

use ast_types::{ImportDef, ImportSelection};

use crate::loader::ModuleGraph;
use crate::ModuleError;

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

    fn bind_module(&mut self, name: &str, id: usize, from: &str) -> Result<(), ModuleError> {
        self.reject_rebind(name, from)?;
        self.modules.insert(name.to_string(), id);
        Ok(())
    }

    fn bind_item(&mut self, name: &str, item: &str, from: &str) -> Result<(), ModuleError> {
        self.reject_rebind(name, from)?;
        self.renames.insert(name.to_string(), item.to_string());
        Ok(())
    }

    fn bind_variant(
        &mut self,
        name: &str,
        owner: &str,
        variant: &str,
        from: &str,
    ) -> Result<(), ModuleError> {
        self.reject_rebind(name, from)?;
        self.variants
            .insert(name.to_string(), (owner.to_string(), variant.to_string()));
        Ok(())
    }

    /// One name may be bound once per module. Silently keeping the last import would make
    /// two imports of the same name read as working code that means only one of them.
    fn reject_rebind(&self, name: &str, from: &str) -> Result<(), ModuleError> {
        let bound = self.modules.contains_key(name)
            || self.renames.contains_key(name)
            || self.variants.contains_key(name);
        if bound {
            return Err(ModuleError::DuplicateImport {
                name: name.to_string(),
                from: from.to_string(),
            });
        }
        Ok(())
    }
}

/// Build one scope per module, in module order.
pub(crate) fn resolve_imports(graph: &ModuleGraph) -> Result<Vec<ImportScope>, ModuleError> {
    let mut scopes = Vec::with_capacity(graph.modules.len());
    for id in 0..graph.modules.len() {
        let mut scope = ImportScope::default();
        for import in &graph.modules[id].imports {
            resolve_one(graph, id, import, &mut scope)?;
        }
        scopes.push(scope);
    }
    Ok(scopes)
}

fn resolve_one(
    graph: &ModuleGraph,
    from: usize,
    import: &ImportDef,
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
    // being imported — including one from the prelude, which is prepended after this pass
    // and is therefore invisible here, so the enum itself is left for the type checker.
    if consumed == 0 && segments.len() == 1 {
        if let ImportSelection::List(names) = &import.selection {
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                scope.bind_variant(&bound.name, segments[0], &entry.name.name, &owner)?;
            }
            return Ok(());
        }
    }

    Err(ModuleError::UnknownModule {
        path: segments.join("::"),
        from: owner,
        head: segments[0].to_string(),
    })
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
        ImportSelection::Module => scope.bind_module(last, module, &owner),
        ImportSelection::Alias(alias) => scope.bind_module(&alias.name, module, &owner),
        ImportSelection::List(names) => {
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                let name = &entry.name.name;
                // A listed name is a child module (`import ./utils::{io}`) or an item the
                // module declares — the file system settles which.
                match graph.resolve_segment(from, Some(module), name) {
                    Some(child) => scope.bind_module(&bound.name, child, &owner)?,
                    None if graph.declares(module, name) => {
                        scope.bind_item(&bound.name, name, &owner)?
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
            let bound = match &import.selection {
                ImportSelection::Alias(alias) => &alias.name,
                _ => item,
            };
            scope.bind_item(bound, item, &owner)
        }
        // `import geometry::Shape::{Circle, Square}` — the tail is a type in the module,
        // so the listed names are its variants.
        ImportSelection::List(names) => {
            if !graph.declares_type(module, item) {
                return Err(undeclared());
            }
            for entry in names {
                let bound = entry.alias.as_ref().unwrap_or(&entry.name);
                scope.bind_variant(&bound.name, item, &entry.name.name, &owner)?;
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
