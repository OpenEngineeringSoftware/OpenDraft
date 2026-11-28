//! The plan: tables, blocks, objects and layouts after binding.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::block::{BlockDef, BlockId};
use crate::xref_bind::BindPlan;
use crate::{Attributes, BlockEntity, Entity, LayerId, Layout};

use super::Binder;

impl Binder<'_> {
    pub(super) fn finish(self) -> BindPlan {
        let map = self.layer_merge.clone();
        let mut changed: HashSet<BlockId> = self.moving.keys().copied().collect();
        for block in self.doc.blocks() {
            if uses_merged_layer(block, &map)
                || block.entities.iter().any(
                    |e| matches!(&e.entity, Entity::Insert(i) if changed.contains(&i.block.id)),
                )
            {
                changed.insert(block.id);
            }
        }
        loop {
            let mut grew = false;
            for block in self.doc.blocks() {
                if changed.contains(&block.id) {
                    continue;
                }
                if block.entities.iter().any(
                    |e| matches!(&e.entity, Entity::Insert(i) if changed.contains(&i.block.id)),
                ) {
                    changed.insert(block.id);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }

        let mut final_map: HashMap<BlockId, Arc<BlockDef>> = self
            .doc
            .blocks()
            .iter()
            .map(|b| (b.id, b.clone()))
            .collect();
        for block in self.doc.blocks() {
            if changed.contains(&block.id) {
                final_map.insert(block.id, rebuild(block, block.name.clone(), &map));
            }
        }
        for (&id, name) in &self.moving {
            if let Some(def) = self.doc.xref_blocks().find(|b| b.id == id).cloned() {
                final_map.insert(id, rebuild(&def, name.clone(), &map));
            }
        }
        // Every rebuilt definition points at the rebuilt definitions.
        let ids: Vec<BlockId> = final_map.keys().copied().collect();
        for id in ids {
            if !changed.contains(&id) {
                continue;
            }
            let block = final_map[&id].clone();
            let entities = block
                .entities
                .iter()
                .map(|e| {
                    let mut entity = e.entity.clone();
                    if let Entity::Insert(i) = &mut entity {
                        if let Some(target) = final_map.get(&i.block.id) {
                            i.block = target.clone();
                        }
                    }
                    BlockEntity::new(e.id, entity, e.attrs.clone())
                })
                .collect();
            let made = Arc::new(BlockDef {
                id: block.id,
                name: block.name.clone(),
                base: block.base,
                description: block.description.clone(),
                unit: block.unit,
                entities,
                dynamic: block.dynamic.clone(),
            });
            final_map.insert(id, made);
        }

        let mut blocks: Vec<Arc<BlockDef>> = self
            .doc
            .blocks()
            .iter()
            .filter_map(|b| final_map.get(&b.id).cloned())
            .collect();
        for id in self.moving_order() {
            if let Some(def) = final_map.get(&id) {
                blocks.push(def.clone());
            }
        }

        let mut entities = Vec::new();
        let mut attributes = Vec::new();
        for object in self.doc.objects() {
            let attrs = remap_attrs(&object.attrs, &map);
            if attrs != object.attrs {
                attributes.push((object.id, attrs));
            }
            let mut entity = remap_entity(&object.entity, &map);
            if let Entity::Insert(i) = &mut entity {
                if let Some(target) = final_map.get(&i.block.id) {
                    i.block = target.clone();
                }
                if self.dropped.contains(&i.block.id) {
                    i.is_xref = false;
                }
            }
            if entity != object.entity {
                entities.push((object.id, entity));
            }
        }

        let mut xrefs = self.doc.xrefs().to_vec();
        xrefs.retain(|x| !self.dropped.contains(&x.block));
        for (xref, layers) in &self.unbound_layers {
            if let Some(record) = xrefs.iter_mut().find(|x| x.block == *xref) {
                record.layers.retain(|l| !layers.contains(&l.layer));
            }
        }
        for (xref, blocks) in &self.unbound_blocks {
            if let Some(record) = xrefs.iter_mut().find(|x| x.block == *xref) {
                record.blocks.retain(|b| !blocks.contains(&b.id));
            }
        }

        let mut layers = Vec::new();
        let mut removed_layers: Vec<LayerId> = self.layer_merge.keys().copied().collect();
        removed_layers.sort();
        for layer in self.doc.layers() {
            if self.layer_merge.contains_key(&layer.id) {
                continue;
            }
            let mut made = layer.clone();
            if let Some(name) = self.layer_rename.get(&layer.id) {
                made.name = name.clone();
            }
            if made != *layer {
                layers.push(made);
            }
        }

        let text_styles = (!self.style_rename.is_empty()).then(|| {
            self.doc
                .text_styles()
                .iter()
                .map(|style| {
                    let mut made = style.clone();
                    if let Some(name) = self.style_rename.get(&style.id) {
                        made.name = name.clone();
                    }
                    made
                })
                .collect()
        });
        let linetypes = (!self.linetype_rename.is_empty()).then(|| {
            let mut table = self.doc.linetypes().clone();
            for (id, name) in &self.linetype_rename {
                if let Some(record) = table.get(*id).cloned() {
                    let mut made = record;
                    made.name = name.clone();
                    let _ = table.replace(made);
                }
            }
            table
        });
        let layouts = self.remap_layouts(&map);

        BindPlan {
            blocks,
            layers,
            removed_layers,
            text_styles,
            current_text_style: self.doc.current_text_style(),
            next_text_style: self.doc.next_text_style_id(),
            linetypes,
            layouts,
            entities,
            attributes,
            xrefs,
            notes: self.notes,
        }
    }

    fn moving_order(&self) -> Vec<BlockId> {
        let mut order = Vec::new();
        for xref in self.doc.xrefs() {
            for block in &xref.blocks {
                if self.moving.contains_key(&block.id) && !order.contains(&block.id) {
                    order.push(block.id);
                }
            }
        }
        for id in self.moving.keys() {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        order
    }

    fn remap_layouts(&self, map: &HashMap<LayerId, LayerId>) -> Option<Vec<Layout>> {
        if map.is_empty() {
            return None;
        }
        let mut changed = false;
        let mut out = self.doc.layouts().to_vec();
        for layout in &mut out {
            if remap_frozen(&mut layout.paper_frozen, map) {
                changed = true;
            }
            for viewport in &mut layout.viewports {
                if remap_frozen(&mut viewport.frozen, map) {
                    changed = true;
                }
            }
        }
        changed.then_some(out)
    }
}

fn rebuild(def: &BlockDef, name: String, map: &HashMap<LayerId, LayerId>) -> Arc<BlockDef> {
    let entities = def
        .entities
        .iter()
        .map(|e| {
            BlockEntity::new(
                e.id,
                remap_entity(&e.entity, map),
                remap_attrs(&e.attrs, map),
            )
        })
        .collect();
    Arc::new(BlockDef {
        id: def.id,
        name,
        base: def.base,
        description: def.description.clone(),
        unit: def.unit,
        entities,
        dynamic: def.dynamic.clone(),
    })
}

fn uses_merged_layer(def: &BlockDef, map: &HashMap<LayerId, LayerId>) -> bool {
    !map.is_empty()
        && def.entities.iter().any(|e| {
            map.contains_key(&e.attrs.layer)
                || matches!(&e.entity, Entity::Insert(i) if i.attributes.iter().any(|a| {
                    a.attrs.as_ref().is_some_and(|x| map.contains_key(&x.layer))
                }))
        })
}

fn remap_attrs(attrs: &Attributes, map: &HashMap<LayerId, LayerId>) -> Attributes {
    let mut out = attrs.clone();
    if let Some(&layer) = map.get(&attrs.layer) {
        out.layer = layer;
    }
    out
}

fn remap_entity(entity: &Entity, map: &HashMap<LayerId, LayerId>) -> Entity {
    let Entity::Insert(insert) = entity else {
        return entity.clone();
    };
    if map.is_empty()
        || !insert
            .attributes
            .iter()
            .any(|a| a.attrs.as_ref().is_some_and(|x| map.contains_key(&x.layer)))
    {
        return entity.clone();
    }
    let mut made = (**insert).clone();
    for attr in &mut made.attributes {
        if let Some(attrs) = &mut attr.attrs {
            *attrs = remap_attrs(attrs, map);
        }
    }
    Entity::Insert(Box::new(made))
}

fn remap_frozen(list: &mut Vec<LayerId>, map: &HashMap<LayerId, LayerId>) -> bool {
    let before = list.clone();
    for layer in list.iter_mut() {
        if let Some(&target) = map.get(layer) {
            *layer = target;
        }
    }
    list.sort();
    list.dedup();
    *list != before
}
