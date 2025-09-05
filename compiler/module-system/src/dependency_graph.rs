//! Dependency graph analysis for modules

use crate::{ModuleError, ModuleId};
use std::collections::{HashMap, HashSet, VecDeque};

/// Represents the dependency relationship between modules
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Module ID -> Set of modules it depends on
    dependencies: HashMap<ModuleId, HashSet<ModuleId>>,
    /// Module ID -> Set of modules that depend on it
    dependents: HashMap<ModuleId, HashSet<ModuleId>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
        }
    }

    /// Add a dependency relationship
    pub fn add_dependency(&mut self, from: ModuleId, to: ModuleId) {
        self.dependencies
            .entry(from)
            .or_insert_with(HashSet::new)
            .insert(to);
        
        self.dependents
            .entry(to)
            .or_insert_with(HashSet::new)
            .insert(from);
    }

    /// Get all modules that a module depends on
    pub fn get_dependencies(&self, module: ModuleId) -> HashSet<ModuleId> {
        self.dependencies.get(&module).cloned().unwrap_or_default()
    }

    /// Detect circular dependencies in the graph
    pub fn detect_cycles(&self) -> Result<(), ModuleError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for &module in self.dependencies.keys() {
            if !visited.contains(&module) {
                if self.has_cycle_dfs(module, &mut visited, &mut rec_stack) {
                    return Err(ModuleError::CircularDependency {
                        modules: vec![format!("{}", module)],
                    });
                }
            }
        }

        Ok(())
    }

    /// DFS-based cycle detection helper
    fn has_cycle_dfs(
        &self,
        module: ModuleId,
        visited: &mut HashSet<ModuleId>,
        rec_stack: &mut HashSet<ModuleId>,
    ) -> bool {
        visited.insert(module);
        rec_stack.insert(module);

        if let Some(dependencies) = self.dependencies.get(&module) {
            for &dep in dependencies {
                if !visited.contains(&dep) && self.has_cycle_dfs(dep, visited, rec_stack) {
                    return true;
                } else if rec_stack.contains(&dep) {
                    return true;
                }
            }
        }

        rec_stack.remove(&module);
        false
    }

    /// Check if the graph is empty
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty() && self.dependents.is_empty()
    }

    /// Get the number of modules in the graph  
    pub fn len(&self) -> usize {
        let mut modules = std::collections::HashSet::new();
        for &module in self.dependencies.keys() {
            modules.insert(module);
        }
        for &module in self.dependents.keys() {
            modules.insert(module);
        }
        modules.len()
    }

    /// Get a topological ordering of modules
    pub fn topological_sort(&self) -> Result<Vec<ModuleId>, ModuleError> {
        self.detect_cycles()?;

        let mut in_degree = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Initialize in-degree counts for all modules
        for &module in self.dependencies.keys() {
            in_degree.insert(module, 0);
        }
        for &module in self.dependents.keys() {
            in_degree.insert(module, 0);
        }
        // Also include all dependencies as potential modules
        for deps in self.dependencies.values() {
            for &dep in deps {
                in_degree.entry(dep).or_insert(0);
            }
        }

        // Calculate in-degrees (how many dependencies this module has)
        for (&module, deps) in &self.dependencies {
            *in_degree.entry(module).or_insert(0) += deps.len();
        }

        // Find modules with no dependencies (can be processed first)
        for (&module, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(module);
            }
        }

        // Process modules in topological order
        while let Some(module) = queue.pop_front() {
            result.push(module);

            // Now that this module is processed, reduce dependency count for modules that depend on it
            if let Some(dependents) = self.dependents.get(&module) {
                for &dependent in dependents {
                    if let Some(degree) = in_degree.get_mut(&dependent) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        Ok(result)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}