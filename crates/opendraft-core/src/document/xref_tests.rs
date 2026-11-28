//! Tests for the external-reference table, its invariants, and the layer-0
//! rule in an xref's picture.

use std::sync::Arc;

use super::*;
use crate::block::{BlockDef, BlockEntity, BlockId, BlockObjectId};
use crate::validate::DocumentError;
use crate::xref::{XrefDef, XrefKind, XrefPathType};
use crate::Insert;
use crate::{Attributes, Entity, Layer, LayerId, Point};

const ROOT: BlockId = BlockId(2);
const LOCAL: BlockId = BlockId(1);

fn line() -> Entity {
    Entity::Line {
        start: Point::new(0.0, 0.0),
        end: Point::new(1.0, 0.0),
    }
}

fn block(id: BlockId, name: &str, entities: Vec<BlockEntity>) -> Arc<BlockDef> {
    Arc::new(BlockDef {
        id,
        name: name.to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities,
        dynamic: None,
    })
}

fn local() -> Arc<BlockDef> {
    block(
        LOCAL,
        "Chairs",
        vec![BlockEntity::new(
            BlockObjectId(0),
            line(),
            Attributes::on_layer(LayerId::DEFAULT),
        )],
    )
}

/// A drawing with layer `base|Walls` bound to `walls`, a host layer, and a
/// root whose content uses layer 0, the bound layer and a local block.
fn sample() -> (Document, LayerId, LayerId) {
    let mut doc = Document::default();
    let walls = doc.allocate_layer_id();
    doc.push_layer(Layer::new(walls, "base|Walls"));
    let host = doc.allocate_layer_id();
    doc.push_layer(Layer::new(host, "Host"));
    doc.reserve_block_ids(3);
    let root = block(
        ROOT,
        "base",
        vec![
            BlockEntity::new(
                BlockObjectId(0),
                line(),
                Attributes::on_layer(LayerId::DEFAULT),
            ),
            BlockEntity::new(
                BlockObjectId(1),
                Entity::Circle {
                    center: Point::new(2.0, 0.0),
                    radius: 1.0,
                },
                Attributes::on_layer(walls),
            ),
            BlockEntity::new(
                BlockObjectId(2),
                Entity::Insert(Box::new(Insert::new(local(), Point::new(5.0, 0.0)))),
                Attributes::on_layer(LayerId::DEFAULT),
            ),
        ],
    );
    doc.push_block(root);
    let xref = XrefDef {
        block: ROOT,
        path: "base.dwg".to_owned(),
        path_type: XrefPathType::Relative,
        kind: XrefKind::Attach,
        unloaded: false,
        stamp: None,
        layers: vec![crate::XrefLayer {
            source: "Walls".to_owned(),
            layer: walls,
        }],
        blocks: vec![local()],
        clip: None,
        report: Default::default(),
    };
    doc.set_xrefs(vec![xref]);
    (doc, walls, host)
}

#[test]
fn a_hand_built_reference_is_valid_and_found_by_name_and_block() {
    let (doc, walls, _) = sample();
    assert_eq!(doc.validate(), Ok(()));
    assert_eq!(doc.xref(ROOT).map(|x| x.path.as_str()), Some("base.dwg"));
    assert_eq!(doc.xref_by_name("BASE").map(|x| x.block), Some(ROOT));
    assert!(doc.is_xref_block(ROOT));
    assert_eq!(doc.xref_layer_owner(walls), Some(ROOT));
    assert!(doc.xrefs()[0].owns_block(LOCAL));
    assert!(doc.xrefs()[0].owns_layer(walls));
}

#[test]
fn a_missing_root_is_refused() {
    let (mut doc, _, _) = sample();
    doc.set_blocks(Vec::new());
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn two_records_for_one_root_are_refused() {
    let (mut doc, _, _) = sample();
    let mut xrefs = doc.xrefs().to_vec();
    xrefs.push(xrefs[0].clone());
    doc.set_xrefs(xrefs);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_local_block_in_the_host_table_is_refused() {
    let (mut doc, _, _) = sample();
    doc.push_block(local());
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_missing_bound_layer_or_a_bad_binding_name_is_refused() {
    let (mut doc, walls, _) = sample();
    doc.remove_layer(walls);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));

    let (mut doc, _, _) = sample();
    let mut xrefs = doc.xrefs().to_vec();
    xrefs[0].layers[0].source = "Doors".to_owned();
    doc.set_xrefs(xrefs);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_bound_layer_cannot_be_current_and_cannot_be_deleted() {
    let (mut doc, walls, _) = sample();
    assert!(!doc.set_current_layer(walls));
    assert_eq!(doc.current_layer(), LayerId::DEFAULT);
    assert!(doc.layer_delete_blocker(walls).is_some());

    // A state a bad edit could make: the current layer is bound.
    doc.current_layer = walls;
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn an_insert_marked_as_a_reference_must_point_at_one() {
    let (mut doc, _, host) = sample();
    doc.add_with(
        Entity::Insert(Box::new(Insert {
            is_xref: true,
            ..Insert::new(block(BlockId(9), "Other", Vec::new()), Point::default())
        })),
        Attributes::on_layer(host),
    );
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));

    // And one that points at a root must say so.
    let (mut doc, _, host) = sample();
    let root = doc.block(ROOT).unwrap().clone();
    doc.add_with(
        Entity::Insert(Box::new(Insert::new(root, Point::default()))),
        Attributes::on_layer(host),
    );
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_host_block_cannot_contain_a_reference() {
    let (mut doc, _, host) = sample();
    let root = doc.block(ROOT).unwrap().clone();
    let holder = block(
        BlockId(10),
        "Holder",
        vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Insert(Box::new(Insert::new_xref(root, Point::default()))),
            Attributes::on_layer(host),
        )],
    );
    doc.push_block(holder);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_local_block_cycle_is_refused() {
    let (mut doc, _, _) = sample();
    // LOCAL inserts itself, through a new local definition of the same id.
    let cyclic = block(
        LOCAL,
        "Chairs",
        vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Insert(Box::new(Insert::new(local(), Point::default()))),
            Attributes::on_layer(LayerId::DEFAULT),
        )],
    );
    let mut xrefs = doc.xrefs().to_vec();
    xrefs[0].blocks = vec![cyclic];
    doc.set_xrefs(xrefs);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn an_insert_to_a_block_outside_the_reference_is_refused() {
    let (mut doc, _, _) = sample();
    let mut xrefs = doc.xrefs().to_vec();
    xrefs[0].blocks = Vec::new();
    doc.set_xrefs(xrefs);
    assert!(matches!(doc.validate(), Err(DocumentError::BadXref(_))));
}

#[test]
fn a_root_may_hold_a_construction_line_but_a_host_block_may_not() {
    let (mut doc, _, _) = sample();
    let mut root = (**doc.block(ROOT).unwrap()).clone();
    root.entities.push(BlockEntity::new(
        BlockObjectId(9),
        Entity::Xline {
            origin: Point::default(),
            direction: Point::new(1.0, 0.0),
        },
        Attributes::on_layer(LayerId::DEFAULT),
    ));
    let redefinition = doc.redefined_blocks(root).expect("an xref may hold one");
    doc.set_blocks(redefinition.blocks);
    assert_eq!(doc.validate(), Ok(()));

    let host = block(
        BlockId(10),
        "Host",
        vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Xline {
                origin: Point::default(),
                direction: Point::new(1.0, 0.0),
            },
            Attributes::on_layer(LayerId::DEFAULT),
        )],
    );
    assert!(doc.redefined_blocks((*host).clone()).is_err());
}

#[test]
fn an_unloaded_reference_is_an_empty_root_that_stays_valid() {
    let (mut doc, _, _) = sample();
    let mut xrefs = doc.xrefs().to_vec();
    xrefs[0].unloaded = true;
    xrefs[0].blocks = Vec::new();
    let mut root = (**doc.block(ROOT).unwrap()).clone();
    root.entities.clear();
    doc.set_blocks(vec![Arc::new(root)]);
    doc.set_xrefs(xrefs);
    assert_eq!(doc.validate(), Ok(()));
    assert!(doc.xref_is_unloaded(ROOT));

    let empty = doc.block(ROOT).unwrap().clone();
    let insert = Insert::new_xref(empty, Point::new(4.0, 5.0));
    assert_eq!(insert.bounds().min, Point::new(4.0, 5.0));
    assert!(insert.picture().is_empty());
}

#[test]
fn the_root_keeps_layer_zero_and_nested_blocks_inherit_normally() {
    let (doc, walls, host) = sample();
    let root = doc.block(ROOT).unwrap().clone();
    let attrs = Attributes::on_layer(host);

    let xref = Insert::new_xref(root.clone(), Point::default());
    let pieces = xref.picture_with(&attrs, &|_| crate::LayerLook::OUTSIDE_BLOCK);
    let layers: Vec<LayerId> = pieces.iter().map(|p| p.attrs.layer).collect();
    assert!(layers.contains(&LayerId::DEFAULT), "layer 0 stays host 0");
    assert!(layers.contains(&walls), "bound layers stay bound");
    assert!(!layers.contains(&host), "the root's layer is not inherited");

    // An ordinary block of the same definition inherits the insert's layer.
    let block = Insert::new(root, Point::default());
    let pieces = block.picture_with(&attrs, &|_| crate::LayerLook::OUTSIDE_BLOCK);
    assert!(pieces.iter().any(|p| p.attrs.layer == host));
}

#[test]
fn the_clipboard_guard_finds_a_reference_through_a_block() {
    let (mut doc, _, host) = sample();
    let root = doc.block(ROOT).unwrap().clone();
    // A host block cannot hold one, but a drawing read from a bad file can.
    doc.remove_xref(ROOT);
    doc.set_blocks(vec![root.clone()]);
    let outer = block(
        BlockId(10),
        "Outer",
        vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Insert(Box::new(Insert::new_xref(root, Point::default()))),
            Attributes::on_layer(host),
        )],
    );
    doc.push_block(outer.clone());
    let id = doc.add(Entity::Insert(Box::new(Insert::new(
        outer,
        Point::default(),
    ))));
    assert_eq!(doc.xref_reachable_from(&[id]), Some(ROOT));
    assert_eq!(doc.xref_reachable_from(&[]), None);

    let plain = doc.add(Entity::Circle {
        center: Point::default(),
        radius: 1.0,
    });
    assert_eq!(doc.xref_reachable_from(&[plain]), None);
}

#[test]
fn a_root_is_never_edited_or_written_out_as_a_block() {
    let (doc, _, _) = sample();
    assert!(doc.block_editor_document(ROOT).is_none());
    assert!(doc.block_as_drawing(ROOT).is_none());
}

#[test]
fn objects_and_the_table_are_independent_for_unreferenced_roots() {
    let (mut doc, _, _) = sample();
    // Erase every insert: the record and root stay.
    let ids: Vec<_> = doc.objects().iter().map(|o| o.id).collect();
    doc.remove_many(&ids);
    assert!(doc.xref(ROOT).is_some());
    assert_eq!(doc.validate(), Ok(()));
    assert!(!crate::purge::purgeable(&doc)
        .iter()
        .any(|p| p.item == crate::purge::Item::Block(ROOT)));
}

#[test]
fn the_insert_base_and_visretain_round_trip_in_memory() {
    let mut doc = Document::default();
    assert_eq!(doc.insert_base(), Point::default());
    assert!(doc.visretain());
    doc.set_insert_base(Point::new(3.0, 4.0));
    doc.set_visretain(false);
    assert_eq!(doc.insert_base(), Point::new(3.0, 4.0));
    assert!(!doc.visretain());
    let mut copy = doc.clone();
    assert_eq!(copy.insert_base(), Point::new(3.0, 4.0));
    assert!(!copy.visretain());
    copy.set_visretain(true);
    assert!(copy.visretain() && !doc.visretain());
}

#[test]
fn an_object_and_its_reference_are_not_confused() {
    let (mut doc, _, host) = sample();
    let id = doc.add_with(
        Entity::Insert(Box::new(Insert::new_xref(
            doc.block(ROOT).unwrap().clone(),
            Point::default(),
        ))),
        Attributes::on_layer(host),
    );
    assert_eq!(doc.xref_for_object(id).map(|x| x.block), Some(ROOT));
    assert!(doc.is_xref_insert(id));
    assert_eq!(doc.xref_references(ROOT), vec![id]);
    let object = doc.object(id).unwrap();
    assert_eq!(
        doc.xref_of_insert(match &object.entity {
            Entity::Insert(i) => i,
            _ => unreachable!(),
        })
        .map(|x| x.block),
        Some(ROOT)
    );
}
