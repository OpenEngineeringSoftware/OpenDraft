//! Binding and XBIND: names, merges, records and validation.

use std::sync::Arc;

use crate::block::{BlockDef, BlockEntity, BlockObjectId, Insert};
use crate::{
    Attr, Attributes, BindError, BindPlan, BindType, BlockId, Document, Entity, Layer, LayerId,
    MText, Point, TextStyle, XbindSymbol, XrefDef, XrefKind, XrefLayer, XrefPathType, XrefReport,
};

fn host() -> (Document, BlockId, BlockId, LayerId) {
    let mut doc = Document::default();
    let walls = doc.allocate_layer_id();
    doc.push_layer(Layer::new(walls, "base|Walls"));
    let notes = doc.allocate_text_style_id();
    doc.push_text_style(TextStyle::new(notes, "base|Notes"));
    let mut linetypes = doc.linetypes().clone();
    let dash = linetypes.add("base|Dash", "", Vec::new()).unwrap();
    doc.set_linetypes(linetypes);

    let root_id = doc.allocate_block_id();
    let chair_id = doc.allocate_block_id();
    let chair = Arc::new(BlockDef {
        id: chair_id,
        name: "Chair".to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Circle {
                center: Point::default(),
                radius: 1.0,
            },
            Attributes::on_layer(walls),
        )],
        dynamic: None,
    });
    let mut line_attrs = Attributes::on_layer(walls);
    line_attrs.linetype = Attr::Set(dash);
    let root = Arc::new(BlockDef {
        id: root_id,
        name: "base".to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: vec![
            BlockEntity::new(
                BlockObjectId(0),
                Entity::Line {
                    start: Point::default(),
                    end: Point::new(1.0, 0.0),
                },
                line_attrs,
            ),
            BlockEntity::new(
                BlockObjectId(1),
                Entity::MText(Box::new(MText::new(Point::default(), 2.5, "note", notes))),
                Attributes::on_layer(walls),
            ),
            BlockEntity::new(
                BlockObjectId(2),
                Entity::Insert(Box::new(Insert::new(chair.clone(), Point::new(2.0, 0.0)))),
                Attributes::on_layer(walls),
            ),
        ],
        dynamic: None,
    });
    doc.push_block(root.clone());
    doc.set_xrefs(vec![XrefDef {
        block: root_id,
        path: "base.dwg".to_owned(),
        path_type: XrefPathType::Relative,
        kind: XrefKind::Attach,
        unloaded: false,
        stamp: None,
        layers: vec![XrefLayer {
            source: "Walls".to_owned(),
            layer: walls,
        }],
        blocks: vec![chair],
        report: XrefReport::default(),
    }]);
    doc.add_with(
        Entity::Insert(Box::new(Insert::new_xref(root, Point::default()))),
        Attributes::on_layer(LayerId::DEFAULT),
    );
    doc.validate().unwrap();
    (doc, root_id, chair_id, walls)
}

fn apply(doc: &mut Document, plan: BindPlan) {
    for layer in plan.layers {
        doc.replace_layer(layer);
    }
    for layer in plan.removed_layers {
        doc.remove_layer(layer);
    }
    if let Some(styles) = plan.text_styles {
        doc.set_text_styles(styles, plan.current_text_style, plan.next_text_style);
    }
    if let Some(table) = plan.linetypes {
        doc.set_linetypes(table);
    }
    if let Some(layouts) = plan.layouts {
        doc.set_layouts(layouts);
    }
    doc.set_xrefs(plan.xrefs);
    doc.set_blocks(plan.blocks);
    for (id, entity) in plan.entities {
        let _ = doc.replace_many(vec![(id, entity)]);
    }
    for (id, attrs) in plan.attributes {
        doc.replace_attrs(id, attrs);
    }
}

#[test]
fn bind_makes_the_snapshot_and_dependent_symbols_local() {
    let (mut doc, root, _chair, _walls) = host();
    let plan = doc.plan_bind(&[root], BindType::Bind).unwrap();
    assert!(plan.xrefs.is_empty());
    assert_eq!(plan.notes.len(), 1, "the local block was renamed");
    apply(&mut doc, plan);

    assert!(doc.xrefs().is_empty());
    assert_eq!(doc.block(root).map(|b| b.name.as_str()), Some("base"));
    assert!(doc.block_named("base$0$Chair").is_some());
    assert!(doc.layers().iter().any(|l| l.name == "base$0$Walls"));
    assert!(doc.text_styles().iter().any(|s| s.name == "base$0$Notes"));
    assert!(doc.linetypes().named("base$0$Dash").is_some());
    let Entity::Insert(insert) = &doc.objects()[0].entity else {
        panic!("an insert was expected")
    };
    assert!(!insert.is_xref);
    assert_eq!(doc.validate(), Ok(()));
}

#[test]
fn insert_mode_merges_a_layer_and_remaps_its_objects() {
    let (mut doc, root, _chair, walls) = host();
    let own = doc.allocate_layer_id();
    doc.push_layer(Layer::new(own, "Walls"));
    doc.add_with(
        Entity::Circle {
            center: Point::new(9.0, 9.0),
            radius: 0.5,
        },
        Attributes::on_layer(walls),
    );
    doc.validate().unwrap();

    let plan = doc.plan_bind(&[root], BindType::Insert).unwrap();
    assert!(plan.removed_layers.contains(&walls));
    assert!(plan.layers.iter().all(|l| l.id != walls));
    assert_eq!(plan.attributes.len(), 1, "the top-level circle moves");
    apply(&mut doc, plan);

    assert!(doc.layer(walls).is_none());
    assert!(doc
        .layers()
        .iter()
        .any(|l| l.name == "Walls" && l.id == own));
    let root = doc.block(root).unwrap();
    assert!(root.entities.iter().all(|e| e.attrs.layer != walls));
    assert!(doc.objects().iter().any(|o| o.attrs.layer == own));
    assert_eq!(doc.validate(), Ok(()));
}

#[test]
fn xbind_a_layer_unbinds_it_without_dropping_the_reference() {
    let (mut doc, root, _chair, walls) = host();
    let plan = doc
        .plan_xbind(
            &[XbindSymbol::Layer {
                xref: root,
                layer: walls,
            }],
            BindType::Bind,
        )
        .unwrap();
    apply(&mut doc, plan);

    assert_eq!(doc.xrefs().len(), 1);
    assert!(doc.xrefs()[0].layers.is_empty());
    assert!(doc.layer(walls).is_some_and(|l| l.name == "base$0$Walls"));
    assert_eq!(doc.validate(), Ok(()));
}

#[test]
fn xbind_a_block_moves_it_and_the_locals_it_holds() {
    let (mut doc, root, chair, _walls) = host();
    let plan = doc
        .plan_xbind(
            &[XbindSymbol::Block {
                xref: root,
                block: chair,
            }],
            BindType::Bind,
        )
        .unwrap();
    assert!(plan.xrefs[0].blocks.is_empty());
    assert!(plan
        .blocks
        .iter()
        .any(|b| b.id == chair && b.name == "base$0$Chair"));
    apply(&mut doc, plan);

    assert_eq!(doc.xrefs().len(), 1);
    assert!(doc.xrefs()[0].blocks.is_empty());
    assert!(doc.block_named("base$0$Chair").is_some());
    assert_eq!(doc.validate(), Ok(()));
}

#[test]
fn xbind_a_style_and_a_linetype_rename_them() {
    let (mut doc, root, _chair, _walls) = host();
    let style = doc
        .text_styles()
        .iter()
        .find(|s| s.name == "base|Notes")
        .unwrap()
        .id;
    let dash = doc.linetypes().named("base|Dash").unwrap().id;
    let plan = doc
        .plan_xbind(
            &[
                XbindSymbol::TextStyle { xref: root, style },
                XbindSymbol::Linetype {
                    xref: root,
                    linetype: dash,
                },
            ],
            BindType::Bind,
        )
        .unwrap();
    apply(&mut doc, plan);

    assert_eq!(doc.xrefs().len(), 1);
    assert!(doc.text_styles().iter().any(|s| s.name == "base$0$Notes"));
    assert!(doc.linetypes().named("base$0$Dash").is_some());
    assert_eq!(doc.validate(), Ok(()));
}

#[test]
fn an_infinite_line_in_the_snapshot_refuses_binding() {
    let (mut doc, root, _chair, _walls) = host();
    let mut block = (**doc.block(root).unwrap()).clone();
    block.entities.push(BlockEntity::new(
        BlockObjectId(9),
        Entity::Xline {
            origin: Point::default(),
            direction: Point::new(1.0, 0.0),
        },
        Attributes::on_layer(LayerId::DEFAULT),
    ));
    doc.set_blocks(vec![Arc::new(block)]);
    let error = doc.plan_bind(&[root], BindType::Bind).unwrap_err();
    assert!(matches!(
        error,
        BindError::UnsupportedContent {
            what: "construction line",
            ..
        }
    ));
}
