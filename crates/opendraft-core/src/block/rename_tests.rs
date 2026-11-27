//! Renaming a block of the drawing: only the definition's name changes, and
//! every insert follows, in the drawing and inside other blocks.

use super::tests::{near, nested_doc, p};
use super::*;
use crate::Document;

/// `doc` with the change applied, as the transaction does.
fn applied(doc: &mut Document, change: Redefinition) {
    doc.set_blocks(change.blocks);
    for (id, entity) in change.objects {
        doc.replace(id, entity);
    }
}

fn insert_of(doc: &Document, id: crate::EntityId) -> &Insert {
    match doc.get(id) {
        Some(Entity::Insert(i)) => i,
        _ => panic!("an insert"),
    }
}

#[test]
fn renaming_a_block_renames_every_insert_of_it_nested_or_not() {
    let (mut doc, inner, outer) = nested_doc();
    let direct = doc.add(Entity::Insert(Box::new(Insert::new(
        inner.clone(),
        p(5.0, 5.0),
    ))));
    let through = doc.add(Entity::Insert(Box::new(Insert::new(
        outer.clone(),
        p(0.0, 0.0),
    ))));
    let change = doc.renamed_block(inner.id, "  Tile ").expect("a free name");
    applied(&mut doc, change);
    assert_eq!(doc.validate(), Ok(()));
    assert!(doc.block_named("Square").is_none());
    let renamed = doc.block_named("Tile").expect("renamed");
    assert_eq!(renamed.id, inner.id, "the same block, under a new name");
    assert_eq!(renamed.entities, inner.entities);
    assert!(near(renamed.base, inner.base));
    assert_eq!(insert_of(&doc, direct).block.name, "Tile");
    let Some(Entity::Insert(nested)) = insert_of(&doc, through)
        .block
        .entities
        .first()
        .map(|e| &e.entity)
    else {
        panic!("the outer block still holds the inner one")
    };
    assert_eq!(nested.block.name, "Tile", "the nested insert follows");
    assert_eq!(doc.block(outer.id).map(|b| b.name.as_str()), Some("Pair"));
}

#[test]
fn a_name_another_block_has_is_refused_whatever_its_case() {
    let (doc, inner, _) = nested_doc();
    let why = doc.renamed_block(inner.id, "PAIR").expect_err("taken");
    assert_eq!(why, "A block called \"Pair\" already exists.");
}

#[test]
fn changing_only_the_case_is_allowed() {
    let (mut doc, inner, _) = nested_doc();
    let change = doc.renamed_block(inner.id, "SQUARE").expect("its own name");
    applied(&mut doc, change);
    assert_eq!(doc.validate(), Ok(()));
    assert_eq!(doc.block(inner.id).map(|b| b.name.as_str()), Some("SQUARE"));
}

#[test]
fn names_a_block_cannot_hold_are_refused() {
    let (doc, inner, _) = nested_doc();
    for name in ["", "   ", "*U12", "a/b", "x<y"] {
        assert!(doc.renamed_block(inner.id, name).is_err(), "{name:?}");
    }
    assert!(doc.renamed_block(BlockId(999), "Free").is_err(), "unknown");
}

#[test]
fn renaming_keeps_attribute_values_and_dynamic_state() {
    let (mut doc, inner, _) = nested_doc();
    let mut insert = Insert::new(inner.clone(), p(0.0, 0.0));
    insert.scale_x = 2.0;
    insert.rotation = 0.5;
    let id = doc.add(Entity::Insert(Box::new(insert.clone())));
    let change = doc.renamed_block(inner.id, "Tile").expect("free");
    applied(&mut doc, change);
    let after = insert_of(&doc, id);
    assert_eq!(after.scale_x, 2.0);
    assert_eq!(after.rotation, 0.5);
    assert_eq!(after.attributes, insert.attributes);
    assert_eq!(after.dynamic, insert.dynamic);
}
