//! Bringing one block, and the blocks it nests, from another drawing, with the
//! caller deciding what happens when a name is taken.
//!
//! A paste matches a block by name and silently uses the target's (`absorb`),
//! which is right for a paste and wrong for a library: a library `valve` can be
//! another shape from the drawing's `valve`. [`Document::bring_block`] copies
//! the block with the same merging of layers, linetypes and text styles (by
//! name, ignoring case; what the target has wins, what it lacks is added), and
//! for each block whose name the target has it uses the target's, adds the copy
//! under a new name, or replaces the target's definition (every reference
//! follows, keeping its place and its attribute values).
//! [`Document::insert_from`] does that and adds a reference, and the whole is
//! undone and done again with [`Document::unplace`] and [`Document::replace_placed`].

use std::collections::HashMap;
use std::sync::Arc;

use super::{Document, Merge, Tables, TransferWarning};
use crate::block::{BlockDef, BlockId, FieldText, Insert};
use crate::layer::Attributes;
use crate::layout::LayoutId;
use crate::{Entity, EntityId, Object, Point};

/// What to do with a block of the source whose name the target already has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockClash {
    /// Leave the target's block as it is and use it.
    UseExisting,
    /// Add the source's block under a new name (`Name_2`).
    #[default]
    Rename,
    /// Give the target's block the source's definition. It keeps its id and
    /// name, and every reference to it follows.
    Replace,
}

/// A block of the source whose name the target has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockNameClash {
    pub source: BlockId,
    pub name: String,
    /// The target's block of that name.
    pub existing: BlockId,
}

/// Why a block could not be brought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BringError {
    /// The source has no such block.
    NoSuchBlock,
    /// The drawing's rules refused it (a block that would hold itself).
    Refused(&'static str),
}

impl std::fmt::Display for BringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BringError::NoSuchBlock => write!(f, "There is no such block."),
            BringError::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for BringError {}

/// What `bring_block` did, so it can be undone and done again.
#[derive(Clone, Debug)]
pub struct Brought {
    /// The target's block that stands for the one asked for.
    pub block: Arc<BlockDef>,
    /// Every source block that was brought or matched, with the target's block
    /// for it: the block asked for and each block it nests, at any depth.
    pub blocks: Vec<(BlockId, Arc<BlockDef>)>,
    pub warnings: Vec<TransferWarning>,
    /// How many references were changed to follow a replaced definition.
    pub rebound: usize,
    before: Tables,
    after: Tables,
    /// The objects `Replace` changed: as they were, and as they are.
    old: Vec<(EntityId, Entity)>,
    new: Vec<(EntityId, Entity)>,
}

impl Brought {
    /// The objects whose reference followed a replaced definition.
    pub fn changed(&self) -> Vec<EntityId> {
        self.old.iter().map(|(id, _)| *id).collect()
    }
}

/// Where and how a block is placed by [`Document::insert_from`].
#[derive(Clone, Debug)]
pub struct LibraryPlacement {
    pub insertion: Point,
    pub scale_x: f64,
    pub scale_y: f64,
    /// Radians counter-clockwise.
    pub rotation: f64,
    /// A value for each non-constant attribute definition, in order. `None`
    /// uses the definitions' defaults.
    pub values: Option<Vec<FieldText>>,
}

/// An insert made by `insert_from`, with what it took to make it.
#[derive(Clone, Debug)]
pub struct Placed {
    pub brought: Brought,
    pub object: Object,
}

impl Document {
    /// A block name no block has, made from `base`: `base_2`, `base_3`...
    /// (unique ignoring case, as the block table requires). `base` itself
    /// when it is free.
    pub fn unused_block_name_like(&self, base: &str) -> String {
        let base = base.trim();
        if self.block_named(base).is_none() {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base}_{n}"))
            .find(|name| self.block_named(name).is_none())
            .unwrap_or_default()
    }

    /// The blocks `root` is made of (itself first, then the blocks it nests,
    /// each once) whose names this drawing already has. `None` when `src` has
    /// no such block.
    pub fn block_clashes(&self, src: &Document, root: BlockId) -> Option<Vec<BlockNameClash>> {
        let root = src.block(root)?;
        let mut clashes = Vec::new();
        for def in blocks_below(root) {
            if let Some(existing) = self.block_named(&def.name) {
                clashes.push(BlockNameClash {
                    source: def.id,
                    name: def.name.clone(),
                    existing: existing.id,
                });
            }
        }
        Some(clashes)
    }

    /// Copies block `root` of `src` into this drawing, with the blocks it
    /// nests. `choices` says, for each source block whose name this drawing
    /// has (`block_clashes` lists them), what to do; a clash with no choice is
    /// renamed. Layers, linetypes and text styles the blocks use are matched
    /// by name or added. On an error the drawing is as it was.
    pub fn bring_block(
        &mut self,
        src: &Document,
        root: BlockId,
        choices: &HashMap<BlockId, BlockClash>,
    ) -> Result<Brought, BringError> {
        let def = src.block(root).ok_or(BringError::NoSuchBlock)?.clone();
        let before = self.tables();
        let mut merge = Merge {
            dst: self,
            src,
            layers: HashMap::new(),
            text_styles: HashMap::new(),
            linetypes: HashMap::new(),
            warnings: Vec::new(),
            blocks: HashMap::new(),
            choices: Some(choices),
            replaced: Vec::new(),
            refused: None,
        };
        let block = merge.block(&def);
        let blocks = blocks_below(&def)
            .into_iter()
            .filter_map(|d| merge.blocks.get(&d.id).map(|b| (d.id, b.clone())))
            .collect();
        let (warnings, old, refused) = (merge.warnings, merge.replaced, merge.refused);
        if let Some(why) = refused {
            self.replace_many(old);
            self.restore_tables(&before);
            return Err(BringError::Refused(why));
        }
        let new: Vec<(EntityId, Entity)> = old
            .iter()
            .filter_map(|(id, _)| self.object(*id).map(|o| (*id, o.entity.clone())))
            .collect();
        Ok(Brought {
            block,
            blocks,
            warnings,
            rebound: new.len(),
            after: self.tables(),
            before,
            old,
            new,
        })
    }

    /// Takes back what `bring_block` did.
    pub fn unbring(&mut self, brought: &Brought) {
        self.replace_many(brought.old.clone());
        self.restore_tables(&brought.before);
    }

    /// Does again what `unbring` took back.
    pub fn rebring(&mut self, brought: &Brought) {
        self.restore_tables(&brought.after);
        self.replace_many(brought.new.clone());
    }

    /// Brings block `root` of `src` (see `bring_block`) and adds a reference to
    /// it, on the current layer of `layout` (`None` for model space).
    pub fn insert_from(
        &mut self,
        src: &Document,
        root: BlockId,
        choices: &HashMap<BlockId, BlockClash>,
        placement: &LibraryPlacement,
        layout: Option<LayoutId>,
    ) -> Result<Placed, BringError> {
        let brought = self.bring_block(src, root, choices)?;
        let block = brought.block.clone();
        let attributes = match &placement.values {
            None => block.new_references(|d| d.default_value.clone()),
            Some(values) => {
                let mut values = values.iter();
                block.new_references(|d| {
                    values
                        .next()
                        .cloned()
                        .unwrap_or_else(|| d.default_value.clone())
                })
            }
        };
        let insert = Insert {
            block,
            insertion: placement.insertion,
            scale_x: placement.scale_x,
            scale_y: placement.scale_y,
            rotation: placement.rotation,
            attributes,
            dynamic: None,
            is_xref: false,
        };
        let attrs = Attributes::on_layer(self.current_layer);
        let object = Object::new(
            self.allocate_id(),
            Entity::Insert(Box::new(insert)),
            attrs,
            layout,
        );
        self.push_many(vec![object.clone()]);
        Ok(Placed { brought, object })
    }

    /// Takes back an `insert_from`: the reference, then the blocks.
    pub fn unplace(&mut self, placed: &Placed) {
        self.remove_many(&[placed.object.id]);
        self.unbring(&placed.brought);
    }

    /// Does again what `unplace` took back.
    pub fn replace_placed(&mut self, placed: &Placed) {
        self.rebring(&placed.brought);
        self.push_many(vec![placed.object.clone()]);
    }
}

/// `root` and every block below it, each once, parents before children.
fn blocks_below(root: &Arc<BlockDef>) -> Vec<Arc<BlockDef>> {
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(def) = stack.pop() {
        if !seen.insert(def.id) {
            continue;
        }
        for e in def.entities.iter().rev() {
            if let Entity::Insert(i) = &e.entity {
                stack.push(i.block.clone());
            }
        }
        found.push(def);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockEntity, BlockObjectId, Insert};
    use crate::layer::{Attributes, Layer};
    use crate::{Entity, Point};

    fn circle(r: f64) -> Entity {
        Entity::Circle {
            center: Point::new(0.0, 0.0),
            radius: r,
        }
    }

    /// A block called `name` on a new layer `layer`, holding a circle of
    /// radius `r` and an insert of each of `nested`.
    fn make(
        doc: &mut Document,
        name: &str,
        layer: &str,
        r: f64,
        nested: &[&Arc<BlockDef>],
    ) -> Arc<BlockDef> {
        let layer_id = match doc.layers().iter().find(|l| l.name == layer) {
            Some(l) => l.id,
            None => {
                let id = doc.allocate_layer_id();
                doc.push_layer(Layer::new(id, layer));
                id
            }
        };
        let mut entities = vec![BlockEntity::new(
            BlockObjectId(0),
            circle(r),
            Attributes::on_layer(layer_id),
        )];
        for (i, n) in nested.iter().enumerate() {
            entities.push(BlockEntity::new(
                BlockObjectId(i as u64 + 1),
                Entity::Insert(Box::new(Insert::new((*n).clone(), Point::new(1.0, 1.0)))),
                Attributes::on_layer(layer_id),
            ));
        }
        let def = Arc::new(BlockDef {
            unit: Default::default(),
            id: doc.allocate_block_id(),
            name: name.to_owned(),
            base: Point::new(0.0, 0.0),
            description: String::new(),
            entities,
            dynamic: None,
        });
        doc.push_block(def.clone());
        def
    }

    fn radius(def: &BlockDef) -> f64 {
        match def.entities[0].entity {
            Entity::Circle { radius, .. } => radius,
            _ => f64::NAN,
        }
    }

    #[test]
    fn a_block_comes_across_with_its_layers_and_what_it_nests() {
        let mut src = Document::default();
        let nut = make(&mut src, "nut", "Fixings", 1.0, &[]);
        let bolt = make(&mut src, "bolt", "Fixings", 2.0, &[&nut]);
        let mut dst = Document::default();
        let brought = dst
            .bring_block(&src, bolt.id, &HashMap::new())
            .expect("block exists");
        assert_eq!(brought.block.name, "bolt");
        assert_eq!(dst.blocks().len(), 2);
        assert!(dst.block_named("nut").is_some());
        assert_eq!(brought.blocks.len(), 2);
        let layer = dst.layers().iter().find(|l| l.name == "Fixings");
        assert!(layer.is_some());
        assert_eq!(
            brought.block.entities[0].attrs.layer,
            layer.map(|l| l.id).expect("layer was added")
        );
        assert!(dst.validate().is_ok());
    }

    #[test]
    fn a_taken_name_is_renamed_by_default_and_the_targets_block_is_left_alone() {
        let mut src = Document::default();
        let valve = make(&mut src, "Valve", "Piping", 5.0, &[]);
        let mut dst = Document::default();
        let own = make(&mut dst, "valve", "Other", 1.0, &[]);

        let clashes = dst.block_clashes(&src, valve.id).expect("exists");
        assert_eq!(
            clashes,
            [BlockNameClash {
                source: valve.id,
                name: "Valve".into(),
                existing: own.id
            }]
        );
        let brought = dst
            .bring_block(&src, valve.id, &HashMap::new())
            .expect("exists");
        assert_eq!(brought.block.name, "Valve_2");
        assert_eq!(radius(&brought.block), 5.0);
        assert_eq!(radius(dst.block_named("valve").expect("kept")), 1.0);
        assert_eq!(dst.blocks().len(), 2);
    }

    #[test]
    fn use_existing_adds_nothing() {
        let mut src = Document::default();
        let valve = make(&mut src, "valve", "Piping", 5.0, &[]);
        let mut dst = Document::default();
        let own = make(&mut dst, "valve", "Other", 1.0, &[]);
        let choices = HashMap::from([(valve.id, BlockClash::UseExisting)]);
        let brought = dst.bring_block(&src, valve.id, &choices).expect("exists");
        assert_eq!(brought.block.id, own.id);
        assert_eq!(dst.blocks().len(), 1);
        assert!(dst.layers().iter().all(|l| l.name != "Piping"));
    }

    #[test]
    fn each_nested_block_has_its_own_choice() {
        let mut src = Document::default();
        let nut = make(&mut src, "nut", "L", 1.0, &[]);
        let bolt = make(&mut src, "bolt", "L", 2.0, &[&nut]);
        let mut dst = Document::default();
        let own_nut = make(&mut dst, "nut", "L", 9.0, &[]);

        let clashes = dst.block_clashes(&src, bolt.id).expect("exists");
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].source, nut.id);

        let keep = HashMap::from([(nut.id, BlockClash::UseExisting)]);
        let mut a = dst.clone();
        let brought = a.bring_block(&src, bolt.id, &keep).expect("exists");
        let inner = brought.block.nested_blocks().next().expect("nests");
        assert_eq!(inner, own_nut.id);

        let brought = dst
            .bring_block(&src, bolt.id, &HashMap::new())
            .expect("exists");
        let inner = brought.block.nested_blocks().next().expect("nests");
        assert_ne!(inner, own_nut.id);
        assert_eq!(dst.block(inner).map(|b| b.name.as_str()), Some("nut_2"));
    }

    #[test]
    fn names_are_made_unique_ignoring_case_and_repeatedly() {
        let mut doc = Document::default();
        make(&mut doc, "Nut", "L", 1.0, &[]);
        make(&mut doc, "nut_2", "L", 1.0, &[]);
        assert_eq!(doc.unused_block_name_like("NUT"), "NUT_3");
        assert_eq!(doc.unused_block_name_like(" free "), "free");
    }

    #[test]
    fn a_missing_block_is_none() {
        let src = Document::default();
        let mut dst = Document::default();
        assert!(dst.block_clashes(&src, BlockId(99)).is_none());
        assert_eq!(
            dst.bring_block(&src, BlockId(99), &HashMap::new()).err(),
            Some(BringError::NoSuchBlock)
        );
    }

    #[test]
    fn a_block_brought_twice_is_two_blocks() {
        let mut src = Document::default();
        let valve = make(&mut src, "valve", "L", 1.0, &[]);
        let mut dst = Document::default();
        dst.bring_block(&src, valve.id, &HashMap::new())
            .expect("brings");
        dst.bring_block(&src, valve.id, &HashMap::new())
            .expect("brings");
        let names: Vec<_> = dst.blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["valve", "valve_2"]);
    }
}
