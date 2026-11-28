//! References inside the source. A reference the source attaches is read and
//! planned like the source itself, then carried in the top reference's
//! snapshot as one more local block, named as AutoCAD names its symbols:
//! layer `Walls` of `floors`, attached by `site`, is `site|floors|Walls`.
//!
//! The nested content belongs to the top reference: reloading, unloading,
//! clipping, binding and detaching act on it as a whole. Overlays are not
//! carried through an attachment, as in AutoCAD.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use opendraft_core::{BlockDef, Document, Insert};

use super::super::error::XrefLoadError;
use super::super::path::resolve;
use super::super::source::read_source;
use super::{Planner, XrefPlanError};

/// How many references may sit inside one another.
pub(super) const MAX_NESTING_DEPTH: usize = 16;

/// How many nested references one load may read.
const MAX_NESTED_LOADS: u64 = 256;

impl<'a> Planner<'a> {
    /// The ordinary insert that stands for the source's insert of a
    /// reference: the same placement, of the block the reference's content
    /// became. `None` when the reference was not loaded (the reason is a
    /// note).
    pub(super) fn nested_insert(
        &mut self,
        insert: &Insert,
    ) -> Result<Option<Insert>, XrefPlanError> {
        let key = insert.block.id;
        let block = match self.nested_cache.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                let made = self.load_nested(key)?;
                self.nested_cache.insert(key, made.clone());
                made
            }
        };
        Ok(block.map(|block| {
            let mut standing_in = insert.clone();
            standing_in.block = block;
            standing_in.is_xref = false;
            standing_in.dynamic = None;
            standing_in.attributes.clear();
            standing_in
        }))
    }

    /// Whether `name` is a symbol one of the source's references brought
    /// (`floors|Walls`): the source holds a copy, but the reference is
    /// planned from its own file, so the copy is left out.
    pub(super) fn is_nested_symbol(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.source
            .xrefs()
            .iter()
            .filter_map(|x| self.source.block(x.block))
            .any(|b| lower.starts_with(&format!("{}|", b.name.to_lowercase())))
    }

    fn not_loaded(&mut self, name: &str, why: impl std::fmt::Display) -> Option<Arc<BlockDef>> {
        self.notes.push(format!(
            "Nested external reference \"{name}\" was not loaded: {why}."
        ));
        None
    }

    fn load_nested(
        &mut self,
        key: opendraft_core::BlockId,
    ) -> Result<Option<Arc<BlockDef>>, XrefPlanError> {
        let source = self.source;
        let (Some(xref), Some(root)) = (source.xref(key), source.block(key)) else {
            self.skip("insert of a missing reference");
            return Ok(None);
        };
        let name = root.name.clone();
        // Listed in the report whether or not it can be read.
        let index = self.nested.len();
        self.nested.push(opendraft_core::XrefNested {
            name: name.clone(),
            parent: self.self_index,
            path: xref.path.clone(),
            kind: xref.kind,
            found: String::new(),
            stamp: None,
            loaded: false,
        });
        if xref.kind.is_overlay() {
            return Ok(self.not_loaded(&name, "an overlay is not carried through an attachment"));
        }
        if self.depth >= MAX_NESTING_DEPTH {
            return Ok(self.not_loaded(&name, "references are nested too deeply"));
        }
        if self.loaded_nested >= MAX_NESTED_LOADS {
            return Ok(self.not_loaded(&name, "the source has too many references"));
        }
        let path = match resolve(&xref.path, self.source_dir.as_deref()) {
            Ok(path) => path,
            Err(why) => return Ok(self.not_loaded(&name, why)),
        };
        let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if self.chain.contains(&canonical) {
            return Ok(self.not_loaded(&name, "it would lead back to a drawing already open"));
        }
        let loaded = match read_source(&path, self.cancelled, &self.limits) {
            Ok(loaded) => loaded,
            Err(XrefLoadError::Cancelled) => return Err(XrefPlanError::Cancelled),
            Err(why) => return Ok(self.not_loaded(&name, why)),
        };
        self.loaded_nested += 1;
        if let Some(entry) = self.nested.get_mut(index) {
            entry.loaded = true;
            entry.found = path.display().to_string();
            entry.stamp = std::fs::metadata(&path)
                .ok()
                .map(|m| opendraft_core::XrefStamp {
                    length: m.len(),
                    modified: m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs() as i64),
                });
        }
        for warning in &loaded.warnings {
            self.notes.push(format!("\"{name}\": {warning}"));
        }

        let mut child = self.child(
            index,
            &loaded.document,
            format!("{}|{name}", self.name),
            format!("{}{name}|", self.prefix),
            path.parent(),
            canonical,
        );
        child.map_linetypes()?;
        child.map_layers()?;
        child.map_styles()?;
        let mut order = Vec::new();
        child.collect_model(&mut order)?;
        child.build_locals(&order)?;
        let entities = child.root_entities()?;
        let bindings = child.own_bindings();
        let locals: Vec<Arc<BlockDef>> = child.blocks.values().cloned().collect();
        self.absorb(child);
        self.bindings_done.extend(bindings);
        self.locals_done.extend(locals);

        let id = self
            .next_block
            .checked_add(self.issued)
            .map(opendraft_core::BlockId)
            .ok_or(XrefPlanError::TooMany {
                what: "blocks in the source",
                limit: self.limits.max_local_blocks,
            })?;
        self.issued += 1;
        let block = Arc::new(BlockDef {
            id,
            name: name.clone(),
            base: loaded.document.insert_base(),
            description: String::new(),
            unit: loaded.document.units().insert,
            entities,
            dynamic: None,
        });
        self.locals_done.push(block.clone());
        self.notes.push(format!(
            "Loaded nested external reference \"{name}\" from {}.",
            path.display()
        ));
        Ok(Some(block))
    }

    /// A planner for a reference of this source, carrying on from this one's
    /// counters and tables so that ids, names and limits run across them.
    fn child<'b>(
        &mut self,
        index: usize,
        source: &'b Document,
        name: String,
        prefix: String,
        dir: Option<&Path>,
        canonical: std::path::PathBuf,
    ) -> Planner<'b>
    where
        'a: 'b,
    {
        let mut chain = self.chain.clone();
        chain.push(canonical);
        Planner {
            host: self.host,
            source,
            name,
            kind: self.kind,
            existing: self.existing.clone(),
            layers: HashMap::new(),
            linetypes: HashMap::new(),
            styles: HashMap::new(),
            added_layers: std::mem::take(&mut self.added_layers),
            changed_layers: std::mem::take(&mut self.changed_layers),
            linetypes_plan: self.linetypes_plan.take(),
            style_table: std::mem::take(&mut self.style_table),
            next_style: self.next_style,
            next_layer: self.next_layer,
            next_block: self.next_block,
            blocks: HashMap::new(),
            block_ids: HashMap::new(),
            visiting: HashSet::new(),
            skipped: std::mem::take(&mut self.skipped),
            notes: std::mem::take(&mut self.notes),
            limits: self.limits,
            cancelled: self.cancelled,
            steps: self.steps,
            cleared_associations: self.cleared_associations,
            dynamic_states: self.dynamic_states,
            prefix,
            source_dir: dir.map(Path::to_path_buf),
            chain,
            depth: self.depth + 1,
            issued: self.issued,
            locals_done: std::mem::take(&mut self.locals_done),
            bindings_done: std::mem::take(&mut self.bindings_done),
            entities_total: self.entities_total,
            loaded_nested: self.loaded_nested,
            nested: std::mem::take(&mut self.nested),
            self_index: Some(index),
            nested_cache: HashMap::new(),
        }
    }

    /// Take back what a child planner added.
    fn absorb(&mut self, child: Planner<'_>) {
        self.added_layers = child.added_layers;
        self.changed_layers = child.changed_layers;
        self.linetypes_plan = child.linetypes_plan;
        self.style_table = child.style_table;
        self.next_style = child.next_style;
        self.next_layer = child.next_layer;
        self.skipped = child.skipped;
        self.notes = child.notes;
        self.nested = child.nested;
        self.steps = child.steps;
        self.cleared_associations = child.cleared_associations;
        self.dynamic_states = child.dynamic_states;
        self.issued = child.issued;
        self.locals_done = child.locals_done;
        self.bindings_done = child.bindings_done;
        self.entities_total = child.entities_total;
        self.loaded_nested = child.loaded_nested;
    }
}
