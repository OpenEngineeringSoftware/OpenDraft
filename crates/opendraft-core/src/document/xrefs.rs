//! The drawing's external-reference table: the accessors, the edits an
//! action makes, and the invariants `validate` checks.
//!
//! The table is keyed by the root block's id. Local definitions belong to
//! the record, not to the block table, and are reached from the root's
//! inserts by `Arc`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{after, Document};
use crate::block::{block_name_problem, BlockDef, BlockId, Insert, MAX_EXPANSION, MAX_NESTING};
use crate::validate::DocumentError;
use crate::xref::{XrefDef, MAX_XREF_PATH};
use crate::{Entity, EntityId, LayerId, Point};

impl Document {
    /// The external-reference table, in attach order.
    pub fn xrefs(&self) -> &[XrefDef] {
        &self.xrefs
    }

    /// The record whose root is `block`.
    pub fn xref(&self, block: BlockId) -> Option<&XrefDef> {
        self.xrefs.iter().find(|x| x.block == block)
    }

    /// The record called `name`, ignoring case.
    pub fn xref_by_name(&self, name: &str) -> Option<&XrefDef> {
        let name = name.trim();
        self.xrefs.iter().find(|x| {
            self.block(x.block)
                .is_some_and(|b| b.name.eq_ignore_ascii_case(name))
        })
    }

    /// The record an insert refers to, when it is an xref insert.
    pub fn xref_of_insert(&self, insert: &Insert) -> Option<&XrefDef> {
        self.xref(insert.block.id)
    }

    /// The record the object is an insert of, if any.
    pub fn xref_for_object(&self, id: EntityId) -> Option<&XrefDef> {
        match self.get(id)? {
            Entity::Insert(i) => self.xref_of_insert(i),
            _ => None,
        }
    }

    pub fn is_xref_block(&self, block: BlockId) -> bool {
        self.xref(block).is_some()
    }

    pub fn is_xref_insert(&self, id: EntityId) -> bool {
        self.xref_for_object(id).is_some()
    }

    /// The xref whose bound layer is `layer`.
    pub fn xref_layer_owner(&self, layer: LayerId) -> Option<BlockId> {
        self.xrefs
            .iter()
            .find(|x| x.owns_layer(layer))
            .map(|x| x.block)
    }

    /// The top-level objects that insert the reference `block` directly.
    pub fn xref_references(&self, block: BlockId) -> Vec<EntityId> {
        self.objects()
            .iter()
            .filter(|o| matches!(&o.entity, Entity::Insert(i) if i.block.id == block))
            .map(|o| o.id)
            .collect()
    }

    pub fn xref_is_unloaded(&self, block: BlockId) -> bool {
        self.xref(block).is_some_and(|x| x.unloaded)
    }

    /// Whether any xref root or local definition uses text style `style`.
    pub fn xref_content_uses_text_style(&self, style: crate::TextStyleId) -> bool {
        self.xref_blocks().any(|def| {
            def.entities.iter().any(|e| match &e.entity {
                Entity::MText(t) => t.style == style,
                Entity::Text(t) => t.style() == style,
                _ => false,
            })
        })
    }

    /// Whether any xref root or local definition draws with linetype `id`.
    pub fn xref_content_uses_linetype(&self, id: crate::LinetypeId) -> bool {
        self.xref_blocks().any(|def| {
            def.entities
                .iter()
                .any(|e| e.attrs.linetype == crate::Attr::Set(id))
        })
    }

    /// Every xref root and local definition, for scans over xref content.
    pub fn xref_blocks(&self) -> impl Iterator<Item = &Arc<BlockDef>> {
        self.xrefs
            .iter()
            .flat_map(|x| x.blocks.iter())
            .chain(self.xrefs.iter().filter_map(|x| self.block(x.block)))
    }

    /// Replace the whole table, returning the old one (the undo action swaps
    /// them back). Raises the block and attribute-definition counters past
    /// every local definition, so an id a local block had is never handed
    /// out again after an undo or a reload.
    pub fn set_xrefs(&mut self, xrefs: Vec<XrefDef>) -> Vec<XrefDef> {
        for xref in &xrefs {
            for block in &xref.blocks {
                self.next_block_id = self.next_block_id.max(after(block.id.0));
                self.note_block_attribute_ids(block);
            }
        }
        std::mem::replace(&mut self.xrefs, xrefs)
    }

    pub fn push_xref(&mut self, xref: XrefDef) {
        let mut all = self.xrefs.clone();
        all.retain(|x| x.block != xref.block);
        all.push(xref);
        self.set_xrefs(all);
    }

    /// Take the record for `block` out, returning it. The root block stays;
    /// Detach removes it with `SetBlocks` in the same transaction.
    pub fn remove_xref(&mut self, block: BlockId) -> Option<XrefDef> {
        let at = self.xrefs.iter().position(|x| x.block == block)?;
        Some(self.xrefs.remove(at))
    }

    /// The drawing's `INSBASE`.
    pub fn insert_base(&self) -> Point {
        self.insert_base
    }

    pub fn set_insert_base(&mut self, at: Point) -> Point {
        std::mem::replace(&mut self.insert_base, at)
    }

    pub fn visretain(&self) -> bool {
        self.visretain
    }

    pub fn set_visretain(&mut self, keep: bool) -> bool {
        std::mem::replace(&mut self.visretain, keep)
    }

    /// The first external reference reachable from `ids`, walking into the
    /// blocks their inserts place. The clipboard refuses such a selection:
    /// copying it to another drawing would carry the snapshot and lose the
    /// reference.
    pub fn xref_reachable_from(&self, ids: &[EntityId]) -> Option<BlockId> {
        fn walk(def: &BlockDef, depth: usize) -> Option<BlockId> {
            if depth == 0 {
                return None;
            }
            for child in &def.entities {
                if let Entity::Insert(i) = &child.entity {
                    if i.is_xref {
                        return Some(i.block.id);
                    }
                    if let Some(found) = walk(&i.block, depth - 1) {
                        return Some(found);
                    }
                }
            }
            None
        }
        for id in ids {
            let Some(object) = self.object(*id) else {
                continue;
            };
            if let Entity::Insert(i) = &object.entity {
                if i.is_xref || self.is_xref_block(i.block.id) {
                    return Some(i.block.id);
                }
                if let Some(found) = walk(&i.block, MAX_NESTING) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// External-reference invariants. Runs before `validate_blocks` so that
    /// the block table may allow what only xref content may hold.
    pub(crate) fn validate_xrefs(&self) -> Result<(), DocumentError> {
        let bad = |name: &str, why: String| -> Result<(), DocumentError> {
            Err(DocumentError::BadXref(format!(
                "external reference \"{name}\": {why}"
            )))
        };
        let mut roots: HashSet<BlockId> = HashSet::new();
        let mut layer_owner: HashMap<LayerId, BlockId> = HashMap::new();
        let mut local_owner: HashMap<BlockId, BlockId> = HashMap::new();

        for xref in &self.xrefs {
            let Some(root) = self.block(xref.block) else {
                return Err(DocumentError::BadXref(format!(
                    "external reference block {} is not in the drawing",
                    xref.block.0
                )));
            };
            let name = root.name.clone();
            if !roots.insert(xref.block) {
                return bad(&name, "its root block is used twice".to_owned());
            }
            if block_name_problem(&name).is_some() {
                return bad(&name, "the root block has an invalid name".to_owned());
            }
            if xref.path.chars().count() > MAX_XREF_PATH {
                return bad(&name, "the saved path is too long".to_owned());
            }
            if xref.path.chars().any(|c| c == '\0' || c.is_control()) {
                return bad(&name, "the saved path has a control character".to_owned());
            }
            let mut sources: HashSet<String> = HashSet::new();
            for bound in &xref.layers {
                if bound.source.is_empty() || bound.source == "0" {
                    return bad(&name, "layer 0 cannot be bound".to_owned());
                }
                if !sources.insert(bound.source.to_ascii_lowercase()) {
                    return bad(
                        &name,
                        format!("source layer \"{}\" is bound twice", bound.source),
                    );
                }
                if bound.layer == LayerId::DEFAULT {
                    return bad(&name, "layer 0 cannot be bound".to_owned());
                }
                if bound.layer == self.current_layer() {
                    return bad(&name, "a bound layer is the current layer".to_owned());
                }
                let Some(layer) = self.layer(bound.layer) else {
                    return bad(
                        &name,
                        format!("bound layer {} does not exist", bound.layer.0),
                    );
                };
                let want = format!("{}|{}", name, bound.source);
                if !layer.name.eq_ignore_ascii_case(&want) {
                    return bad(
                        &name,
                        format!("bound layer \"{}\" is not named \"{want}\"", layer.name),
                    );
                }
                if layer_owner.insert(bound.layer, xref.block).is_some() {
                    return bad(&name, "a bound layer belongs to two references".to_owned());
                }
            }
            let mut seen: HashSet<BlockId> = HashSet::new();
            for block in &xref.blocks {
                if !seen.insert(block.id) {
                    return bad(&name, format!("block {} is used twice", block.id.0));
                }
                if block.id.0 >= self.next_block_id() {
                    return Err(DocumentError::CounterBehind);
                }
                if self.block(block.id).is_some() {
                    return bad(
                        &name,
                        format!("block {} is also in the block table", block.id.0),
                    );
                }
                if local_owner.insert(block.id, xref.block).is_some() {
                    return bad(
                        &name,
                        format!("block {} belongs to two references", block.id.0),
                    );
                }
            }
            let locals: HashMap<BlockId, Arc<BlockDef>> =
                xref.blocks.iter().map(|b| (b.id, b.clone())).collect();
            let root_size = self.check_xref_content(&name, root, xref, &locals)?;
            if root_size > MAX_EXPANSION as u64 {
                return bad(&name, "its content expands past the limit".to_owned());
            }
        }

        for object in self.objects() {
            if let Entity::Insert(i) = &object.entity {
                let is_root = self.is_xref_block(i.block.id);
                if i.is_xref && !is_root {
                    return Err(DocumentError::BadXref(format!(
                        "object {} is marked as an external reference but its block is not one",
                        object.id.0
                    )));
                }
                if is_root && !i.is_xref {
                    return Err(DocumentError::BadXref(format!(
                        "object {} does not place the external reference it points at",
                        object.id.0
                    )));
                }
            }
        }
        for block in self.blocks() {
            if self.xref(block.id).is_some() {
                continue;
            }
            if block.entities.iter().any(|e| match &e.entity {
                Entity::Insert(i) => i.is_xref || self.is_xref_block(i.block.id),
                _ => false,
            }) {
                return Err(DocumentError::BadXref(format!(
                    "block \"{}\" contains an external reference",
                    block.name
                )));
            }
        }
        Ok(())
    }

    /// Check one root's content and every local block, returning the size of
    /// the root's full expansion. Layer bindings and insert targets follow
    /// the rules for xref content only.
    fn check_xref_content(
        &self,
        name: &str,
        root: &BlockDef,
        xref: &XrefDef,
        locals: &HashMap<BlockId, Arc<BlockDef>>,
    ) -> Result<u64, DocumentError> {
        let bad =
            |why: String| DocumentError::BadXref(format!("external reference \"{name}\": {why}"));
        let check = |def: &BlockDef| -> Result<(), DocumentError> {
            let mut ids = HashSet::new();
            for child in &def.entities {
                if !ids.insert(child.id) {
                    return Err(bad(format!("block \"{}\" repeats an object id", def.name)));
                }
                let layer = child.attrs.layer;
                // Layer 0, a layer bound to this reference, or an ordinary
                // layer XBIND took off it; never another reference's layer.
                let layer_ok = self.layer(layer).is_some()
                    && (layer == LayerId::DEFAULT
                        || xref.owns_layer(layer)
                        || self.xref_layer_owner(layer).is_none());
                if !layer_ok {
                    return Err(bad(format!(
                        "block \"{}\" has an object on layer {}",
                        def.name, layer.0
                    )));
                }
                if !self.xref_entity_ok(&child.entity) {
                    return Err(bad(format!(
                        "block \"{}\" holds an invalid object",
                        def.name
                    )));
                }
            }
            Ok(())
        };
        check(root)?;
        for block in locals.values() {
            if block.name.trim().is_empty() || block.name.chars().count() > 255 {
                return Err(bad(
                    "a local block has an empty or over-long name".to_owned()
                ));
            }
            if !block.base.x.is_finite() || !block.base.y.is_finite() {
                return Err(bad(format!(
                    "block \"{}\" has a non-finite base",
                    block.name
                )));
            }
            check(block)?;
        }
        let mut memo: HashMap<BlockId, u64> = HashMap::new();
        let mut stack: Vec<BlockId> = Vec::new();
        self.xref_subtree(root, locals, 0, &mut stack, &mut memo)
            .map_err(|why| DocumentError::BadXref(format!("external reference \"{name}\": {why}")))
    }

    /// Whether an entity of xref content could exist. Copies are allowed to
    /// hold hatches, construction lines and rays, unlike host blocks.
    fn xref_entity_ok(&self, entity: &Entity) -> bool {
        match entity {
            Entity::Ellipse(e) => e.validate().is_ok(),
            Entity::Spline(s) => s.validate().is_ok(),
            Entity::Polyline {
                points,
                bulges,
                widths,
                ..
            } => crate::polyline::check(points, bulges, widths).is_ok(),
            Entity::Hatch(h) => h.validate().is_ok(),
            Entity::Insert(i) => i.is_valid() && !i.is_xref && !self.is_xref_block(i.block.id),
            Entity::MText(t) => t.validate().is_ok() && self.text_style(t.style).is_some(),
            Entity::Text(t) => t.validate().is_ok() && self.text_style(t.style()).is_some(),
            _ => true,
        }
    }

    /// The expanded size of one xref block's contents, or why it cannot be
    /// walked (a cycle, a missing local block, or it nests too deep).
    fn xref_subtree(
        &self,
        def: &BlockDef,
        locals: &HashMap<BlockId, Arc<BlockDef>>,
        depth: usize,
        stack: &mut Vec<BlockId>,
        memo: &mut HashMap<BlockId, u64>,
    ) -> Result<u64, String> {
        if let Some(size) = memo.get(&def.id) {
            return Ok(*size);
        }
        if depth > MAX_NESTING {
            return Err("its blocks nest too deep".to_owned());
        }
        if stack.contains(&def.id) {
            return Err(format!("block \"{}\" contains itself", def.name));
        }
        stack.push(def.id);
        let mut total = def.entities.len() as u64;
        for child in &def.entities {
            let Entity::Insert(i) = &child.entity else {
                continue;
            };
            // A local definition of this reference, or a block XBIND made
            // local while the reference stayed.
            let target = match locals.get(&i.block.id) {
                Some(local) => local,
                None => match self.block(i.block.id) {
                    Some(host) if !self.is_xref_block(i.block.id) => host,
                    _ => {
                        stack.pop();
                        return Err(format!(
                            "an insert points at block {}, which is not part of the reference",
                            i.block.id.0
                        ));
                    }
                },
            };
            let size = self.xref_subtree(target, locals, depth + 1, stack, memo)?;
            total = total.saturating_add(size);
        }
        stack.pop();
        if total > MAX_EXPANSION as u64 {
            return Err("its content expands past the limit".to_owned());
        }
        memo.insert(def.id, total);
        Ok(total)
    }
}
