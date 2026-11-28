//! The drawing's external-reference table: the accessors, the edits an
//! action makes, and the invariants `validate` checks.
//!
//! The table is keyed by the root block's id. Local definitions belong to
//! the record, not to the block table, and are reached from the root's
//! inserts by `Arc`.

use std::sync::Arc;

use super::{after, Document};
use crate::block::{BlockDef, BlockId, Insert, MAX_NESTING};
use crate::xref::XrefDef;
use crate::{Entity, EntityId, LayerId, Object, Point};

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

    /// The clip frame a clipped reference plots when `XCLIPFRAME` is 2, as a
    /// closed polyline in drawing coordinates; `None` for any other object.
    pub fn clip_frame(&self, object: &Object) -> Option<Object> {
        if self.xclip_frame() != 2 {
            return None;
        }
        let Entity::Insert(insert) = &object.entity else {
            return None;
        };
        let clip = self
            .xref(insert.block.id)?
            .clip
            .as_ref()
            .filter(|clip| clip.enabled)?;
        let transform = insert.transform();
        let points = clip
            .boundary
            .iter()
            .map(|point| transform.apply(*point))
            .collect();
        Some(Object::new(
            object.id,
            Entity::Polyline {
                points,
                closed: true,
                bulges: Vec::new(),
                widths: Vec::new(),
            },
            object.attrs.clone(),
            object.layout,
        ))
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

    /// `XCLIPFRAME`: 0 hides clip frames, 1 draws them, 2 draws and plots them.
    pub fn xclip_frame(&self) -> u8 {
        self.xclip_frame
    }

    /// Set `XCLIPFRAME`; values past 2 are read as 2.
    pub fn set_xclip_frame(&mut self, value: u8) -> u8 {
        std::mem::replace(&mut self.xclip_frame, value.min(2))
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
}
