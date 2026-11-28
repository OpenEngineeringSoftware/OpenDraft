//! The planner's block walk: the definitions a source's model space reaches,
//! their local ids, and the content of the root and of each definition.

use std::sync::Arc;

use opendraft_core::block::MAX_NESTING;
use opendraft_core::{BlockDef, BlockEntity, BlockId, BlockObjectId, Entity};

use super::super::remap::Remap;
use super::{Planner, XrefPlanError};

impl Planner<'_> {
    pub(super) fn collect(
        &mut self,
        def: &Arc<BlockDef>,
        depth: usize,
        order: &mut Vec<Arc<BlockDef>>,
    ) -> Result<(), XrefPlanError> {
        if self.block_ids.contains_key(&def.id) {
            return Ok(());
        }
        if depth > MAX_NESTING {
            return Err(XrefPlanError::BrokenSource(format!(
                "block \"{}\" nests too deeply",
                def.name
            )));
        }
        if !self.visiting.insert(def.id) {
            return Err(XrefPlanError::BrokenSource(format!(
                "block \"{}\" contains itself",
                def.name
            )));
        }
        for entity in &def.entities {
            let Entity::Insert(insert) = &entity.entity else {
                continue;
            };
            self.check()?;
            let target = insert.block.clone();
            if self.source.is_xref_block(target.id) {
                self.nested += 1;
                continue;
            }
            if self.source.block(target.id).is_none() {
                self.skip("insert of a missing block");
                continue;
            }
            self.collect(&target, depth + 1, order)?;
        }
        self.visiting.remove(&def.id);
        if self.block_ids.len() as u64 >= self.limits.max_local_blocks {
            return Err(XrefPlanError::TooMany {
                what: "blocks in the source",
                limit: self.limits.max_local_blocks,
            });
        }
        let id = self
            .next_block
            .checked_add(order.len() as u64)
            .map(BlockId)
            .ok_or(XrefPlanError::TooMany {
                what: "blocks in the source",
                limit: self.limits.max_local_blocks,
            })?;
        self.block_ids.insert(def.id, id);
        order.push(def.clone());
        Ok(())
    }

    pub(super) fn collect_model(
        &mut self,
        order: &mut Vec<Arc<BlockDef>>,
    ) -> Result<(), XrefPlanError> {
        let source = self.source;
        let mut roots = Vec::new();
        for object in source.model_objects() {
            if let Entity::Insert(insert) = &object.entity {
                let target = insert.block.clone();
                if self.source.is_xref_block(target.id) {
                    self.nested += 1;
                    continue;
                }
                if self.source.block(target.id).is_none() {
                    self.skip("insert of a missing block");
                    continue;
                }
                roots.push(target);
            }
        }
        for target in roots {
            self.collect(&target, 0, order)?;
        }
        Ok(())
    }

    pub(super) fn build_locals(&mut self, order: &[Arc<BlockDef>]) -> Result<(), XrefPlanError> {
        for def in order {
            let Some(id) = self.block_ids.get(&def.id).copied() else {
                continue;
            };
            let mut entities = Vec::with_capacity(def.entities.len());
            for entity in &def.entities {
                self.check()?;
                if let Entity::Insert(insert) = &entity.entity {
                    if self.source.is_xref_block(insert.block.id) {
                        continue;
                    }
                }
                let made = {
                    let remap = Remap {
                        layers: &self.layers,
                        linetypes: &self.linetypes,
                        styles: &self.styles,
                        blocks: &self.blocks,
                        block_ids: &self.block_ids,
                        xref_name: &self.name,
                    };
                    remap
                        .entity(&entity.entity)
                        .map(|made| (made, remap.attributes(&entity.attrs)))
                };
                let Some((made, attrs)) = made else {
                    self.skip("object of a missing block");
                    continue;
                };
                entities.push(BlockEntity::new(entity.id, made, attrs));
                if matches!(
                    entity.entity,
                    Entity::Dimension(_) | Entity::Leader(_) | Entity::Hatch(_)
                ) {
                    self.cleared_associations = true;
                }
                if matches!(entity.entity, Entity::Insert(ref i) if i.dynamic.is_some()) {
                    self.dynamic_states = true;
                }
            }
            let block = Arc::new(BlockDef {
                id,
                name: def.name.clone(),
                base: def.base,
                description: def.description.clone(),
                unit: def.unit,
                entities,
                dynamic: None,
            });
            self.blocks.insert(def.id, block);
        }
        Ok(())
    }

    pub(super) fn root_entities(&mut self) -> Result<Vec<BlockEntity>, XrefPlanError> {
        let mut out = Vec::new();
        let mut index = 0u64;
        let source = self.source;
        for object in source.model_objects() {
            self.check()?;
            if let Entity::Insert(insert) = &object.entity {
                if self.source.is_xref_block(insert.block.id) {
                    continue;
                }
            }
            if out.len() as u64 >= self.limits.max_model_entities {
                return Err(XrefPlanError::TooMany {
                    what: "objects in the source's model space",
                    limit: self.limits.max_model_entities,
                });
            }
            let made = {
                let remap = Remap {
                    layers: &self.layers,
                    linetypes: &self.linetypes,
                    styles: &self.styles,
                    blocks: &self.blocks,
                    block_ids: &self.block_ids,
                    xref_name: &self.name,
                };
                remap
                    .entity(&object.entity)
                    .map(|made| (made, remap.attributes(&object.attrs)))
            };
            let Some((made, attrs)) = made else {
                self.skip("object of a missing block");
                continue;
            };
            if matches!(
                object.entity,
                Entity::Dimension(_) | Entity::Leader(_) | Entity::Hatch(_)
            ) {
                self.cleared_associations = true;
            }
            if matches!(object.entity, Entity::Insert(ref i) if i.dynamic.is_some()) {
                self.dynamic_states = true;
            }
            out.push(BlockEntity::new(BlockObjectId(index), made, attrs));
            index += 1;
        }
        Ok(out)
    }
}
