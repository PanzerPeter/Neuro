//! Discovery: which files belong to this program, and what each one declares.
//!
//! A module is pulled in only when a qualified path reaches into it, which is why a
//! directory holding a dozen unrelated single-file programs still compiles one at a time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ast_types::{ImportDef, Item, ModuleDef, ModuleId};

use crate::walk::{walk_items, Site};
use crate::ModuleError;

/// A module that was loaded, as reported back to the driver.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// Dotted-free module path as first reached, e.g. `utils::io`. Empty for the root.
    pub path: String,
    /// The file the module was read from.
    pub file: PathBuf,
}

pub(crate) struct Module {
    pub(crate) path: String,
    /// Short form used in diagnostics — the file relative to the root module's directory.
    pub(crate) display: String,
    pub(crate) file: PathBuf,
    /// Directory a path written *inside* this module resolves its first segment against.
    ref_dir: PathBuf,
    /// Directory this module's own children live in. `Some` only for a `mod.nr`: a leaf
    /// `math.nr` has no children, so `math::helper` must not reach `math`'s siblings.
    child_dir: Option<PathBuf>,
    /// Inline `module Name { ... }` blocks declared directly in this module, by name.
    /// Consulted before the file system, so a block always wins over a same-named file.
    inline_children: HashMap<String, usize>,
    pub(crate) items: Vec<Item>,
    /// The file's `import` declarations, lifted out of `items` at load time so nothing
    /// downstream has to know the item kind existed.
    pub(crate) imports: Vec<ImportDef>,
    pub(crate) declared: HashSet<String>,
    /// Names this module re-exports with `export import`, each mapped to where the
    /// declaration really lives. A re-export is reachable *through* this module without
    /// being declared in it.
    reexports: HashMap<String, Reexport>,
    /// The subset of `declared` written with `export`. Everything else is private to
    /// this module, so a qualified path or an import naming it is rejected.
    exported: HashSet<String>,
    /// Declared struct / enum / newtype / trait names. A locally declared type wins over a
    /// same-named file, so `Point::new` keeps meaning the associated function even when a
    /// `Point.nr` happens to sit next door.
    declared_types: HashSet<String>,
}

/// Where a re-exported name really comes from, already followed to the end of any chain
/// of re-exports so the flat namespace can be reached in one step.
#[derive(Clone)]
pub(crate) struct Reexport {
    pub(crate) module: usize,
    pub(crate) item: String,
}

pub(crate) struct ModuleGraph {
    pub(crate) modules: Vec<Module>,
    by_file: HashMap<PathBuf, usize>,
    root_dir: PathBuf,
}

impl ModuleGraph {
    /// Load `root` and every module reachable from it.
    pub(crate) fn load(
        root: &Path,
        parse_module: &dyn Fn(&str) -> Result<Vec<Item>, String>,
    ) -> Result<Self, ModuleError> {
        let file = canonical(root)?;
        let root_dir = parent_dir(&file);
        let mut graph = ModuleGraph {
            modules: Vec::new(),
            by_file: HashMap::new(),
            root_dir: root_dir.clone(),
        };
        graph.load_file(file, None, String::new(), parse_module)?;

        // Each module is visited once: `ensure_chain` walks a whole path in one call, so a
        // module loaded later cannot deepen an already-visited module's chain.
        let mut next = 0;
        while next < graph.modules.len() {
            let chains = graph.module_chains(next);
            for chain in chains {
                graph.ensure_chain(next, &chain, parse_module)?;
            }
            next += 1;
        }
        Ok(graph)
    }

    /// The module-path candidates written in module `id` — every qualified name minus its
    /// final segment, which is the item or type being named rather than a module, plus the
    /// path of every `import`, which is what makes an import pull its module in even when
    /// no qualified name reaches into it.
    fn module_chains(&mut self, id: usize) -> Vec<Vec<String>> {
        let mut items = std::mem::take(&mut self.modules[id].items);
        let declared_types = std::mem::take(&mut self.modules[id].declared_types);
        let mut chains: Vec<Vec<String>> = Vec::new();
        for import in &self.modules[id].imports {
            let path: Vec<String> = import.path.iter().map(|s| s.name.clone()).collect();
            // A `{...}` entry may name a child module rather than an item
            // (`import ./utils::{io}`), so each one extends the path it is loaded along.
            if let ast_types::ImportSelection::List(names) = &import.selection {
                for entry in names {
                    let mut deeper = path.clone();
                    deeper.push(entry.name.name.clone());
                    chains.push(deeper);
                }
            }
            chains.push(path);
        }
        let mut collect = |site: Site<'_>| -> Result<(), ModuleError> {
            let segments = site_segments(&site);
            if segments.len() < 2 {
                return Ok(());
            }
            let chain = &segments[..segments.len() - 1];
            if declared_types.contains(&chain[0]) {
                return Ok(());
            }
            if !chains.iter().any(|seen| seen == chain) {
                chains.push(chain.to_vec());
            }
            Ok(())
        };
        // The collector never fails; only the rewriting pass reports on what it finds.
        let _ = walk_items(&mut items, &mut collect);
        self.modules[id].items = items;
        self.modules[id].declared_types = declared_types;
        chains
    }

    /// Load every module along `chain`, starting from module `from`'s directory. A segment
    /// that names no file ends the descent: the remainder is a type path such as the
    /// `Point::new` of `math::Point::new`.
    fn ensure_chain(
        &mut self,
        from: usize,
        chain: &[String],
        parse_module: &dyn Fn(&str) -> Result<Vec<Item>, String>,
    ) -> Result<(), ModuleError> {
        let mut dir = self.modules[from].ref_dir.clone();
        let mut current: Option<usize> = None;
        let mut path = String::new();
        for segment in chain {
            // An inline block is already loaded and holds no file children, so reaching one
            // ends the descent before the file system is consulted at all.
            let holder = current.unwrap_or(from);
            if self.modules[holder].inline_children.contains_key(segment) {
                return Ok(());
            }
            let found = self.locate(&dir, segment, from)?;
            let Some((file, child_dir)) = found else {
                return Ok(());
            };
            path = if path.is_empty() {
                segment.clone()
            } else {
                format!("{}::{}", path, segment)
            };
            let id = self.load_file(file, child_dir, path.clone(), parse_module)?;
            current = Some(id);
            match &self.modules[id].child_dir {
                Some(next) => dir = next.clone(),
                None => return Ok(()),
            }
        }
        Ok(())
    }

    /// Map one path segment to a module file inside `dir`.
    fn locate(
        &self,
        dir: &Path,
        segment: &str,
        from: usize,
    ) -> Result<Option<(PathBuf, Option<PathBuf>)>, ModuleError> {
        let as_dir = dir.join(segment);
        let mod_file = as_dir.join("mod.nr");
        if mod_file.is_file() {
            return Ok(Some((mod_file, Some(as_dir))));
        }
        let leaf = dir.join(format!("{}.nr", segment));
        if leaf.is_file() {
            return Ok(Some((leaf, None)));
        }
        if as_dir.is_dir() {
            return Err(ModuleError::MissingModFile {
                name: segment.to_string(),
                from: self.modules[from].display.clone(),
                expected: display_path(&mod_file, &self.root_dir),
            });
        }
        Ok(None)
    }

    fn load_file(
        &mut self,
        file: PathBuf,
        child_dir: Option<PathBuf>,
        path: String,
        parse_module: &dyn Fn(&str) -> Result<Vec<Item>, String>,
    ) -> Result<usize, ModuleError> {
        let file = canonical(&file)?;
        if let Some(id) = self.by_file.get(&file) {
            return Ok(*id);
        }
        let display = display_path(&file, &self.root_dir);
        let source = std::fs::read_to_string(&file).map_err(|e| ModuleError::Read {
            path: display.clone(),
            message: e.to_string(),
        })?;
        let items = parse_module(&source).map_err(|message| ModuleError::Parse {
            path: display.clone(),
            message,
        })?;

        let ref_dir = parent_dir(&file);
        let child_dir = child_dir.map(|d| canonical(&d)).transpose()?;
        let id = self.push_module(path, display, file.clone(), ref_dir, child_dir, items)?;
        self.by_file.insert(file, id);
        Ok(id)
    }

    /// Register one module — a file, or an inline `module` block — along with every block
    /// it declares.
    ///
    /// An inline block is a module in every sense that matters here: its items are private
    /// unless exported, a qualified path reaches into it, and its declarations join the one
    /// flat namespace. Making it a graph module rather than a special case is what lets the
    /// visibility rule, the collision check, and the `ModuleId` stamp apply unchanged.
    fn push_module(
        &mut self,
        path: String,
        display: String,
        file: PathBuf,
        ref_dir: PathBuf,
        child_dir: Option<PathBuf>,
        mut items: Vec<Item>,
    ) -> Result<usize, ModuleError> {
        // Imports and inline blocks are consumed here rather than carried along: after
        // resolution the program is one flat item list, and neither has anything left to
        // say to a downstream pass.
        let mut imports = Vec::new();
        let mut blocks: Vec<ModuleDef> = Vec::new();
        items.retain(|item| match item {
            Item::Import(def) => {
                imports.push(def.clone());
                false
            }
            Item::Module(def) => {
                blocks.push(def.clone());
                false
            }
            _ => true,
        });

        let mut declared = HashSet::new();
        let mut exported = HashSet::new();
        let mut declared_types = HashSet::new();
        for item in &items {
            if let Some(name) = item_name(item) {
                declared.insert(name.to_string());
                if is_exported(item) {
                    exported.insert(name.to_string());
                }
            }
            if let Some(name) = type_name(item) {
                declared_types.insert(name.to_string());
            }
        }

        let id = self.modules.len();
        self.modules.push(Module {
            path,
            display,
            file,
            ref_dir,
            child_dir,
            inline_children: HashMap::new(),
            items,
            imports,
            declared,
            reexports: HashMap::new(),
            exported,
            declared_types,
        });

        for block in blocks {
            let name = block.name.name.clone();
            let parent = &self.modules[id];
            let child_path = if parent.path.is_empty() {
                name.clone()
            } else {
                format!("{}::{}", parent.path, name)
            };
            let child_display = format!("{}::{}", parent.display, name);
            let ref_dir = parent.ref_dir.clone();
            let file = parent.file.clone();
            // A block has no directory of its own, so it takes no file children: reaching
            // `outer::inner::item` through a block stops exactly where `math.nr` does.
            let child =
                self.push_module(child_path, child_display, file, ref_dir, None, block.items)?;
            if self.modules[id]
                .inline_children
                .insert(name.clone(), child)
                .is_some()
            {
                return Err(ModuleError::DuplicateInlineModule {
                    name,
                    from: self.modules[id].display.clone(),
                });
            }
        }
        Ok(id)
    }

    /// The module a path segment names: a child of `current`, or — for the first segment,
    /// where `current` is `None` — a module beside `from`. Only already-loaded modules are
    /// consulted; discovery has finished by the time this is asked.
    ///
    /// An inline block declared in the module doing the reaching wins over a same-named
    /// file, on the same principle that makes a locally declared type win: adding a file
    /// beside a module must never silently re-point a path that already resolved.
    pub(crate) fn resolve_segment(
        &self,
        from: usize,
        current: Option<usize>,
        segment: &str,
    ) -> Option<usize> {
        let holder = current.unwrap_or(from);
        if let Some(id) = self.modules[holder].inline_children.get(segment) {
            return Some(*id);
        }
        let dir = match current {
            None => &self.modules[from].ref_dir,
            Some(id) => self.modules[id].child_dir.as_ref()?,
        };
        let candidates = [
            dir.join(segment).join("mod.nr"),
            dir.join(format!("{}.nr", segment)),
        ];
        candidates
            .iter()
            .find_map(|candidate| canonical(candidate).ok().and_then(|c| self.by_file.get(&c)))
            .copied()
    }

    /// Does module `id` declare `name`, or re-export it?
    pub(crate) fn declares(&self, id: usize, name: &str) -> bool {
        self.modules[id].declared.contains(name) || self.modules[id].reexports.contains_key(name)
    }

    pub(crate) fn declares_type(&self, id: usize, name: &str) -> bool {
        let module = &self.modules[id];
        if module.declared_types.contains(name) {
            return true;
        }
        match module.reexports.get(name) {
            Some(target) => self.modules[target.module]
                .declared_types
                .contains(&target.item),
            None => false,
        }
    }

    /// Is `name` reachable from outside module `id`?
    ///
    /// A re-export is reachable by construction — making a name reachable through this
    /// module is the whole of what `export import` does.
    pub(crate) fn exports(&self, id: usize, name: &str) -> bool {
        self.modules[id].exported.contains(name) || self.modules[id].reexports.contains_key(name)
    }

    /// The module and name a reference to `name` in module `id` ultimately lands on.
    ///
    /// Re-export targets are stored already followed to the end, so one hop is enough.
    pub(crate) fn flat_origin(&self, id: usize, name: &str) -> (usize, String) {
        match self.modules[id].reexports.get(name) {
            Some(target) => (target.module, target.item.clone()),
            None => (id, name.to_string()),
        }
    }

    /// The name `name` carries in the flat namespace once module `id` is stripped off it.
    /// They differ only when a re-export renamed the declaration with `as`.
    pub(crate) fn flat_name<'a>(&'a self, id: usize, name: &'a str) -> &'a str {
        match self.modules[id].reexports.get(name) {
            Some(target) => &target.item,
            None => name,
        }
    }

    /// Record that module `id` re-exports `name`, reporting whether that was new.
    ///
    /// A chain of re-exports resolves one link per round, so the caller repeats until
    /// nothing is added.
    pub(crate) fn add_reexport(&mut self, id: usize, name: String, target: Reexport) -> bool {
        match self.modules[id].reexports.entry(name) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(target);
                true
            }
        }
    }

    /// Reject a reference from `from` into `module` naming a declaration that module
    /// keeps to itself. A module referring to its own name is always allowed, which is
    /// what makes a self-qualified path inside a module legal.
    pub(crate) fn check_visible(
        &self,
        from: usize,
        module: usize,
        name: &str,
    ) -> Result<(), ModuleError> {
        if from == module || self.exports(module, name) {
            return Ok(());
        }
        Err(ModuleError::PrivateItem {
            module: self.path_of(module).to_string(),
            item: name.to_string(),
            from: self.display(from).to_string(),
        })
    }

    /// The file a module is read from, as diagnostics name it.
    pub(crate) fn display(&self, id: usize) -> &str {
        &self.modules[id].display
    }

    /// The module path a module was first reached by. The root has none, so it answers
    /// with its file instead.
    pub(crate) fn path_of(&self, id: usize) -> &str {
        let module = &self.modules[id];
        if module.path.is_empty() {
            &module.display
        } else {
            &module.path
        }
    }

    /// Reject the same name declared by two modules. Items share one namespace this phase,
    /// so the alternative is one declaration silently winning.
    pub(crate) fn check_name_collisions(&self) -> Result<(), ModuleError> {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for (id, module) in self.modules.iter().enumerate() {
            let mut names: Vec<&str> = module.declared.iter().map(String::as_str).collect();
            names.sort_unstable();
            for name in names {
                if let Some(first) = seen.insert(name, id) {
                    return Err(ModuleError::DuplicateItem {
                        name: name.to_string(),
                        first: self.modules[first].display.clone(),
                        second: module.display.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn into_program(self) -> crate::ResolvedProgram {
        let mut items = Vec::new();
        let mut modules = Vec::new();
        for (id, module) in self.modules.into_iter().enumerate() {
            modules.push(ResolvedModule {
                path: module.path,
                file: module.file,
            });
            // The merge is flat, so this stamp is the only trace of which file a
            // declaration came from. Field visibility needs types and is therefore
            // settled by the type checker, which has nothing else to read it from.
            for mut item in module.items {
                stamp_module(&mut item, id as ModuleId);
                items.push(item);
            }
        }
        crate::ResolvedProgram { items, modules }
    }
}

/// The `::`-separated segments a qualified site names, item or type name included.
///
/// A site that names no path — a pattern, a non-path expression — yields nothing, and a
/// bare name yields the one segment an import table is keyed on.
pub(crate) fn site_segments(site: &Site<'_>) -> Vec<String> {
    match site {
        Site::TypeName(name) => split_segments(&name.name),
        Site::Pattern(_) => Vec::new(),
        Site::Expr(expr) => match expr {
            ast_types::Expr::Identifier(name) => vec![name.name.clone()],
            ast_types::Expr::Path {
                type_name, member, ..
            } => {
                let mut segments = split_segments(&type_name.name);
                segments.push(member.name.clone());
                segments
            }
            ast_types::Expr::EnumStructLiteral {
                enum_name, variant, ..
            } => {
                let mut segments = split_segments(&enum_name.name);
                segments.push(variant.name.clone());
                segments
            }
            _ => Vec::new(),
        },
    }
}

fn split_segments(name: &str) -> Vec<String> {
    name.split("::").map(str::to_string).collect()
}

fn parent_dir(file: &Path) -> PathBuf {
    file.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Canonicalize so the same file reached by two different paths is one module.
fn canonical(path: &Path) -> Result<PathBuf, ModuleError> {
    std::fs::canonicalize(path).map_err(|e| ModuleError::Read {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

fn display_path(file: &Path, root_dir: &Path) -> String {
    file.strip_prefix(root_dir)
        .unwrap_or(file)
        .display()
        .to_string()
        .replace('\\', "/")
}

/// Record which module a declaration was loaded from, on the item kinds whose bodies
/// or fields the visibility rules are checked against.
fn stamp_module(item: &mut Item, module: ModuleId) {
    match item {
        Item::Function(def) => def.module = module,
        Item::Struct(def) => def.module = module,
        Item::Impl(def) => def.module = module,
        Item::Const(def) => def.module = module,
        // An enum, a newtype, and a trait declaration hold no field access to check:
        // an enum payload is reached through a pattern, which names the variant rather
        // than a field, and a trait's default bodies are checked through the impl copies
        // the parser injects.
        // An inline block is lifted into a module of its own before this runs, so none
        // reaches the merge.
        Item::Enum(_) | Item::Newtype(_) | Item::Trait(_) | Item::Import(_) | Item::Module(_) => {}
    }
}

/// Whether a declaration opted into module-public visibility with `export`.
fn is_exported(item: &Item) -> bool {
    match item {
        Item::Function(def) => def.exported,
        Item::Struct(def) => def.exported,
        Item::Enum(def) => def.exported,
        Item::Trait(def) => def.exported,
        Item::Const(def) => def.exported,
        Item::Newtype(def) => def.exported,
        Item::Impl(_) | Item::Import(_) | Item::Module(_) => false,
    }
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(def) => Some(&def.name.name),
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Const(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Impl(_) | Item::Import(_) | Item::Module(_) => None,
    }
}

fn type_name(item: &Item) -> Option<&str> {
    match item {
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Function(_) | Item::Const(_) | Item::Impl(_) | Item::Import(_) | Item::Module(_) => {
            None
        }
    }
}
