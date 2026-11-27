use std::sync::Arc;

use opendraft_core::{
    Attributes, BlockDef, BlockEntity, BlockId, BlockObjectId, Document, Entity, InsertUnits, Point,
};
use opendraft_files::{DxfFormat, FileFormat};

use super::dxf::{ascii, name_from_file};
use super::*;
use crate::test_support::{add_block, drawing_with, drawing_with_nested};

fn mm() -> InsertUnits {
    InsertUnits::from_code(4).expect("millimetres")
}

/// Block `name` with a circle and a line, base point (5, 5), in millimetres.
fn valve(name: &str, description: &str) -> (Document, BlockId) {
    let mut doc = Document::default();
    let layer = doc.layers()[0].id;
    let object =
        |i: u64, entity| BlockEntity::new(BlockObjectId(i), entity, Attributes::on_layer(layer));
    let id = doc.allocate_block_id();
    doc.push_block(Arc::new(BlockDef {
        unit: mm(),
        id,
        name: name.into(),
        base: Point::new(5.0, 5.0),
        description: description.into(),
        entities: vec![
            object(
                0,
                Entity::Circle {
                    center: Point::new(5.0, 5.0),
                    radius: 2.0,
                },
            ),
            object(
                1,
                Entity::Line {
                    start: Point::new(3.0, 5.0),
                    end: Point::new(7.0, 5.0),
                },
            ),
        ],
        dynamic: None,
    }));
    (doc, id)
}

fn one(read: DxfBlock) -> BlockFile {
    match read {
        DxfBlock::One { file, .. } => *file,
        DxfBlock::NoModelSpace => unreachable!("expected one block"),
    }
}

#[test]
fn our_own_file_comes_back_as_the_exact_block() {
    let (doc, id) = valve("Valve", "A gate valve.\nShown in plan.");
    let tags = ["plumbing".to_owned(), "valve".to_owned()];
    let (file, _) = BlockFile::from_block(&doc, id, &tags).expect("a block");
    let (bytes, _) = file.to_dxf().expect("a dxf");
    // The file is named for something else: the marker wins.
    let back = one(BlockFile::from_dxf(&bytes, "export-2").expect("read"));
    assert_eq!(back.name(), "Valve");
    assert_eq!(back.uuid(), file.uuid());
    assert_eq!(back.tags(), tags);
    let def = back.definition().expect("def");
    assert_eq!(def.description, "A gate valve.\nShown in plan.");
    assert_eq!(def.base, Point::new(5.0, 5.0));
    assert_eq!(def.unit, mm());
    assert_eq!(def.entities.len(), 2);
}

#[test]
fn a_name_and_description_beyond_ascii_survive() {
    let (doc, id) = valve(
        "Vanne \u{e9}t\u{e9} \u{1f527}",
        "Robinet \u{e0} bille \u{2014} DN 25",
    );
    let (file, _) =
        BlockFile::from_block(&doc, id, &["\u{e9}quipement".to_owned()]).expect("block");
    let (bytes, _) = file.to_dxf().expect("a dxf");
    let back = one(BlockFile::from_dxf(&bytes, "x").expect("read"));
    assert_eq!(back.name(), "Vanne \u{e9}t\u{e9} \u{1f527}");
    assert_eq!(
        back.definition().expect("def").description,
        "Robinet \u{e0} bille \u{2014} DN 25"
    );
    assert_eq!(back.tags(), ["\u{e9}quipement".to_owned()]);
}

#[test]
fn a_block_that_nests_another_round_trips() {
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let (file, _) = BlockFile::from_block(&doc, outer, &[]).expect("a block");
    let (bytes, _) = file.to_dxf().expect("a dxf");
    let back = one(BlockFile::from_dxf(&bytes, "bolt").expect("read"));
    assert_eq!(back.name(), "bolt");
    assert_eq!(back.drawing().blocks().len(), 2);
    assert!(back.drawing().block_named("nut").is_some());
    let components = back
        .package()
        .entries
        .iter()
        .filter(|e| e.component)
        .count();
    assert_eq!(components, 1);
}

/// A DXF as another program writes it: no marker, the symbol in model space.
fn foreign() -> Vec<u8> {
    let (doc, id) = valve("Valve", "");
    let drawing = doc.block_as_drawing(id).expect("a drawing");
    DxfFormat.write(&drawing).expect("a dxf").bytes
}

#[test]
fn a_foreign_file_is_one_block_named_after_the_file() {
    let bytes = foreign();
    let back = one(BlockFile::from_dxf(&bytes, "Sink").expect("read"));
    assert_eq!(back.name(), "Sink");
    assert!(back.tags().is_empty());
    let def = back.definition().expect("def");
    assert_eq!(def.entities.len(), 2);
    // A plain drawing writes `$INSBASE` as the origin, and this one is R12,
    // which has no `$INSUNITS`.
    assert_eq!(def.base, Point::new(0.0, 0.0));
    assert_eq!(def.unit, InsertUnits::UNITLESS);
}

#[test]
fn a_foreign_file_with_blocks_brings_the_ones_its_symbol_uses() {
    let (mut doc, _, inner) = drawing_with_nested("bolt", "nut");
    let spare = add_block(&mut doc, "spare", "Symbols", 9.0, &[]);
    let outer = doc.block_named("bolt").expect("bolt").clone();
    // The symbol: one insert of "bolt" in model space.
    let mut insert = opendraft_core::Insert::new(outer, Point::new(0.0, 0.0));
    insert.scale_x = 2.0;
    doc.add(Entity::Insert(Box::new(insert)));
    let bytes = DxfFormat.write(&doc).expect("a dxf").bytes;
    let back = one(BlockFile::from_dxf(&bytes, "Fitting").expect("read"));
    assert_eq!(back.name(), "Fitting");
    let names: Vec<_> = back
        .drawing()
        .blocks()
        .iter()
        .map(|b| b.name.clone())
        .collect();
    assert!(names.contains(&"bolt".to_owned()), "{names:?}");
    assert!(names.contains(&"nut".to_owned()), "{names:?}");
    assert!(
        !names.contains(&spare.name),
        "a block the symbol does not use is left behind"
    );
    let _ = inner;
}

#[test]
fn a_file_named_like_a_block_it_holds_gets_a_free_name() {
    let (mut doc, outer, _) = drawing_with_nested("bolt", "nut");
    let bolt = doc.block(outer).expect("bolt").clone();
    doc.add(Entity::Insert(Box::new(opendraft_core::Insert::new(
        bolt,
        Point::new(0.0, 0.0),
    ))));
    let bytes = DxfFormat.write(&doc).expect("a dxf").bytes;
    let back = one(BlockFile::from_dxf(&bytes, "bolt").expect("read"));
    assert_eq!(
        back.name(),
        "bolt_2",
        "inserting the file must not nest a block in itself"
    );
}

#[test]
fn an_empty_model_space_is_not_one_block() {
    let (doc, _) = drawing_with("only", 1.0);
    let bytes = DxfFormat.write(&doc).expect("a dxf").bytes;
    assert!(matches!(
        BlockFile::from_dxf(&bytes, "Library").expect("read"),
        DxfBlock::NoModelSpace
    ));
}

#[test]
fn a_marker_that_is_not_ours_is_ignored() {
    let (doc, id) = valve("Valve", "");
    let (file, _) = BlockFile::from_block(&doc, id, &[]).expect("a block");
    let (bytes, _) = file.to_dxf().expect("a dxf");
    let text = String::from_utf8(bytes).expect("ascii");
    let damaged = text
        .replace("opendraft.block\\\"", "something.else\\\"")
        .replace(
            "\"kind\":\"opendraft.block\"",
            "\"kind\":\"something.else\"",
        );
    let back = one(BlockFile::from_dxf(damaged.as_bytes(), "Pump").expect("read"));
    assert_eq!(back.name(), "Pump", "read as a foreign file");
    assert_ne!(back.uuid(), file.uuid());
    // The base point and the unit come from `$INSBASE` and `$INSUNITS`.
    let def = back.definition().expect("def");
    assert_eq!(def.base, Point::new(5.0, 5.0));
    assert_eq!(def.unit, mm());
}

#[test]
fn a_file_name_that_cannot_be_a_block_name_is_made_into_one() {
    assert_eq!(name_from_file("Sink"), "Sink");
    assert_eq!(name_from_file("a/b:c"), "a_b_c");
    assert_eq!(name_from_file("  "), "Block");
    assert_eq!(name_from_file(""), "Block");
    assert_eq!(name_from_file(&"x".repeat(400)).chars().count(), 255);
}

#[test]
fn something_that_is_not_a_dxf_is_refused() {
    assert!(matches!(
        BlockFile::from_dxf(b"hello", "x").err(),
        Some(OblockError::Refused(_))
    ));
}

#[test]
fn the_marker_is_ascii_on_one_line() {
    let text = ascii("caf\u{e9} \u{1f527}\nx");
    assert!(text.is_ascii());
    assert_eq!(text, "caf\\u00e9 \\ud83d\\udd27\nx");
}
