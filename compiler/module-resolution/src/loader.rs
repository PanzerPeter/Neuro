//! Discovery: which files belong to this program, and what each one declares.
//!
//! A module is pulled in only when a qualified path reaches into it, which is why a
//! directory holding a dozen unrelated single-file programs still compiles one at a time.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ast_types::Item;

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
    pub(crate) items: Vec<Item>,
    pub(crate) declared: HashSet<String>,
    /// Declared struct / enum / newtype / trait names. A locally declared type wins over a
    /// same-named file, so `Point::new` keeps meaning the associated function even when a
    /// `Point.nr` happens to sit next door.
    declared_types: HashSet<String>,
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
    /// final segment, which is the item or type being named rather than a module.
    fn module_chains(&mut self, id: usize) -> Vec<Vec<String>> {
        let mut items = std::mem::take(&mut self.modules[id].items);
        let declared_types = std::mem::take(&mut self.modules[id].declared_types);
        let mut chains: Vec<Vec<String>> = Vec::new();
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
        let mut path = String::new();
        for segment in chain {
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

        let mut declared = HashSet::new();
        let mut declared_types = HashSet::new();
        for item in &items {
            if let Some(name) = item_name(item) {
                declared.insert(name.to_string());
            }
            if let Some(name) = type_name(item) {
                declared_types.insert(name.to_string());
            }
        }

        let ref_dir = parent_dir(&file);
        let child_dir = child_dir.map(|d| canonical(&d)).transpose()?;
        let id = self.modules.len();
        self.modules.push(Module {
            path,
            display,
            file: file.clone(),
            ref_dir,
            child_dir,
            items,
            declared,
            declared_types,
        });
        self.by_file.insert(file, id);
        Ok(id)
    }

    /// The module a path segment names: a child of `current`, or — for the first segment,
    /// where `current` is `None` — a module beside `from`. Only already-loaded modules are
    /// consulted; discovery has finished by the time this is asked.
    pub(crate) fn resolve_segment(
        &self,
        from: usize,
        current: Option<usize>,
        segment: &str,
    ) -> Option<usize> {
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

    /// Does module `id` declare `name`?
    pub(crate) fn declares(&self, id: usize, name: &str) -> bool {
        self.modules[id].declared.contains(name)
    }

    pub(crate) fn declares_type(&self, id: usize, name: &str) -> bool {
        self.modules[id].declared_types.contains(name)
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
        for module in self.modules {
            modules.push(ResolvedModule {
                path: module.path,
                file: module.file,
            });
            items.extend(module.items);
        }
        crate::ResolvedProgram { items, modules }
    }
}

/// The `::`-separated segments a qualified site names, item or type name included.
pub(crate) fn site_segments(site: &Site<'_>) -> Vec<String> {
    match site {
        Site::TypeName(name) => split_segments(&name.name),
        Site::Expr(expr) => match expr {
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

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Function(def) => Some(&def.name.name),
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Const(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Impl(_) => None,
    }
}

fn type_name(item: &Item) -> Option<&str> {
    match item {
        Item::Struct(def) => Some(&def.name.name),
        Item::Enum(def) => Some(&def.name.name),
        Item::Trait(def) => Some(&def.name.name),
        Item::Newtype(def) => Some(&def.name.name),
        Item::Function(_) | Item::Const(_) | Item::Impl(_) => None,
    }
}
