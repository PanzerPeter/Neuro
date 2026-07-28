//! Newtype declarations: predeclaration, inner-type resolution, and cycle
//! detection.
//!
//! One of the declaration-kind modules under `declarations`; each adds methods
//! to the same `impl TypeChecker` block.

use super::is_builtin_type_name;
use crate::errors::TypeError;
use crate::type_checkers::TypeChecker;
use crate::types::Type;
use ast_types::NewtypeDef;
use shared_types::Span;
use std::collections::{HashMap, HashSet};

impl TypeChecker {
    /// Pre-register a newtype's NAME with a placeholder inner type, so the
    /// name resolves everywhere before its inner type is resolved in
    /// [`Self::resolve_newtype_inners`]. Rejects a name that collides with a builtin,
    /// struct, enum, or another newtype.
    pub(crate) fn predeclare_newtype(&mut self, def: &NewtypeDef) {
        let name = &def.name.name;
        if is_builtin_type_name(name)
            || self.struct_defs.contains_key(name)
            || self.enum_defs.contains_key(name)
            || self.newtype_defs.contains_key(name)
        {
            self.record_error(TypeError::NewtypeAlreadyDefined {
                name: name.clone(),
                span: def.name.span,
            });
            return;
        }
        self.newtype_defs.insert(name.clone(), Type::Unknown);
    }

    /// Resolve each newtype's inner type now that every nominal name is registered,
    /// then reject cyclic newtypes and non-Copy inner types.
    pub(crate) fn resolve_newtype_inners(&mut self, items: &[ast_types::Item]) {
        // Phase 1: resolve every accepted newtype's inner type. A duplicate/rejected
        // declaration (still absent or already resolved) is skipped so it cannot
        // overwrite the first, valid definition.
        let mut spans: HashMap<String, Span> = HashMap::new();
        for item in items {
            if let ast_types::Item::Newtype(def) = item {
                let name = def.name.name.clone();
                if spans.contains_key(&name) || self.newtype_defs.get(&name) != Some(&Type::Unknown)
                {
                    continue;
                }
                let inner = self.resolve_type(&def.inner).unwrap_or(Type::Unknown);
                self.newtype_defs.insert(name.clone(), inner);
                spans.insert(name, def.inner.span());
            }
        }

        // Phase 2: with all inners resolved, reject cycles (which would otherwise make
        // the Copy check below recurse forever) and non-Copy inner types.
        for (name, inner_span) in &spans {
            let mut seen = HashSet::new();
            if self.newtype_cycles(name, &mut seen) {
                self.record_error(TypeError::CyclicNewtype {
                    name: name.clone(),
                    span: *inner_span,
                });
                // Break the cycle so downstream Copy checks terminate.
                self.newtype_defs.insert(name.clone(), Type::Unknown);
                continue;
            }
            let inner = self
                .newtype_defs
                .get(name)
                .cloned()
                .unwrap_or(Type::Unknown);
            if !matches!(inner, Type::Unknown) && !self.is_type_copy(&inner) {
                self.record_error(TypeError::NewtypeInnerNotCopy {
                    name: name.clone(),
                    inner,
                    span: *inner_span,
                });
            }
        }
    }

    /// Whether newtype `name` reaches itself through its inner type chain,
    /// following newtype wrappers and the Copy-recursing aggregates (arrays, tuples,
    /// references). `seen` is the current DFS path; a revisit is a back-edge.
    pub(super) fn newtype_cycles(&self, name: &str, seen: &mut HashSet<String>) -> bool {
        if !seen.insert(name.to_string()) {
            return true;
        }
        let cyclic = match self.newtype_defs.get(name) {
            Some(inner) => self.type_reaches_cyclic_newtype(&inner.clone(), seen),
            None => false,
        };
        seen.remove(name);
        cyclic
    }

    /// Whether `ty` reaches a newtype currently on the DFS path in `seen`.
    pub(super) fn type_reaches_cyclic_newtype(
        &self,
        ty: &Type,
        seen: &mut HashSet<String>,
    ) -> bool {
        match ty {
            Type::Newtype(n) => self.newtype_cycles(n, seen),
            Type::Array { element, .. } => self.type_reaches_cyclic_newtype(element, seen),
            Type::Tuple(elements) => elements
                .iter()
                .any(|e| self.type_reaches_cyclic_newtype(e, seen)),
            Type::Reference { inner, .. } => self.type_reaches_cyclic_newtype(inner, seen),
            _ => false,
        }
    }
}
