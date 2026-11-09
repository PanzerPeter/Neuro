//! Discovery: which files belong to this program, and what each one declares.
//!
//! A module is pulled in only when a qualified path reaches into it, which is why a
//! directory holding a dozen unrelated single-file programs still compiles one at a time.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use ast_types::{ImportDef, ImportSelection, Item, ModuleDef, ModuleId};

use crate::walk::{Site, walk_items};
use crate::{ModuleError, ParseModule};

/// U+FEFF, which some editors write at the start of a UTF-8 file.
const BYTE_ORDER_MARK: char = '\u{feff}';

/// The type name a path may open with inside an `impl` or a trait, which names no module.
const SELF_TYPE: &str = "Self";

/// A module that was loaded, as reported back to the driver.
#[derive(Debug, Clone)]
pub struct ResolvedModule {
    /// The module's path from the program root, e.g. `utils::io`. Empty for the root.
    pub path: String,
    /// The file the module was read from.
    pub file: PathBuf,
}

pub(crate) struct Module {
    /// The module's path from the program root (`utils::io`, `counter::limits`), which
    /// prefixes every name it declares once resolution is done. Empty for the root, whose
    /// names stay bare.
    pub(crate) path: String,
    /// Short form used in diagnostics: the file relative to the root module's directory.
    pub(crate) display: String,
    pub(crate) file: PathBuf,
    /// Directory a `./` path written inside this module starts from.
    ref_dir: PathBuf,
    /// Directory this module's own children live in. `Some` only for a `mod.nr`: a leaf
    /// `math.nr` has no children, so `math::helper` must not reach `math`'s siblings.
    child_dir: Option<PathBuf>,
    /// The module an inline block is written in. A block sees its enclosing module's
    /// names, so name lookup climbs this chain; a file has no enclosing module.
    pub(crate) parent: Option<usize>,
    /// Inline `module Name { ... }` blocks declared directly in this module, by name.
    /// Consulted before the file system, so a block always wins over a same-named file.
    inline_children: HashMap<String, usize>,
    /// The subset of `inline_children` written `export module`.
    exported_blocks: HashSet<String>,
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
    /// The enum subset of `declared_types`. An import whose path resolves to no module is
    /// only legitimately an enum's variant list, so settling that needs enums alone.
    declared_enums: HashSet<String>,
    /// Whether the file this module came from opted out of the implicit prelude. An inline
    /// block inherits its file's answer: `@no_prelude` marks a file, and a block is part of
    /// one.
    pub(crate) no_prelude: bool,
}

/// Where a module was found, as the graph needs it to register the module and resolve the
/// paths written inside it.
struct ModuleOrigin {
    path: String,
    display: String,
    file: PathBuf,
    ref_dir: PathBuf,
    child_dir: Option<PathBuf>,
    parent: Option<usize>,
}

/// Where a declaration really lives: the module that declares it and the name it has
/// there. A re-export is stored already followed to the end of any chain, so the
/// declaration is reached in one step.
#[derive(Clone)]
pub(crate) struct Reexport {
    pub(crate) module: usize,
    pub(crate) item: String,
}

/// A module path written somewhere, as discovery follows it.
#[derive(Clone, PartialEq)]
struct Chain {
    /// `None` for a path that starts at the program's root directory; `Some(n)` for one
    /// that starts at the writing file's own directory, climbed `n` levels (`./` is 0).
    anchor: Option<u32>,
    segments: Vec<String>,
}

pub(crate) struct ModuleGraph {
    pub(crate) modules: Vec<Module>,
    by_file: HashMap<PathBuf, usize>,
    root_dir: PathBuf,
}

impl ModuleGraph {
    /// Load `root` and every module reachable from it.
    pub(crate) fn load(root: &Path, parse_module: &ParseModule<'_>) -> Result<Self, ModuleError> {
        let file = canonical(root)?;
        let root_dir = parent_dir(&file);
        let mut graph = ModuleGraph {
            modules: Vec::new(),
            by_file: HashMap::new(),
            root_dir,
        };
        graph.load_file(file, None, parse_module)?;

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
        graph.check_module_paths()?;
        Ok(graph)
    }

    /// The paths an import in module `id` binds a name to, inner modules first, so a block
    /// reads its enclosing module's imports as its own scope does.
    fn import_chains(&self, id: usize) -> HashMap<String, Chain> {
        let mut bound: HashMap<String, Chain> = HashMap::new();
        let mut current = Some(id);
        while let Some(module) = current {
            for import in &self.modules[module].imports {
                let anchor = import.relative.then_some(import.parents);
                let path: Vec<String> = import.path.iter().map(|s| s.name.clone()).collect();
                let mut bind = |name: &str, segments: Vec<String>| {
                    bound
                        .entry(name.to_string())
                        .or_insert(Chain { anchor, segments });
                };
                match &import.selection {
                    ImportSelection::Module => {
                        if let Some(last) = path.last() {
                            bind(last, path.clone());
                        }
                    }
                    ImportSelection::Alias(alias) => bind(&alias.name, path.clone()),
                    ImportSelection::List(names) => {
                        for entry in names {
                            let mut deeper = path.clone();
                            deeper.push(entry.name.name.clone());
                            bind(&entry.alias.as_ref().unwrap_or(&entry.name).name, deeper);
                        }
                    }
                }
            }
            current = self.modules[module].parent;
        }
        bound
    }

    /// The module-path candidates written in module `id`: every qualified name minus its
    /// final segment, which is the item or type being named rather than a module, plus the
    /// path of every `import`, which is what makes an import pull its module in even when
    /// no qualified name reaches into it.
    fn module_chains(&mut self, id: usize) -> Vec<Chain> {
        let mut chains: Vec<Chain> = Vec::new();
        for import in &self.modules[id].imports {
            let anchor = import.relative.then_some(import.parents);
            let path: Vec<String> = import.path.iter().map(|s| s.name.clone()).collect();
            // A `{...}` entry may name a child module rather than an item
            // (`import ./utils::{io}`), so each one extends the path it is loaded along.
            if let ImportSelection::List(names) = &import.selection {
                for entry in names {
                    let mut deeper = path.clone();
                    deeper.push(entry.name.name.clone());
                    chains.push(Chain {
                        anchor,
                        segments: deeper,
                    });
                }
            }
            chains.push(Chain {
                anchor,
                segments: path,
            });
        }
        // A qualified name headed by an imported name is loaded along the import's path:
        // the rewriting pass reads such a head through the import, so a same-named file at
        // the root is not what it names and must not be dragged into the build.
        let bound = self.import_chains(id);
        let mut items = std::mem::take(&mut self.modules[id].items);
        let view = &*self;
        let mut collect = |site: Site<'_>| -> Result<(), ModuleError> {
            let segments = site_segments(&site);
            if segments.len() < 2 {
                return Ok(());
            }
            let written = &segments[..segments.len() - 1];
            let head = &written[0];
            if head == SELF_TYPE
                || view.lexical_type(id, head)
                || view.lexical_block(id, head).is_some()
            {
                return Ok(());
            }
            let chain = match bound.get(head) {
                Some(import) => Chain {
                    anchor: import.anchor,
                    segments: import
                        .segments
                        .iter()
                        .chain(&written[1..])
                        .cloned()
                        .collect(),
                },
                None => Chain {
                    anchor: None,
                    segments: written.to_vec(),
                },
            };
            if !chains.contains(&chain) {
                chains.push(chain);
            }
            Ok(())
        };
        // The collector never fails; only the rewriting pass reports on what it finds.
        let _ = walk_items(&mut items, &mut collect);
        self.modules[id].items = items;
        chains
    }

    /// Load every module along `chain`, written in module `from`. A segment that names no
    /// file ends the descent: the remainder is a type path such as the `Point::new` of
    /// `math::Point::new`, or an enum's variant list.
    fn ensure_chain(
        &mut self,
        from: usize,
        chain: &Chain,
        parse_module: &ParseModule<'_>,
    ) -> Result<(), ModuleError> {
        let Some(head) = chain.segments.first() else {
            return Ok(());
        };
        let mut dir = match chain.anchor {
            // An inline block is already loaded and holds no file children, so reaching
            // one ends the descent before the file system is consulted at all. A locally
            // declared type wins over a same-named file for the same reason.
            None if self.lexical_block(from, head).is_some() || self.lexical_type(from, head) => {
                return Ok(());
            }
            None => self.root_dir.clone(),
            Some(levels) => climb(&self.modules[from].ref_dir, levels),
        };
        let mut current: Option<usize> = None;
        for segment in &chain.segments {
            if let Some(id) = current
                && self.modules[id].inline_children.contains_key(segment)
            {
                return Ok(());
            }
            let Some((file, child_dir)) = self.locate(&dir, segment, from)? else {
                return Ok(());
            };
            let id = self.load_file(file, child_dir, parse_module)?;
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
        parse_module: &ParseModule<'_>,
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
        // Some Windows editors open a file with a byte order mark; it is encoding
        // metadata, not source, and spans must index the text after it.
        let source = source.strip_prefix(BYTE_ORDER_MARK).unwrap_or(&source);
        let items = parse_module(source, &display).map_err(|message| ModuleError::Parse {
            path: display.clone(),
            message,
        })?;

        // The root's names stay bare; every other file is named by where it sits under
        // the root directory, which is the same whichever module first reached it.
        let path = if self.modules.is_empty() {
            String::new()
        } else {
            module_path(&file, &self.root_dir)
        };
        let ref_dir = parent_dir(&file);
        let child_dir = child_dir.map(|d| canonical(&d)).transpose()?;
        let origin = ModuleOrigin {
            path,
            display,
            file: file.clone(),
            ref_dir,
            child_dir,
            parent: None,
        };
        let id = self.push_module(origin, items, false)?;
        self.by_file.insert(file, id);
        Ok(id)
    }

    /// Register one module (a file, or an inline `module` block) along with every block
    /// it declares.
    ///
    /// An inline block is a module in every sense that matters here: its items are private
    /// unless exported, a qualified path reaches into it, and its declarations live in a
    /// namespace of their own. Making it a graph module rather than a special case is what
    /// lets the visibility rule, the canonical names, and the `ModuleId` stamp apply
    /// unchanged.
    fn push_module(
        &mut self,
        origin: ModuleOrigin,
        mut items: Vec<Item>,
        inherited_no_prelude: bool,
    ) -> Result<usize, ModuleError> {
        // Imports, inline blocks, and the `@no_prelude` marker are consumed here rather
        // than carried along: after resolution the program is one item list, and none
        // of the three has anything left to say to a downstream pass.
        let mut imports = Vec::new();
        let mut blocks: Vec<ModuleDef> = Vec::new();
        let mut no_prelude = inherited_no_prelude;
        items.retain(|item| match item {
            Item::Import(def) => {
                imports.push(def.clone());
                false
            }
            Item::Module(def) => {
                blocks.push(def.clone());
                false
            }
            Item::NoPrelude(_) => {
                no_prelude = true;
                false
            }
            _ => true,
        });

        let mut declared = HashSet::new();
        let mut exported = HashSet::new();
        let mut declared_types = HashSet::new();
        let mut declared_enums = HashSet::new();
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
            if let Item::Enum(def) = item {
                declared_enums.insert(def.name.name.clone());
            }
        }

        let id = self.modules.len();
        self.modules.push(Module {
            path: origin.path,
            display: origin.display,
            file: origin.file,
            ref_dir: origin.ref_dir,
            child_dir: origin.child_dir,
            parent: origin.parent,
            inline_children: HashMap::new(),
            exported_blocks: HashSet::new(),
            items,
            imports,
            declared,
            reexports: HashMap::new(),
            exported,
            declared_types,
            declared_enums,
            no_prelude,
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
            let origin = ModuleOrigin {
                path: child_path,
                display: child_display,
                file,
                ref_dir,
                child_dir: None,
                parent: Some(id),
            };
            let child = self.push_module(origin, block.items, no_prelude)?;
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
            if block.exported {
                self.modules[id].exported_blocks.insert(name);
            }
        }
        Ok(id)
    }

    /// Reject two loaded modules that would share a path, so share every name. The one way
    /// it happens is an inline block in the root file and a file at the root of the same
    /// name: the root reaches its block, every other module the file.
    fn check_module_paths(&self) -> Result<(), ModuleError> {
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for (id, module) in self.modules.iter().enumerate() {
            if let Some(first) = seen.insert(&module.path, id) {
                return Err(ModuleError::DuplicateModulePath {
                    path: module.path.clone(),
                    first: self.modules[first].display.clone(),
                    second: module.display.clone(),
                });
            }
        }
        Ok(())
    }

    /// The first segment of a path written in module `from` without `./`: an inline block
    /// in scope (`from`'s own, or an enclosing module's), else a file at the program root.
    ///
    /// A path starting `./` or `../` is anchored at the writing file's directory instead,
    /// climbed `levels` times.
    pub(crate) fn resolve_head(
        &self,
        from: usize,
        anchor: Option<u32>,
        segment: &str,
    ) -> Option<usize> {
        let dir = match anchor {
            None => {
                if let Some(block) = self.lexical_block(from, segment) {
                    return Some(block);
                }
                self.root_dir.clone()
            }
            Some(levels) => climb(&self.modules[from].ref_dir, levels),
        };
        self.loaded_file(&dir, segment)
    }

    /// A segment below module `current`: an inline block it declares, else a file in its
    /// directory. Only already-loaded modules are consulted; discovery has finished by the
    /// time this is asked.
    pub(crate) fn resolve_child(&self, current: usize, segment: &str) -> Option<usize> {
        if let Some(id) = self.modules[current].inline_children.get(segment) {
            return Some(*id);
        }
        let dir = self.modules[current].child_dir.as_ref()?;
        self.loaded_file(dir, segment)
    }

    fn loaded_file(&self, dir: &Path, segment: &str) -> Option<usize> {
        let candidates = [
            dir.join(segment).join("mod.nr"),
            dir.join(format!("{}.nr", segment)),
        ];
        candidates
            .iter()
            .find_map(|candidate| canonical(candidate).ok().and_then(|c| self.by_file.get(&c)))
            .copied()
    }

    /// Reject reaching module `target` from `from` when it is an inline block that keeps
    /// to its enclosing module: a block without `export` is reached only from inside the
    /// module that declares it.
    pub(crate) fn check_block_reachable(
        &self,
        from: usize,
        target: usize,
    ) -> Result<(), ModuleError> {
        let Some(parent) = self.modules[target].parent else {
            return Ok(());
        };
        let name = self.modules[target]
            .path
            .rsplit("::")
            .next()
            .unwrap_or_default();
        if self.modules[parent].exported_blocks.contains(name) || self.encloses(parent, from) {
            return Ok(());
        }
        Err(ModuleError::PrivateItem {
            module: self.path_of(parent).to_string(),
            item: name.to_string(),
            from: self.display(from).to_string(),
        })
    }

    /// Is module `inner` the module `outer`, or a block written inside it at any depth?
    pub(crate) fn encloses(&self, outer: usize, inner: usize) -> bool {
        let mut current = Some(inner);
        while let Some(id) = current {
            if id == outer {
                return true;
            }
            current = self.modules[id].parent;
        }
        false
    }

    /// The inline block module `id` declares as `name`.
    pub(crate) fn inline_child(&self, id: usize, name: &str) -> Option<usize> {
        self.modules[id].inline_children.get(name).copied()
    }

    /// The inline block `name` in scope in module `from`: its own, or an enclosing
    /// module's, nearest first.
    pub(crate) fn lexical_block(&self, from: usize, name: &str) -> Option<usize> {
        let mut current = Some(from);
        while let Some(id) = current {
            if let Some(block) = self.modules[id].inline_children.get(name) {
                return Some(*block);
            }
            current = self.modules[id].parent;
        }
        None
    }

    /// Does module `from`, or a module enclosing it, declare a type called `name`?
    pub(crate) fn lexical_type(&self, from: usize, name: &str) -> bool {
        let mut current = Some(from);
        while let Some(id) = current {
            if self.modules[id].declared_types.contains(name) {
                return true;
            }
            current = self.modules[id].parent;
        }
        false
    }

    /// Does module `id` itself declare `name`, as an item or an inline block? An import
    /// may not bind such a name: the two would compete for one spelling.
    pub(crate) fn declares_own(&self, id: usize, name: &str) -> bool {
        self.modules[id].declared.contains(name)
            || self.modules[id].inline_children.contains_key(name)
    }

    /// Does module `id` declare `name`, or re-export it?
    pub(crate) fn declares(&self, id: usize, name: &str) -> bool {
        self.modules[id].declared.contains(name) || self.modules[id].reexports.contains_key(name)
    }

    pub(crate) fn declares_type(&self, id: usize, name: &str) -> bool {
        let (origin, item) = self.flat_origin(id, name);
        self.modules[origin].declared_types.contains(&item)
    }

    pub(crate) fn declares_enum(&self, id: usize, name: &str) -> bool {
        let (origin, item) = self.flat_origin(id, name);
        self.modules[origin].declared_enums.contains(&item)
    }

    /// Does any module in the graph hold an inline `module {name} { ... }` block?
    ///
    /// Used only to explain a path that resolved nothing: a block is reached from inside
    /// the module that declares it, which is the one way a real module can fail to be found.
    pub(crate) fn has_inline_block_named(&self, name: &str) -> bool {
        self.modules
            .iter()
            .any(|module| module.inline_children.contains_key(name))
    }

    /// Is `name` reachable from outside module `id`?
    ///
    /// A re-export is reachable by construction: making a name reachable through this
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

    /// The name module `id`'s declaration `name` carries once resolution is done: its
    /// module path in front of it, or the bare name for the root.
    pub(crate) fn canonical(&self, id: usize, name: &str) -> String {
        canonical_name(&self.modules[id].path, name)
    }

    /// The canonical name a reference to `name` through module `id` lands on, following
    /// a re-export to the declaration.
    pub(crate) fn canonical_through(&self, id: usize, name: &str) -> String {
        let (origin, item) = self.flat_origin(id, name);
        self.canonical(origin, &item)
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
    /// keeps to itself. A module may always name its own declarations, and a block those
    /// of every module it is written inside: it is part of them.
    pub(crate) fn check_visible(
        &self,
        from: usize,
        module: usize,
        name: &str,
    ) -> Result<(), ModuleError> {
        if self.encloses(module, from) || self.exports(module, name) {
            return Ok(());
        }
        Err(ModuleError::PrivateItem {
            module: self.path_of(module).to_string(),
            item: name.to_string(),
            from: self.display(from).to_string(),
        })
    }

    /// Did this module's file opt out of the implicit prelude?
    pub(crate) fn no_prelude(&self, id: usize) -> bool {
        self.modules[id].no_prelude
    }

    /// The file a module is read from, as diagnostics name it.
    pub(crate) fn display(&self, id: usize) -> &str {
        &self.modules[id].display
    }

    /// The module's path. The root has none, so it answers with its file instead.
    pub(crate) fn path_of(&self, id: usize) -> &str {
        let module = &self.modules[id];
        if module.path.is_empty() {
            &module.display
        } else {
            &module.path
        }
    }

    pub(crate) fn into_program(self) -> crate::ResolvedProgram {
        // The prelude's declarations join the program as one list, so opting out of them
        // is a decision for the program rather than for a file, and the root is the file
        // that makes it.
        let no_prelude = self.modules.first().is_some_and(|root| root.no_prelude);
        let mut items = Vec::new();
        let mut modules = Vec::new();
        for (id, module) in self.modules.into_iter().enumerate() {
            // Every reference was already rewritten to the canonical name, so renaming
            // the declarations is what makes the two meet.
            for mut item in module.items {
                rename_declaration(&mut item, &module.path);
                stamp_module(&mut item, id as ModuleId);
                items.push(item);
            }
            modules.push(ResolvedModule {
                path: module.path,
                file: module.file,
            });
        }
        crate::defaults::inject_trait_defaults(&mut items);
        crate::ResolvedProgram {
            items,
            modules,
            no_prelude,
        }
    }
}

/// `path::name`, or `name` alone for the root's empty path.
pub(crate) fn canonical_name(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", path, name)
    }
}

/// The `::`-separated segments a qualified site names, item or type name included.
///
/// A site that names no path yields nothing, and a bare name yields the one segment an
/// import table is keyed on.
pub(crate) fn site_segments(site: &Site<'_>) -> Vec<String> {
    match site {
        Site::TypeName(name) => split_segments(&name.name),
        Site::Pattern(ast_types::Pattern::Enum {
            enum_name, variant, ..
        }) => {
            let mut segments = split_segments(&enum_name.name);
            segments.push(variant.name.clone());
            segments
        }
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

pub(crate) fn split_segments(name: &str) -> Vec<String> {
    name.split("::").map(str::to_string).collect()
}

fn parent_dir(file: &Path) -> PathBuf {
    file.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// `dir` with `levels` trailing components removed: where `../` leads.
fn climb(dir: &Path, levels: u32) -> PathBuf {
    let mut dir = dir.to_path_buf();
    for _ in 0..levels {
        dir = parent_dir(&dir);
    }
    dir
}

/// The module path of `file`: its location under `root_dir` with `/` read as `::`, a
/// `mod.nr` named by its directory and a leaf by its stem. A file above the root, reached
/// through `../`, keeps a `..` segment per level climbed, so its names cannot collide with
/// a module under the root.
fn module_path(file: &Path, root_dir: &Path) -> String {
    let file_parts: Vec<Component<'_>> = file.components().collect();
    let root_parts: Vec<Component<'_>> = root_dir.components().collect();
    let common = file_parts
        .iter()
        .zip(&root_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut segments: Vec<String> = vec!["..".to_string(); root_parts.len() - common];
    segments.extend(
        file_parts[common..]
            .iter()
            .map(|part| part.as_os_str().to_string_lossy().into_owned()),
    );
    match segments.last().map(String::as_str) {
        Some("mod.nr") => {
            segments.pop();
        }
        Some(leaf) => {
            let stem = leaf.strip_suffix(".nr").unwrap_or(leaf).to_string();
            segments.pop();
            segments.push(stem);
        }
        None => {}
    }
    segments.join("::")
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

/// Give a declaration its canonical name: `path::Name` outside the root module.
fn rename_declaration(item: &mut Item, path: &str) {
    if path.is_empty() {
        return;
    }
    let name = match item {
        Item::Function(def) => &mut def.name,
        Item::Struct(def) => &mut def.name,
        Item::Enum(def) => &mut def.name,
        Item::Trait(def) => &mut def.name,
        Item::Const(def) => &mut def.name,
        Item::Newtype(def) => &mut def.name,
        Item::Impl(_) | Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => return,
    };
    name.name = canonical_name(path, &name.name);
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
        // they are injected as.
        // An inline block is lifted into a module of its own before this runs, so none
        // reaches the merge.
        Item::Enum(_)
        | Item::Newtype(_)
        | Item::Trait(_)
        | Item::Import(_)
        | Item::Module(_)
        | Item::NoPrelude(_) => {}
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
        Item::Impl(_) | Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => false,
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
        Item::Impl(_) | Item::Import(_) | Item::Module(_) | Item::NoPrelude(_) => None,
    }
}

fn type_name(item: &Item) -> Option<&str> {
    match item {
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Function(_)
        | Item::Const(_)
        | Item::Impl(_)
        | Item::Import(_)
        | Item::Module(_)
        | Item::NoPrelude(_) => None,
    }
}
