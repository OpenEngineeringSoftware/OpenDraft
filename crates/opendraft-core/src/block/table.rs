//! Rules of the block table: names, redefinition, and what makes a table
//! valid.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::block::{BlockDef, BlockEntity, BlockId, BlockObjectId, Insert};
use crate::validate::DocumentError;
use crate::{Document, Entity, EntityId, Object};

/// Characters AutoCAD does not allow in a symbol name.
const FORBIDDEN: &[char] = &[
    '<', '>', '/', '\\', '"', ':', ';', '?', '*', '|', ',', '=', '`',
];

/// Longest block name AutoCAD accepts, in characters.
const MAX_NAME: usize = 255;

/// Why `name` cannot be a block's name, or `None` when it can. Leading and
/// trailing spaces are not part of a name; callers trim them.
pub fn block_name_problem(name: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        Some("A block name cannot be empty.")
    } else if name.chars().count() > MAX_NAME {
        Some("A block name can be at most 255 characters long.")
    } else if name.contains(FORBIDDEN) {
        Some("A block name cannot contain < > / \\ \" : ; ? * | , = or `.")
    } else {
        None
    }
}

impl BlockDef {
    /// A definition holding copies of `objects`, with their layers and
    /// properties, in the drawing's coordinates.
    pub fn from_objects<'a>(
        id: BlockId,
        name: &str,
        base: crate::Point,
        objects: impl IntoIterator<Item = &'a Object>,
    ) -> BlockDef {
        BlockDef {
            unit: Default::default(),
            id,
            name: name.trim().to_owned(),
            base,
            description: String::new(),
            entities: objects
                .into_iter()
                .enumerate()
                .map(|(i, o)| {
                    BlockEntity::new(BlockObjectId(i as u64), o.entity.clone(), o.attrs.clone())
                })
                .collect(),
            dynamic: None,
        }
    }
}

/// What redefining a block changes: the new block table, and the objects
/// whose inserts now point at new records.
#[derive(Clone, Debug)]
pub struct Redefinition {
    pub blocks: Vec<Arc<BlockDef>>,
    pub objects: Vec<(EntityId, Entity)>,
}

impl Document {
    /// The table with `def` added, or replacing the block with its id, and
    /// every definition and insert that holds that block rebuilt to point at
    /// the new record. Refused when the new contents insert the block
    /// itself, which would nest it forever.
    pub fn redefined_blocks(&self, def: BlockDef) -> Result<Redefinition, &'static str> {
        let target = def.id;
        // The strokes of a pattern would have to be drawn and moved with the
        // block; that is not built.
        if def
            .entities
            .iter()
            .any(|e| matches!(e.entity, Entity::Hatch(_)))
        {
            return Err("A block cannot hold a hatch yet.");
        }
        // They have no ends to give the block a size.
        if def
            .entities
            .iter()
            .any(|e| matches!(e.entity, Entity::Xline { .. } | Entity::Ray { .. }))
        {
            return Err("A block cannot hold a construction line or a ray.");
        }
        let loops = def.entities.iter().any(|e| match &e.entity {
            Entity::Insert(i) => self
                .block(i.block.id)
                .map_or(i.block.holds(target), |b| b.holds(target)),
            _ => false,
        });
        if loops {
            return Err("A block cannot contain an insert of itself.");
        }
        let mut done: HashMap<BlockId, Arc<BlockDef>> = HashMap::new();
        done.insert(target, Arc::new(def));
        let mut blocks: Vec<Arc<BlockDef>> = self
            .blocks()
            .iter()
            .map(|b| self.repointed(b, target, &mut done))
            .collect();
        if self.block(target).is_none() {
            blocks.extend(done.get(&target).cloned());
        }
        let objects = self
            .objects()
            .iter()
            .filter_map(|o| match &o.entity {
                Entity::Insert(i) if i.block.holds(target) => {
                    let block = self.repointed(&i.block, target, &mut done);
                    Some((o.id, Entity::Insert(Box::new(i.rebound(block)))))
                }
                _ => None,
            })
            .collect();
        Ok(Redefinition { blocks, objects })
    }

    /// The table with block `id` called `name`, and every insert of it (in
    /// the drawing or inside other blocks) pointing at the renamed record.
    /// Only the definition's name changes, so every reference follows with
    /// its attributes and dynamic values. Refused for an unknown block, a
    /// name a block cannot have (an anonymous `*` name among them), and a
    /// name another block has, ignoring case; changing only the case is
    /// allowed.
    pub fn renamed_block(&self, id: BlockId, name: &str) -> Result<Redefinition, String> {
        let name = name.trim();
        let Some(old) = self.block(id) else {
            return Err("That block is no longer in the drawing.".to_owned());
        };
        if let Some(problem) = block_name_problem(name) {
            return Err(problem.to_owned());
        }
        if let Some(other) = self.block_named(name).filter(|b| b.id != id) {
            return Err(format!("A block called \"{}\" already exists.", other.name));
        }
        let mut def = (**old).clone();
        def.name = name.to_owned();
        self.redefined_blocks(def).map_err(str::to_owned)
    }

    /// `def` rebuilt so that every insert of `target` inside it, at any
    /// depth, uses the record in `done`.
    fn repointed(
        &self,
        def: &Arc<BlockDef>,
        target: BlockId,
        done: &mut HashMap<BlockId, Arc<BlockDef>>,
    ) -> Arc<BlockDef> {
        if let Some(new) = done.get(&def.id) {
            return new.clone();
        }
        if !def.holds(target) {
            return def.clone();
        }
        let mut new = (**def).clone();
        for child in &mut new.entities {
            if let Entity::Insert(i) = &mut child.entity {
                let current = self.block(i.block.id).cloned().unwrap_or(i.block.clone());
                let block = self.repointed(&current, target, done);
                **i = i.rebound(block);
            }
        }
        let new = Arc::new(new);
        done.insert(def.id, new.clone());
        new
    }

    /// Unique ids and names, valid names, every object of every definition
    /// with an id of its own in the block and on a real layer, and every insert (in the drawing or in a block)
    /// pointing at the table's own record for its block.
    pub(crate) fn validate_blocks(&self) -> Result<(), DocumentError> {
        let bad = |b: &BlockDef| Err(DocumentError::BadBlock(b.name.clone()));
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for block in self.blocks() {
            let unique = ids.insert(block.id) && names.insert(block.name.to_lowercase());
            if !unique || block_name_problem(&block.name).is_some() {
                return bad(block);
            }
            if block.id.0 >= self.next_block_id() {
                return Err(DocumentError::CounterBehind);
            }
            let mut object_ids = HashSet::new();
            for child in &block.entities {
                if !object_ids.insert(child.id) {
                    return bad(block);
                }
                let layer_ok = self.layer(child.attrs.layer).is_some();
                let shape_ok = match &child.entity {
                    Entity::Insert(i) => {
                        i.block.id != block.id
                            && self.is_current(i)
                            && (i.dynamic.is_none() || i.block.dynamic.is_some())
                    }
                    Entity::Ellipse(e) => e.validate().is_ok(),
                    Entity::Spline(s) => s.validate().is_ok(),
                    Entity::Polyline {
                        points,
                        bulges,
                        widths,
                        ..
                    } => crate::polyline::check(points, bulges, widths).is_ok(),
                    // A block cannot hold a hatch: the strokes of a pattern
                    // would have to be drawn and moved with it.
                    Entity::Hatch(_) | Entity::Xline { .. } | Entity::Ray { .. } => false,
                    _ => true,
                };
                if !layer_ok || !shape_ok {
                    return bad(block);
                }
            }
            if let Some(schema) = &block.dynamic {
                schema
                    .check(&object_ids)
                    .map_err(|why| DocumentError::BadDynamicBlock(block.name.clone(), why))?;
            }
        }
        for object in self.objects() {
            if let Entity::Insert(i) = &object.entity {
                if !self.is_current(i) {
                    return Err(DocumentError::BadInsert(object.id));
                }
                if let Some(state) = &i.dynamic {
                    let checked = match &i.block.dynamic {
                        Some(schema) => schema.check_state(state),
                        None => Err("the block has no parameters".to_owned()),
                    };
                    checked.map_err(|why| DocumentError::BadDynamicState(object.id, why))?;
                }
            }
        }
        Ok(())
    }

    /// Whether an insert is well formed and uses the table's record.
    fn is_current(&self, insert: &Insert) -> bool {
        insert.is_valid()
            && self
                .block(insert.block.id)
                .is_some_and(|b| Arc::ptr_eq(b, &insert.block) || **b == *insert.block)
    }
}
