//! The block table: block definitions by id and name.

use std::borrow::Cow;
use std::sync::Arc;

use super::{after, Document};
use crate::block::{BlockDef, BlockId};
use crate::{Entity, EntityId, Object};

impl Document {
    /// Block definitions in table order (the order they were made).
    pub fn blocks(&self) -> &[Arc<BlockDef>] {
        &self.blocks
    }

    pub fn block(&self, id: BlockId) -> Option<&Arc<BlockDef>> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// The block called `name`, ignoring case as AutoCAD does.
    pub fn block_named(&self, name: &str) -> Option<&Arc<BlockDef>> {
        let name = name.trim();
        self.blocks
            .iter()
            .find(|b| b.name.eq_ignore_ascii_case(name))
    }

    /// An id no block has had or will have.
    pub fn allocate_block_id(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id = after(self.next_block_id);
        id
    }

    /// The next block id that would be handed out.
    pub fn next_block_id(&self) -> u64 {
        self.next_block_id
    }

    /// Never hand out a block id below `next` (a loaded drawing keeps its
    /// counter). Only ever raises it.
    pub fn reserve_block_ids(&mut self, next: u64) {
        self.next_block_id = self.next_block_id.max(next);
    }

    /// Append a definition that already has an id (a file being read).
    pub fn push_block(&mut self, block: Arc<BlockDef>) {
        self.next_block_id = self.next_block_id.max(after(block.id.0));
        self.note_block_attribute_ids(&block);
        self.blocks.push(block);
    }

    /// Replace the whole table, returning the old one (the undo action
    /// swaps them back).
    pub fn set_blocks(&mut self, blocks: Vec<Arc<BlockDef>>) -> Vec<Arc<BlockDef>> {
        if let Some(top) = blocks.iter().map(|b| after(b.id.0)).max() {
            self.next_block_id = self.next_block_id.max(top);
        }
        for block in &blocks {
            self.note_block_attribute_ids(block);
        }
        std::mem::replace(&mut self.blocks, blocks)
    }

    /// Objects of any space that insert `id`, directly or through other
    /// blocks.
    pub fn block_references(&self, id: BlockId) -> Vec<EntityId> {
        self.objects
            .iter()
            .filter(|o| matches!(&o.entity, Entity::Insert(i) if i.block.holds(id)))
            .map(|o| o.id)
            .collect()
    }

    /// For a block reference, what it draws as objects of their own: its
    /// pieces in drawing coordinates, on their inherited layers, with the
    /// reference's id and space, leaving out pieces on hidden layers. `None`
    /// for any other kind of object.
    pub fn insert_pieces(&self, object: &Object) -> Option<Vec<Object>> {
        let Entity::Insert(insert) = &object.entity else {
            return None;
        };
        let pieces = insert
            .picture_with(&object.attrs, &|l| self.layer_look(l))
            .into_iter()
            .map(|p| Object::new(object.id, p.entity, p.attrs, object.layout));
        Some(pieces.filter(|o| self.is_visible(o)).collect())
    }

    /// The object as drawn: itself, or the pieces of a block reference
    /// (see [`insert_pieces`](Self::insert_pieces)).
    pub fn drawn<'a>(&self, object: &'a Object) -> Vec<Cow<'a, Object>> {
        match self.insert_pieces(object) {
            Some(pieces) => pieces.into_iter().map(Cow::Owned).collect(),
            None => vec![Cow::Borrowed(object)],
        }
    }

    /// The drawing the Block Editor works on for block `id`: this drawing's
    /// tables (layers, text styles, blocks, linetypes, units, dimension style) and id
    /// counters, no layouts, and as its only objects the block's, moved so
    /// the base point is the origin, as AutoCAD's Block Editor shows it.
    /// `None` for an unknown block.
    pub fn block_editor_document(&self, id: BlockId) -> Option<Document> {
        // An external reference is edited in its source drawing.
        if self.is_xref_block(id) {
            return None;
        }
        let block = self.block(id)?.clone();
        let mut doc = Document {
            objects: Vec::new(),
            positions: Default::default(),
            layouts: Vec::new(),
            active_layout: None,
            active_frozen: Vec::new(),
            hidden_objects: Vec::new(),
            active_viewport: None,
            view_twist: 0.0,
            model_ccs: crate::ccs::Ccs::WORLD,
            ccs_previous: Vec::new(),
            ..self.clone_tables()
        };
        // The schema moves with the geometry, to the origin.
        doc.block_editor_dynamic = block
            .dynamic
            .as_deref()
            .map(|d| d.translated(-block.base.x, -block.base.y));
        for e in &block.entities {
            let entity = e.entity.translated(-block.base.x, -block.base.y);
            // An object keeps its block id as its id here, and BSAVE reads it
            // back, so an action that names it still does after an edit.
            doc.add_with_id(EntityId(e.id.0), entity, e.attrs.clone());
        }
        Some(doc)
    }

    /// Block `id` as a drawing of its own, the way AutoCAD's WBLOCK writes a
    /// file: the block's objects are the drawing's, where they are (the base
    /// point is not moved to the origin), the blocks they nest stay as
    /// definitions, the block itself is not a definition, and the insert unit
    /// is the block's. The caller keeps the base point for `$INSBASE`. `None`
    /// for an unknown block.
    pub fn block_as_drawing(&self, id: BlockId) -> Option<Document> {
        // Writing an external reference out as a block would flatten it.
        if self.is_xref_block(id) {
            return None;
        }
        let block = self.block(id)?.clone();
        let mut doc = Document {
            objects: Vec::new(),
            positions: Default::default(),
            layouts: Vec::new(),
            active_layout: None,
            active_frozen: Vec::new(),
            hidden_objects: Vec::new(),
            active_viewport: None,
            view_twist: 0.0,
            model_ccs: crate::ccs::Ccs::WORLD,
            ccs_previous: Vec::new(),
            ..self.clone_tables()
        };
        doc.blocks.retain(|b| b.id != id);
        doc.units.insert = block.unit;
        for e in &block.entities {
            doc.add_with_id(EntityId(e.id.0), e.entity.clone(), e.attrs.clone());
        }
        Some(doc)
    }

    /// The dynamic schema a Block Editor drawing is authoring: `None` for a
    /// drawing, and for an editor of an ordinary block.
    pub fn block_editor_dynamic(&self) -> Option<&crate::block::DynamicBlock> {
        self.block_editor_dynamic.as_ref()
    }

    /// Replace the authoring schema, returning the old one (the undo action
    /// swaps them back).
    pub fn set_block_editor_dynamic(
        &mut self,
        schema: Option<crate::block::DynamicBlock>,
    ) -> Option<crate::block::DynamicBlock> {
        std::mem::replace(&mut self.block_editor_dynamic, schema)
    }

    /// A copy of everything but the objects, which can be many.
    fn clone_tables(&self) -> Document {
        Document {
            objects: Vec::new(),
            next_id: self.next_id,
            positions: Default::default(),
            layers: self.layers.clone(),
            layer_positions: self.layer_positions.clone(),
            next_layer_id: self.next_layer_id,
            current_layer: self.current_layer,
            new_color: self.new_color.clone(),
            new_linetype: self.new_linetype.clone(),
            new_lineweight: self.new_lineweight.clone(),
            units: self.units,
            properties: self.properties.clone(),
            dim_style: self.dim_style.clone(),
            dim_styles: self.dim_styles.clone(),
            layouts: self.layouts.clone(),
            model_plot: self.model_plot.clone(),
            next_layout_id: self.next_layout_id,
            next_viewport_id: self.next_viewport_id,
            active_layout: self.active_layout,
            active_frozen: self.active_frozen.clone(),
            hidden_objects: self.hidden_objects.clone(),
            active_viewport: self.active_viewport,
            view_twist: self.view_twist,
            model_ccs: self.model_ccs,
            named_ccs: self.named_ccs.clone(),
            ccs_previous: self.ccs_previous.clone(),
            text_styles: self.text_styles.clone(),
            next_text_style_id: self.next_text_style_id,
            current_text_style: self.current_text_style,
            mleader_styles: self.mleader_styles.clone(),
            next_mleader_style_id: self.next_mleader_style_id,
            current_mleader_style: self.current_mleader_style,
            blocks: self
                .blocks
                .iter()
                .filter(|b| self.xref(b.id).is_none())
                .cloned()
                .collect(),
            next_block_id: self.next_block_id,
            xrefs: Vec::new(),
            insert_base: self.insert_base,
            visretain: self.visretain,
            block_editor_dynamic: None,
            next_attribute_definition_id: self.next_attribute_definition_id,
            linetypes: self.linetypes.clone(),
            linetype_scale: self.linetype_scale,
            point_style: self.point_style,
            modify_settings: self.modify_settings,
            arrays: Vec::new(),
            polyline_curves: Vec::new(),
            spatial: Default::default(),
        }
    }

    /// Whether anything inserts the block: an object in the drawing or
    /// another block's definition. A block in use cannot be purged.
    pub fn block_in_use(&self, id: BlockId) -> bool {
        !self.block_references(id).is_empty()
            || self
                .blocks
                .iter()
                .any(|b| b.id != id && b.nested_blocks().any(|n| n == id))
    }
}
