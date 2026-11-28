//! Building a [`Document`] from the file's drawing, refusing what cannot exist.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use opendraft_core::{
    Attributes, BlockDef, Document, Entity, EntityId, Layer, LayerId, LayoutId, LinetypeId,
    Lineweight, Object,
};

use super::block::{load_blocks_with, Blocks};
use super::entity::{from_point, invalid};
use super::layout::{into_layouts, LayoutDto};
use super::{attr_from_dto, text, Color, DrawingDto};
use crate::FileError;

impl DrawingDto {
    /// How many objects are built between looks at `cancelled`.
    const CANCEL_CHECK_EVERY: usize = 1024;

    pub(super) fn into_document(self, cancelled: &AtomicBool) -> Result<Document, FileError> {
        let mut document = Document::default();
        document.begin_bulk_load();

        let mut layer_ids = HashSet::new();
        for layer in self.layers {
            if !layer_ids.insert(layer.id) {
                return invalid(format!("layer {} appears twice", layer.id));
            }
            if layer.name.trim().is_empty() {
                return invalid(format!("layer {} has no name", layer.id));
            }
            let layer = Layer {
                id: LayerId(layer.id),
                name: layer.name,
                color: layer.color.into(),
                linetype: LinetypeId(layer.linetype),
                lineweight: Lineweight(layer.lineweight),
                on: layer.on,
                frozen: layer.frozen,
                locked: layer.locked,
                frozen_in_model: layer.frozen_in_model,
                plot: !layer.no_plot,
            };
            // The drawing always has layer 0, first; the file's version of
            // it replaces the stock one.
            if layer.id == LayerId::DEFAULT {
                document.replace_layer(layer);
            } else {
                document.push_layer(layer);
            }
        }
        if !layer_ids.contains(&LayerId::DEFAULT.0) {
            return invalid("there is no layer 0");
        }

        let dim_styles = self.dim_styles.read()?;
        text::load_styles(
            &mut document,
            &self.text_styles,
            self.current_text_style,
            self.next_text_style_id,
        )?;
        self.mleader_styles.load(&mut document)?;
        self.linetypes.load(&mut document)?;
        self.new_style.load(&mut document);
        self.point_style.load(&mut document);
        self.modify_settings.load(&mut document);
        self.ccs.load(&mut document)?;
        // Local definitions first: a root's snapshot inserts them, and they
        // are not host blocks.
        let mut local_map: Blocks = Blocks::new();
        let mut local_blocks: Vec<Vec<Arc<BlockDef>>> = Vec::new();
        for xref in &self.xrefs {
            let locals = load_blocks_with(
                xref.local_blocks(),
                &dim_styles.pool,
                |l| layer_ids.contains(&l),
                false,
                &Blocks::new(),
            )?;
            for block in &locals {
                if local_map.insert(block.id.0, block.clone()).is_some() {
                    return invalid(format!(
                        "block {} belongs to two external references",
                        block.id.0
                    ));
                }
            }
            local_blocks.push(locals);
        }
        let blocks = load_blocks_with(
            &self.blocks,
            &dim_styles.pool,
            |l| layer_ids.contains(&l),
            true,
            &local_map,
        )?;
        let by_id: Blocks = blocks.iter().map(|b| (b.id.0, b.clone())).collect();
        let mut records = Vec::with_capacity(self.xrefs.len());
        for (xref, locals) in self.xrefs.into_iter().zip(local_blocks) {
            records.push(xref.into_record(&by_id, locals)?);
        }
        for block in blocks {
            document.push_block(block);
        }
        document.set_xrefs(records);
        document.set_insert_base(from_point(self.insert_base)?);
        document.set_visretain(self.visretain);
        document.reserve_block_ids(self.next_block_id);
        document.reserve_attribute_definition_ids(self.next_attribute_definition_id);
        let layout_ids: HashSet<u64> = self.layouts.iter().map(LayoutDto::id).collect();
        let mut object_ids = HashSet::new();
        for (index, object) in self.objects.into_iter().enumerate() {
            if index % Self::CANCEL_CHECK_EVERY == 0 && cancelled.load(Ordering::Relaxed) {
                return Err(FileError::Cancelled);
            }
            if object.layout.is_some_and(|l| !layout_ids.contains(&l)) {
                return invalid(format!(
                    "object {} is on a layout that does not exist",
                    object.id
                ));
            }
            if !object_ids.insert(object.id) {
                return invalid(format!("object {} appears twice", object.id));
            }
            if !layer_ids.contains(&object.layer) {
                return invalid(format!(
                    "object {} is on layer {}, which does not exist",
                    object.id, object.layer
                ));
            }
            let entity = object.entity.to_entity(&by_id, &dim_styles.pool)?;
            if let Entity::MText(text) = &entity {
                if document.text_style(text.style).is_none() {
                    return invalid(format!(
                        "object {} uses a text style that does not exist",
                        object.id
                    ));
                }
            }
            if let Entity::Dimension(d) = &entity {
                if document.text_style(d.style.text_style).is_none() {
                    return invalid(format!(
                        "object {} uses a text style that does not exist",
                        object.id
                    ));
                }
            }
            let mut attrs = Attributes {
                layer: LayerId(object.layer),
                color: attr_from_dto(object.color, Color::from),
                linetype: attr_from_dto(object.linetype, LinetypeId),
                lineweight: attr_from_dto(object.lineweight, Lineweight),
                ..Attributes::on_layer(LayerId(object.layer))
            };
            object.scale.apply(&mut attrs);
            document.push(Object::new(
                EntityId(object.id),
                entity,
                attrs,
                object.layout.map(LayoutId),
            ));
        }

        self.arrays.load(&mut document, &object_ids)?;
        self.polyline_curves.load(&mut document, &object_ids);
        document.reserve_ids(self.next_object_id, self.next_layer_id);
        if !layer_ids.contains(&self.current_layer) {
            return invalid(format!(
                "the current layer {} does not exist",
                self.current_layer
            ));
        }
        document.set_current_layer(LayerId(self.current_layer));
        document.set_units(self.units.into_units()?);
        document.set_properties(self.properties.into_properties()?);
        dim_styles.install(&mut document)?;
        document.set_layouts(into_layouts(self.layouts)?);
        if let Some(dto) = self.model_plot {
            let settings = dto.into_settings();
            if !settings.is_valid() {
                return invalid("the Model tab's plot settings are not usable".to_owned());
            }
            document.set_model_plot(settings);
        }
        document.reserve_layout_ids(self.next_layout_id, self.next_viewport_id);
        document.finish_bulk_load();
        Ok(document)
    }
}
