//! The native format's block table and block references.
//!
//! A definition is written once, in the drawing's `blocks` list, with its
//! objects in the block's own coordinates. An insert names its block by id.
//! Definitions may nest in any order (a redefined block can insert a newer
//! one), so reading resolves them by id, refusing a block that inserts
//! itself, however indirectly, or nests deeper than the core's limit.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use opendraft_core::block::{block_name_problem, MAX_EXPANSION, MAX_NESTING};
use opendraft_core::{
    Attributes, BlockDef, BlockEntity, BlockId, BlockObjectId, Color, DimStyle, Entity, LayerId,
    LinetypeId, Lineweight,
};
use serde::{Deserialize, Serialize};

use super::attribute::AttributeReferenceDto;
use super::block_dynamic::{DynamicBlockDto, DynamicStateDto};
use super::dimension_style::StylePool;
use super::entity::{from_point, invalid, point, EntityDto, PointDto};
use super::{attr_from_dto, attr_to_dto, AttrDto, ColorDto};
use crate::FileError;

/// Definitions already read, by id, for resolving inserts.
pub(super) type Blocks = HashMap<u64, Arc<BlockDef>>;

#[derive(Serialize, Deserialize)]
pub(super) struct BlockDto {
    id: u64,
    name: String,
    base: PointDto,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    description: String,
    /// `INSUNITS` code of the block's units; 0 (unitless) is left out.
    #[serde(default, skip_serializing_if = "is_zero")]
    unit: u8,
    /// In draw order.
    #[serde(deserialize_with = "super::limits::block_objects")]
    objects: Vec<BlockObjectDto>,
    /// The parameters and actions of a dynamic block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dynamic: Option<DynamicBlockDto>,
}

#[derive(Serialize, Deserialize)]
struct BlockObjectDto {
    /// Names the object inside its block. A block with no schema may leave
    /// it out (older files); then the objects are numbered in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<u64>,
    layer: u64,
    color: AttrDto<ColorDto>,
    linetype: AttrDto<u64>,
    lineweight: AttrDto<u16>,
    #[serde(flatten)]
    scale: super::linetype::ScaleDto,
    entity: EntityDto,
}

fn is_zero(n: &u8) -> bool {
    *n == 0
}

impl BlockDto {
    /// `from_block`, with `known` naming the blocks its inserts may refer to
    /// that are not in the block table (an xref's local definitions).
    pub(super) fn from_block_in(
        block: &BlockDef,
        styles: &mut StylePool,
        known: &Blocks,
    ) -> Result<Self, FileError> {
        let objects = block
            .entities
            .iter()
            .map(|e| {
                Ok(BlockObjectDto {
                    id: Some(e.id.0),
                    layer: e.attrs.layer.0,
                    color: attr_to_dto(&e.attrs.color, |c| (*c).into()),
                    linetype: attr_to_dto(&e.attrs.linetype, |id| id.0),
                    lineweight: attr_to_dto(&e.attrs.lineweight, |w| w.0),
                    scale: super::linetype::ScaleDto::of(&e.attrs),
                    entity: EntityDto::from_entity_in(&e.entity, styles, known)?,
                })
            })
            .collect::<Result<_, FileError>>()?;
        let dynamic = match &block.dynamic {
            Some(schema) => {
                let ids = block.entities.iter().map(|e| e.id).collect();
                Some(DynamicBlockDto::of(schema, &ids)?)
            }
            None => None,
        };
        Ok(Self {
            id: block.id.0,
            name: block.name.clone(),
            base: point(block.base),
            description: block.description.clone(),
            unit: block.unit.code() as u8,
            objects,
            dynamic,
        })
    }

    /// The ids of the block's objects: as written, or (a block with no
    /// schema, in a file that has none) their order. Never a mixture.
    fn object_ids(&self) -> Result<Vec<BlockObjectId>, FileError> {
        let given = self.objects.iter().filter(|o| o.id.is_some()).count();
        if given == self.objects.len() {
            let ids: Vec<_> = self.objects.iter().flat_map(|o| o.id).collect();
            Ok(ids.into_iter().map(BlockObjectId).collect())
        } else if given == 0 && self.dynamic.is_none() {
            Ok((0..self.objects.len() as u64).map(BlockObjectId).collect())
        } else {
            invalid(format!("block \"{}\" has objects without ids", self.name))
        }
    }
}

/// `load_blocks`, with the host table's name rules (`strict_names` false for
/// an xref's local definitions, whose names may contain `|`), and `known`
/// naming definitions outside `dtos` that an insert may still refer to.
pub(super) fn load_blocks_with(
    dtos: &[BlockDto],
    dim_styles: &[DimStyle],
    layer_ok: impl Fn(u64) -> bool,
    strict_names: bool,
    known: &Blocks,
) -> Result<Vec<Arc<BlockDef>>, FileError> {
    let mut by_id = HashMap::new();
    for dto in dtos {
        if by_id.insert(dto.id, dto).is_some() {
            return invalid(format!("block {} appears twice", dto.id));
        }
        if known.contains_key(&dto.id) {
            return invalid(format!("block {} appears twice", dto.id));
        }
        let bad = if strict_names {
            block_name_problem(&dto.name)
        } else if dto.name.trim().is_empty() || dto.name.chars().count() > 255 {
            Some("a block name must be 1 to 255 characters")
        } else {
            None
        };
        if let Some(why) = bad {
            return invalid(format!("block \"{}\": {why}", dto.name));
        }
    }
    let mut done = Blocks::new();
    let mut path = Vec::new();
    let mut sizes = HashMap::new();
    for dto in dtos {
        resolve(
            dto.id, &by_id, &layer_ok, dim_styles, &mut done, &mut path, &mut sizes, known,
        )?;
    }
    Ok(dtos
        .iter()
        .filter_map(|d| done.get(&d.id).cloned())
        .collect())
}

// A block's resolution threads the file's lookup, the blocks read so far and
// the outsider map; splitting it would only move the same state around.
#[allow(clippy::too_many_arguments)]
fn resolve(
    id: u64,
    by_id: &HashMap<u64, &BlockDto>,
    layer_ok: &impl Fn(u64) -> bool,
    dim_styles: &[DimStyle],
    done: &mut Blocks,
    path: &mut Vec<u64>,
    sizes: &mut HashMap<BlockId, usize>,
    known: &Blocks,
) -> Result<Arc<BlockDef>, FileError> {
    if let Some(block) = done.get(&id) {
        return Ok(block.clone());
    }
    if let Some(block) = known.get(&id) {
        return Ok(block.clone());
    }
    let Some(dto) = by_id.get(&id) else {
        return invalid(format!(
            "an insert refers to block {id}, which does not exist"
        ));
    };
    if path.contains(&id) {
        return invalid(format!("block \"{}\" inserts itself", dto.name));
    }
    if path.len() >= MAX_NESTING {
        return invalid(format!("block \"{}\" is nested too deeply", dto.name));
    }
    path.push(id);
    for object in &dto.objects {
        if let EntityDto::Insert { block, .. } = &object.entity {
            if by_id.contains_key(block) {
                resolve(
                    *block, by_id, layer_ok, dim_styles, done, path, sizes, known,
                )?;
            } else if !known.contains_key(block) {
                return invalid(format!(
                    "block \"{}\" inserts block {block}, which does not exist",
                    dto.name
                ));
            }
        }
    }
    path.pop();
    let mut entities = Vec::with_capacity(dto.objects.len());
    let mut resolved = known.clone();
    resolved.extend(done.iter().map(|(id, block)| (*id, block.clone())));
    let object_ids = dto.object_ids()?;
    let unique: HashSet<BlockObjectId> = object_ids.iter().copied().collect();
    if unique.len() != object_ids.len() {
        return invalid(format!(
            "block \"{}\" has two objects with one id",
            dto.name
        ));
    }
    let dynamic = dto
        .dynamic
        .as_ref()
        .map(|d| d.to_schema(&unique).map(Box::new))
        .transpose()
        .or_else(|e| match e {
            FileError::Invalid(why) => invalid(format!("block \"{}\": {why}", dto.name)),
            other => Err(other),
        })?;
    for (object, object_id) in dto.objects.iter().zip(object_ids) {
        if !layer_ok(object.layer) {
            return invalid(format!(
                "block \"{}\" has an object on layer {}, which does not exist",
                dto.name, object.layer
            ));
        }
        let mut attrs = Attributes {
            layer: LayerId(object.layer),
            color: attr_from_dto(object.color.clone(), Color::from),
            linetype: attr_from_dto(object.linetype.clone(), LinetypeId),
            lineweight: attr_from_dto(object.lineweight.clone(), Lineweight),
            ..Attributes::on_layer(LayerId(object.layer))
        };
        object.scale.apply(&mut attrs);
        entities.push(BlockEntity::new(
            object_id,
            object.entity.to_entity(&resolved, dim_styles)?,
            attrs,
        ));
    }
    let block = Arc::new(BlockDef {
        unit: opendraft_core::InsertUnits::from_code(i64::from(dto.unit)).unwrap_or_default(),
        id: BlockId(id),
        name: dto.name.trim().to_owned(),
        base: from_point(dto.base)?,
        description: dto.description.clone(),
        entities,
        dynamic,
    });
    if block.expansion_size(sizes) > MAX_EXPANSION {
        return invalid(format!(
            "block \"{}\" expands to too many objects (its inserts nest and repeat other blocks)",
            block.name
        ));
    }
    done.insert(id, block.clone());
    Ok(block)
}

/// The entity for an insert's fields, resolving its block in `blocks`.
// One field per part of the DTO; bundling them would not make the call safer.
#[allow(clippy::too_many_arguments)]
pub(super) fn insert_entity(
    blocks: &Blocks,
    block: u64,
    insertion: PointDto,
    scale: [f64; 2],
    rotation: f64,
    attributes: &[AttributeReferenceDto],
    dynamic: Option<&DynamicStateDto>,
    is_xref: bool,
) -> Result<Entity, FileError> {
    let Some(def) = blocks.get(&block) else {
        return invalid(format!(
            "an insert refers to block {block}, which does not exist"
        ));
    };
    let insert = opendraft_core::Insert {
        block: def.clone(),
        insertion: from_point(insertion)?,
        scale_x: scale[0],
        scale_y: scale[1],
        rotation,
        attributes: attributes
            .iter()
            .map(AttributeReferenceDto::to_reference)
            .collect::<Result<_, _>>()?,
        dynamic: dynamic.map(|d| d.to_state().map(Box::new)).transpose()?,
        is_xref,
    };
    if let Some(state) = insert.dynamic.as_deref() {
        let held = match &def.dynamic {
            Some(schema) => schema.check_state(state),
            None => Err("the block has no parameters".to_owned()),
        };
        if let Err(why) = held {
            return invalid(format!("an insert of block {block}: {why}"));
        }
    }
    if !insert.is_valid() {
        return invalid("a block insert has a zero or non-finite scale or rotation");
    }
    Ok(Entity::Insert(Box::new(insert)))
}

#[cfg(test)]
mod tests {
    use opendraft_core::{Attr, Document, Insert, Layer, Point};

    use crate::{FileError, FileFormat, NativeFormat};

    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    /// Two blocks, the second inserting the first, on a layer of their own,
    /// placed twice in the drawing.
    fn nested() -> Document {
        let mut doc = Document::default();
        let walls = doc.allocate_layer_id();
        doc.push_layer(Layer::new(walls, "Walls"));
        let inner = Arc::new(BlockDef {
            unit: opendraft_core::InsertUnits::from_code(4).unwrap(),
            id: doc.allocate_block_id(),
            name: "Inner".into(),
            base: p(1.0, 1.0),
            description: "a circle".into(),
            entities: vec![
                BlockEntity {
                    id: opendraft_core::BlockObjectId(0),
                    entity: Entity::Circle {
                        center: p(1.0, 1.0),
                        radius: 0.5,
                    },
                    attrs: Attributes {
                        color: Attr::Set(Color::Rgb(0, 128, 255)),
                        ..Attributes::on_layer(walls)
                    },
                },
                BlockEntity {
                    id: opendraft_core::BlockObjectId(1),
                    entity: Entity::Ellipse(
                        opendraft_core::Ellipse::new(p(1.0, 1.0), p(0.0, 2.0), 0.5, 1.0, 4.0)
                            .unwrap(),
                    ),
                    attrs: Attributes::on_layer(walls),
                },
                BlockEntity {
                    id: opendraft_core::BlockObjectId(2),
                    entity: Entity::Spline(Box::new(
                        opendraft_core::Spline::from_fit_points(
                            vec![p(0.0, 0.0), p(1.0, 2.0), p(3.0, 1.0)],
                            opendraft_core::KnotParameterization::Chord,
                        )
                        .unwrap(),
                    )),
                    attrs: Attributes::on_layer(walls),
                },
            ],
            dynamic: None,
        });
        let mut placed = Insert::new(inner.clone(), p(3.0, 0.0));
        placed.scale_x = -2.0;
        placed.rotation = 0.5;
        let outer = Arc::new(BlockDef {
            unit: Default::default(),
            id: doc.allocate_block_id(),
            name: "Outer".into(),
            base: p(0.0, 0.0),
            description: String::new(),
            entities: vec![BlockEntity {
                id: opendraft_core::BlockObjectId(0),
                entity: Entity::Insert(Box::new(placed)),
                attrs: Attributes::on_layer(LayerId::DEFAULT),
            }],
            dynamic: None,
        });
        doc.push_block(inner);
        doc.push_block(outer.clone());
        doc.add(Entity::Insert(Box::new(Insert::new(
            outer.clone(),
            p(0.0, 0.0),
        ))));
        let mut second = Insert::new(outer, p(10.0, 5.0));
        second.scale_y = 3.0;
        doc.add(Entity::Insert(Box::new(second)));
        doc
    }

    fn round_trip(doc: &Document) -> Document {
        let bytes = NativeFormat.write(doc).unwrap().bytes;
        NativeFormat.read(&bytes).unwrap().document
    }

    #[test]
    fn a_blocks_units_survive_a_round_trip() {
        let back = round_trip(&nested());
        let unit = |name: &str| back.block_named(name).unwrap().unit;
        assert_eq!(unit("Inner").label(), "Millimeters");
        assert_eq!(unit("Outer"), opendraft_core::InsertUnits::UNITLESS);
    }

    #[test]
    fn blocks_and_their_references_survive_a_round_trip() {
        let doc = nested();
        let back = round_trip(&doc);
        assert_eq!(back.blocks(), doc.blocks());
        assert_eq!(back.objects(), doc.objects());
        assert_eq!(back.next_block_id(), doc.next_block_id());
        assert_eq!(back.validate(), Ok(()));
    }

    #[test]
    fn a_block_defined_before_the_one_it_inserts_still_reads() {
        let text = String::from_utf8(NativeFormat.write(&nested()).unwrap().bytes).unwrap();
        let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
        let blocks = file["drawing"]["blocks"].as_array_mut().unwrap();
        blocks.reverse();
        let back = NativeFormat
            .read(file.to_string().as_bytes())
            .unwrap()
            .document;
        assert_eq!(back.blocks()[0].name, "Outer");
        assert_eq!(back.validate(), Ok(()));
    }

    fn refused(edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let text = String::from_utf8(NativeFormat.write(&nested()).unwrap().bytes).unwrap();
        let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
        edit(&mut file);
        match NativeFormat.read(file.to_string().as_bytes()) {
            Err(FileError::Invalid(why)) => why,
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("a damaged file was loaded"),
        }
    }

    /// `levels` blocks above an empty one, each inserting the one below twice.
    fn doubling_chain(f: &mut serde_json::Value, levels: u64) {
        let template = f["drawing"]["blocks"][1]["objects"][0].clone();
        let block = |id: u64, objects: Vec<serde_json::Value>| {
            let mut b = f["drawing"]["blocks"][0].clone();
            b["id"] = serde_json::json!(id);
            b["name"] = serde_json::json!(format!("level{id}"));
            b["objects"] = serde_json::json!(objects);
            b
        };
        let mut blocks = vec![block(0, Vec::new())];
        for id in 1..=levels {
            let mut insert = template.clone();
            insert["entity"]["block"] = serde_json::json!(id - 1);
            let mut second = insert.clone();
            second["id"] = serde_json::json!(1);
            insert["id"] = serde_json::json!(0);
            blocks.push(block(id, vec![insert, second]));
        }
        f["drawing"]["blocks"] = serde_json::json!(blocks);
        f["drawing"]["next_block_id"] = serde_json::json!(levels + 1);
        f["drawing"]["objects"][0]["entity"]["block"] = serde_json::json!(levels);
        f["drawing"]["objects"] = serde_json::json!([f["drawing"]["objects"][0].clone()]);
    }

    #[test]
    fn blocks_that_repeat_each_other_into_millions_of_objects_are_refused() {
        let text = String::from_utf8(NativeFormat.write(&nested()).unwrap().bytes).unwrap();
        let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
        // 2^15 objects: large, but fine.
        doubling_chain(&mut file, 15);
        let back = NativeFormat.read(file.to_string().as_bytes());
        assert!(back.is_ok(), "{:?}", back.err());
        // 2^40, from forty small definitions.
        let why = refused(|f| doubling_chain(f, 40));
        assert!(why.contains("too many objects"), "{why}");
    }

    #[test]
    fn loops_missing_blocks_and_zero_scales_are_refused() {
        let why = refused(|f| {
            // Make Inner insert Outer, which inserts Inner.
            let outer_object = f["drawing"]["blocks"][1]["objects"][0].clone();
            f["drawing"]["blocks"][0]["objects"] = serde_json::json!([outer_object]);
            f["drawing"]["blocks"][0]["objects"][0]["entity"]["block"] = serde_json::json!(2);
        });
        assert!(why.contains("inserts itself"), "{why}");
        let why = refused(|f| f["drawing"]["objects"][0]["entity"]["block"] = serde_json::json!(9));
        assert!(why.contains("does not exist"), "{why}");
        let why = refused(|f| {
            f["drawing"]["objects"][0]["entity"]["scale"] = serde_json::json!([0.0, 1.0])
        });
        assert!(why.contains("zero"), "{why}");
        let why = refused(|f| f["drawing"]["blocks"][0]["name"] = serde_json::json!("a|b"));
        assert!(why.contains("cannot contain"), "{why}");
    }
}
