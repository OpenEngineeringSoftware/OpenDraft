use std::sync::Arc;

use crate::block::{BlockEntity, BlockObjectId, Insert};
use crate::{Attributes, BlockDef, Document, Entity, InsertUnits, Layer, Point};

/// A drawing with a layer `layer` (after some padding layers, so the ids differ
/// from drawing to drawing) and block "Valve": a circle of `radius`.
fn drawing(padding: usize, layer: &str, radius: f64) -> (Document, Arc<BlockDef>) {
    let mut doc = Document::default();
    for i in 0..padding {
        let id = doc.allocate_layer_id();
        doc.push_layer(Layer::new(id, format!("Pad{i}")));
    }
    let id = doc.allocate_layer_id();
    doc.push_layer(Layer::new(id, layer));
    let block_id = doc.allocate_block_id();
    let def = Arc::new(BlockDef {
        unit: InsertUnits::UNITLESS,
        id: block_id,
        name: "Valve".into(),
        base: Point::new(1.0, 1.0),
        description: String::new(),
        entities: vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Circle {
                center: Point::new(1.0, 1.0),
                radius,
            },
            Attributes::on_layer(id),
        )],
        dynamic: None,
    });
    doc.push_block(def.clone());
    (doc, def)
}

fn same(a: &(Document, Arc<BlockDef>), b: &(Document, Arc<BlockDef>)) -> bool {
    a.1.same_as(&a.0, &b.1, &b.0)
}

#[test]
fn the_same_block_in_two_drawings_is_the_same_whatever_the_ids() {
    let a = drawing(0, "Symbols", 5.0);
    let b = drawing(3, "SYMBOLS", 5.0);
    assert!(same(&a, &b));
    assert!(same(&b, &a));
}

#[test]
fn another_shape_base_unit_or_layer_is_another_block() {
    let a = drawing(0, "Symbols", 5.0);
    assert!(!same(&a, &drawing(0, "Symbols", 6.0)));
    assert!(!same(&a, &drawing(0, "Plumbing", 5.0)));
    let mut moved = drawing(0, "Symbols", 5.0);
    let mut def = (*moved.1).clone();
    def.base = Point::new(0.0, 0.0);
    moved.1 = Arc::new(def);
    assert!(!same(&a, &moved));
    let mut unit = drawing(0, "Symbols", 5.0);
    let mut def = (*unit.1).clone();
    def.unit = InsertUnits::from_code(4).expect("millimetres");
    unit.1 = Arc::new(def);
    assert!(!same(&a, &unit));
}

#[test]
fn a_nested_block_is_compared_by_its_name_and_placement() {
    let make = |at: f64, inner_name: &str| {
        let (mut doc, _) = drawing(0, "Symbols", 5.0);
        let inner = doc.block_named("Valve").expect("valve").clone();
        let layer = doc.layers()[0].id;
        let mut insert = Insert::new(inner, Point::new(at, 0.0));
        insert.scale_x = 2.0;
        let id = doc.allocate_block_id();
        let def = Arc::new(BlockDef {
            unit: InsertUnits::UNITLESS,
            id,
            name: inner_name.into(),
            base: Point::new(0.0, 0.0),
            description: String::new(),
            entities: vec![BlockEntity::new(
                BlockObjectId(0),
                Entity::Insert(Box::new(insert)),
                Attributes::on_layer(layer),
            )],
            dynamic: None,
        });
        doc.push_block(def.clone());
        (doc, def)
    };
    assert!(same(&make(1.0, "Pair"), &make(1.0, "Pair")));
    assert!(!same(&make(1.0, "Pair"), &make(2.0, "Pair")));
}
