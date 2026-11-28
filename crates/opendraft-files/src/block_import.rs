//! Block definitions from an exchange file, for the DXF and DWG importers.
//!
//! A file names the block an insert places (DXF by name, DWG by handle) and
//! lists definitions in any order, but a `BlockDef` can only hold inserts of
//! blocks built before it. So the importer collects each definition with its
//! inserts unresolved, and [`ImportedBlocks::build`] builds them depth first,
//! then puts them in the table in the file's order. What cannot be kept is
//! counted for the warnings.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use opendraft_core::block::{
    block_name_problem, AttributeReference, DynamicBlock, DynamicValue, ParameterId, MAX_NESTING,
};
use opendraft_core::{
    Attributes, BlockDef, BlockEntity, BlockId, BlockObjectId, Document, Entity, Insert, Point,
};

use crate::plural;

/// A block reference whose block is known only by the file's key for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PendingInsert<K> {
    pub block: K,
    pub insertion: Point,
    pub scale_x: f64,
    pub scale_y: f64,
    /// Radians counter-clockwise.
    pub rotation: f64,
    /// The attributes it held, placed in the drawing (file coordinates).
    pub attributes: Vec<AttributeReference>,
    /// Distances of the block's linear parameters this reference changes.
    pub dynamic: Vec<(ParameterId, DynamicValue)>,
    /// Whether the block is an external reference's root.
    pub is_xref: bool,
}

/// Gives `insert` its attributes, brought from the drawing into the block's
/// coordinates and matched to the block's definitions: the first unused
/// non-constant definition with the same tag (ignoring case) is each one's
/// source, so duplicate tags pair up in order; one that matches nothing is
/// kept as an orphan. `false` when the insert's transform cannot be undone.
fn attach_attributes(insert: &mut Insert, placed: &[AttributeReference]) -> bool {
    let mut taken = std::collections::HashSet::new();
    for r in placed {
        let Some(text) = insert.attribute_text_in_block(&r.text) else {
            return false;
        };
        let source = insert
            .block
            .attribute_definitions()
            .filter(|(_, d)| !d.flags.constant() && d.tag.eq_ignore_ascii_case(&r.tag))
            .map(|(_, d)| d.id)
            .find(|id| taken.insert(*id));
        // Properties the definition already has are not the attribute's own.
        let own = r.attrs.clone().filter(|a| {
            source
                .and_then(|id| insert.block.attribute_definition(id))
                .is_none_or(|d| d.attrs != *a)
        });
        insert.attributes.push(AttributeReference {
            source,
            text,
            attrs: own,
            ..r.clone()
        });
    }
    true
}

/// One object of a definition being read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Item<K> {
    Entity(Entity),
    Insert(PendingInsert<K>),
}

/// A definition as the file gives it.
#[derive(Clone, Debug)]
pub(crate) struct PendingBlock<K> {
    pub key: K,
    pub name: String,
    pub base: Point,
    pub description: String,
    pub unit: opendraft_core::InsertUnits,
    pub items: Vec<(Item<K>, Attributes)>,
    /// The block's parameters and actions, its objects named by their
    /// position in `items`. Dropped when an object an action needs is gone.
    pub dynamic: Option<Box<DynamicBlock>>,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Building,
    Refused,
}

/// The blocks built from a file, by the file's key, and what was lost.
pub(crate) struct ImportedBlocks<K> {
    built: HashMap<K, Arc<BlockDef>>,
    /// Definitions refused for a name AutoCAD would not allow, or one that
    /// repeats an earlier block's (ignoring case).
    bad_names: usize,
    /// References that would put a block inside itself, or nest past
    /// `MAX_NESTING`.
    looped: usize,
    /// References to a block that is not in the file (or was refused).
    missing: usize,
    /// References with a zero or non-finite scale, position or rotation.
    invalid: usize,
    /// Dynamic blocks whose parameters and actions could not be kept.
    dynamic_refused: usize,
    /// References whose changed values the block's parameters do not allow.
    dynamic_unfit: usize,
}

impl<K> Default for ImportedBlocks<K> {
    fn default() -> Self {
        ImportedBlocks {
            built: HashMap::new(),
            bad_names: 0,
            looped: 0,
            missing: 0,
            invalid: 0,
            dynamic_refused: 0,
            dynamic_unfit: 0,
        }
    }
}

/// The sentence for references to anonymous blocks, which come in as
/// ordinary blocks that keep their appearance.
pub(crate) fn anonymous_references(count: u64) -> String {
    format!(
        "Imported {} to anonymous blocks (such as modified dynamic blocks) as ordinary blocks; their parameters and actions were not imported.",
        plural(count, "block reference")
    )
}

/// A name for an anonymous block (`*U12`), which AutoCAD does not let a block
/// carry: unique among `taken` (lower-cased), which it joins.
pub(crate) fn anonymous_name(
    original: &str,
    taken: &mut std::collections::HashSet<String>,
) -> String {
    let base = format!("Anonymous_{}", original.trim_start_matches('*'));
    let mut name = base.clone();
    let mut n = 2;
    while !taken.insert(name.to_lowercase()) {
        name = format!("{base}_{n}");
        n += 1;
    }
    name
}

/// The names to build `names` under, in order: each anonymous one (`*U12`)
/// renamed, the rest as they are.
pub(crate) fn importable_names<'a>(names: impl Iterator<Item = &'a str> + Clone) -> Vec<String> {
    let mut taken: std::collections::HashSet<String> = names
        .clone()
        .filter(|n| !n.starts_with('*'))
        .map(|n| n.trim().to_lowercase())
        .collect();
    names
        .map(|n| match n.starts_with('*') {
            true => anonymous_name(n, &mut taken),
            false => n.to_owned(),
        })
        .collect()
}

/// The sentence for references to other drawings: they are kept, but their
/// content is not in this file, so nothing is drawn until they are reloaded.
pub(crate) fn external_references(count: u64) -> String {
    format!(
        "Attached {} but did not load their drawings; reload each one to show it.",
        plural(count, "external reference")
    )
}

impl<K: Clone + Eq + Hash> ImportedBlocks<K> {
    /// Builds every definition and adds them to `document`'s block table in
    /// the order given.
    pub(crate) fn build(document: &mut Document, pending: Vec<PendingBlock<K>>) -> Self {
        let mut blocks = ImportedBlocks::default();
        let mut by_key: HashMap<K, usize> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut states: Vec<Option<State>> = vec![None; pending.len()];
        for (i, p) in pending.iter().enumerate() {
            let name = p.name.trim().to_lowercase();
            if block_name_problem(&p.name).is_some() || names.contains(&name) {
                blocks.bad_names += 1;
                states[i] = Some(State::Refused);
            } else {
                names.push(name);
            }
            by_key.entry(p.key.clone()).or_insert(i);
        }
        // Ids in the file's order, so the table reads as the file does.
        let ids: Vec<_> = pending
            .iter()
            .map(|_| document.allocate_block_id())
            .collect();
        let mut builder = Builder {
            pending: &pending,
            by_key: &by_key,
            ids: &ids,
            states: &mut states,
            blocks: &mut blocks,
        };
        for i in 0..pending.len() {
            builder.build(i, 0);
        }
        for p in &pending {
            if let Some(def) = blocks.built.get(&p.key) {
                if document.block(def.id).is_none() {
                    document.push_block(def.clone());
                }
            }
        }
        blocks
    }

    /// The reference as an entity, or `None` (counted) when its block is not
    /// there or its transform is not usable.
    /// The definition built under the file's key, if it came across.
    pub(crate) fn block(&self, key: &K) -> Option<&Arc<BlockDef>>
    where
        K: std::hash::Hash + Eq,
    {
        self.built.get(key)
    }

    pub(crate) fn insert(&mut self, p: PendingInsert<K>) -> Option<Entity> {
        let Some(block) = self.built.get(&p.block) else {
            self.missing += 1;
            return None;
        };
        self.entity(block.clone(), &p)
    }

    fn entity(&mut self, block: Arc<BlockDef>, p: &PendingInsert<K>) -> Option<Entity> {
        let insert = Insert {
            block,
            insertion: p.insertion,
            scale_x: p.scale_x,
            scale_y: p.scale_y,
            rotation: p.rotation,
            attributes: Vec::new(),
            dynamic: None,
            is_xref: p.is_xref,
        };
        let mut insert = self.with_values(insert, &p.dynamic);
        if insert.is_valid() && attach_attributes(&mut insert, &p.attributes) {
            Some(Entity::Insert(Box::new(insert)))
        } else {
            self.invalid += 1;
            None
        }
    }

    /// `insert` with the parameter values a modified reference had, each
    /// brought into what its parameter allows.
    fn with_values(&mut self, insert: Insert, values: &[(ParameterId, DynamicValue)]) -> Insert {
        let mut out = insert.clone();
        for (parameter, value) in values {
            match out.with_dynamic_value(*parameter, value.clone()) {
                Some(next) => out = next,
                None => {
                    self.dynamic_unfit += 1;
                    return insert;
                }
            }
        }
        let fitted = out
            .dynamic
            .as_deref()
            .zip(out.block.dynamic.as_deref())
            .map(|(state, schema)| schema.fit_state(state));
        match fitted {
            Some(state) => Insert {
                dynamic: state.map(Box::new),
                ..out
            },
            None => out,
        }
    }

    /// The sentences for what did not come across.
    pub(crate) fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.bad_names > 0 {
            out.push(format!(
                "Skipped {} whose names are not allowed or repeat another block's name.",
                plural(self.bad_names as u64, "block")
            ));
        }
        if self.looped > 0 {
            out.push(format!(
                "Skipped {} that would put a block inside itself.",
                plural(self.looped as u64, "block reference")
            ));
        }
        if self.missing > 0 {
            out.push(format!(
                "Skipped {} to blocks that are not in the file.",
                plural(self.missing as u64, "block reference")
            ));
        }
        if self.invalid > 0 {
            out.push(format!(
                "Skipped {} with a zero or invalid scale or position.",
                plural(self.invalid as u64, "block reference")
            ));
        }
        if self.dynamic_refused > 0 {
            out.push(format!(
                "Kept {} as ordinary: the parameters and actions did not read as a valid dynamic block.",
                plural(self.dynamic_refused as u64, "block")
            ));
        }
        if self.dynamic_unfit > 0 {
            out.push(format!(
                "Placed {} at the block's defaults: their values did not fit its parameters.",
                plural(self.dynamic_unfit as u64, "dynamic block reference")
            ));
        }
        out
    }
}

/// The depth-first walk that builds one definition after the ones it holds.
struct Builder<'a, K> {
    pending: &'a [PendingBlock<K>],
    by_key: &'a HashMap<K, usize>,
    ids: &'a [BlockId],
    states: &'a mut Vec<Option<State>>,
    blocks: &'a mut ImportedBlocks<K>,
}

impl<K: Clone + Eq + Hash> Builder<'_, K> {
    /// Builds definition `i` (and first every block it inserts), unless it
    /// is built, refused or being built further up.
    fn build(&mut self, i: usize, depth: usize) {
        if self.states[i].is_some() || self.blocks.built.contains_key(&self.pending[i].key) {
            return;
        }
        self.states[i] = Some(State::Building);
        let pending = self.pending;
        let p = &pending[i];
        let mut entities = Vec::with_capacity(p.items.len());
        for (index, (item, attrs)) in p.items.iter().enumerate() {
            let entity = match item {
                Item::Entity(e) => Some(e.clone()),
                Item::Insert(insert) => self.nested(insert, depth),
            };
            if let Some(entity) = entity {
                // An object keeps its place in the file's list as its id, which
                // is what a dynamic block's actions name.
                let id = BlockObjectId(index as u64);
                entities.push(BlockEntity::new(id, entity, attrs.clone()));
            }
        }
        let dynamic = p
            .dynamic
            .as_deref()
            .and_then(|schema| self.kept(schema, &entities));
        let def = BlockDef {
            unit: p.unit,
            id: self.ids[i],
            name: p.name.trim().to_owned(),
            base: p.base,
            description: p.description.clone(),
            entities,
            dynamic,
        };
        self.blocks.built.insert(p.key.clone(), Arc::new(def));
        self.states[i] = None;
    }

    /// The schema without objects that did not come across, or `None`
    /// (counted) when no action or visibility is left or what is left does not check.
    fn kept(
        &mut self,
        schema: &DynamicBlock,
        entities: &[BlockEntity],
    ) -> Option<Box<DynamicBlock>> {
        let ids: std::collections::HashSet<_> = entities.iter().map(|e| e.id).collect();
        let schema = schema.pruned(|o| ids.contains(&o)).schema;
        if (schema.actions.is_empty() && schema.visibility().is_none())
            || schema.check(&ids).is_err()
        {
            self.blocks.dynamic_refused += 1;
            return None;
        }
        Some(Box::new(schema))
    }

    fn nested(&mut self, insert: &PendingInsert<K>, depth: usize) -> Option<Entity> {
        let Some(&j) = self.by_key.get(&insert.block) else {
            self.blocks.missing += 1;
            return None;
        };
        if self.states[j] == Some(State::Building) || depth >= MAX_NESTING {
            self.blocks.looped += 1;
            return None;
        }
        self.build(j, depth + 1);
        match self.blocks.built.get(&insert.block) {
            Some(block) => {
                let block = block.clone();
                self.blocks.entity(block, insert)
            }
            None => {
                self.blocks.missing += 1;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::LayerId;

    use super::*;

    fn line() -> (Item<&'static str>, Attributes) {
        let e = Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        };
        (Item::Entity(e), Attributes::on_layer(LayerId::DEFAULT))
    }

    fn insert(of: &'static str) -> (Item<&'static str>, Attributes) {
        let i = PendingInsert {
            block: of,
            insertion: Point::new(2.0, 0.0),
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            attributes: Vec::new(),
            dynamic: Vec::new(),
            is_xref: false,
        };
        (Item::Insert(i), Attributes::on_layer(LayerId::DEFAULT))
    }

    fn block(
        key: &'static str,
        items: Vec<(Item<&'static str>, Attributes)>,
    ) -> PendingBlock<&'static str> {
        PendingBlock {
            unit: Default::default(),
            key,
            name: key.to_owned(),
            base: Point::new(0.0, 0.0),
            description: String::new(),
            items,
            dynamic: None,
        }
    }

    #[test]
    fn a_block_may_insert_one_defined_after_it() {
        let mut doc = Document::default();
        let mut blocks = ImportedBlocks::build(
            &mut doc,
            vec![
                block("Outer", vec![insert("Inner")]),
                block("Inner", vec![line()]),
            ],
        );
        let names: Vec<&str> = doc.blocks().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["Outer", "Inner"], "the file's order");
        let outer = doc.block_named("Outer").unwrap();
        let inner = doc.block_named("Inner").unwrap();
        assert!(outer.holds(inner.id));
        assert!(blocks.warnings().is_empty());
        let entity = blocks.insert(PendingInsert {
            block: "Outer",
            insertion: Point::new(5.0, 5.0),
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 0.0,
            attributes: Vec::new(),
            dynamic: Vec::new(),
            is_xref: false,
        });
        doc.add(entity.unwrap());
        doc.validate().unwrap();
    }

    #[test]
    fn loops_missing_blocks_and_bad_names_are_counted() {
        let mut doc = Document::default();
        let mut blocks = ImportedBlocks::build(
            &mut doc,
            vec![
                block("A", vec![line(), insert("B")]),
                block("B", vec![insert("A")]),
                block("C", vec![insert("Nowhere")]),
                block("bad|name", vec![line()]),
                block("a", vec![line()]),
            ],
        );
        assert_eq!(doc.blocks().len(), 3);
        doc.validate().unwrap();
        assert!(blocks
            .insert(PendingInsert {
                block: "A",
                insertion: Point::new(0.0, 0.0),
                scale_x: 0.0,
                scale_y: 1.0,
                rotation: 0.0,
                attributes: Vec::new(),
                dynamic: Vec::new(),
                is_xref: false,
            })
            .is_none());
        assert_eq!(
            blocks.warnings(),
            [
                "Skipped 2 blocks whose names are not allowed or repeat another block's name.",
                "Skipped 1 block reference that would put a block inside itself.",
                "Skipped 1 block reference to blocks that are not in the file.",
                "Skipped 1 block reference with a zero or invalid scale or position.",
            ]
        );
    }
}
