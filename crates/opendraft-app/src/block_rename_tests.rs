use std::sync::Arc;

use opendraft_core::{BlockDef, BlockId, Entity, Insert, Point};

use crate::library_cmd_tests::drawing_with_box;
use crate::state::AppState;

/// A drawing with block "Box" and two inserts of it, and the block's id.
fn two_boxes(dir: &std::path::Path) -> (AppState, BlockId) {
    let mut state = drawing_with_box(dir, 5.0);
    let block: Arc<BlockDef> = state
        .session()
        .document()
        .block_named("Box")
        .cloned()
        .expect("BLOCK made it");
    for x in [40.0, 80.0] {
        let insert = Insert::new(block.clone(), Point::new(x, 0.0));
        state.add_entity(Entity::Insert(Box::new(insert)));
    }
    (state, block.id)
}

/// The block names the inserts of the active drawing show.
fn inserted_names(state: &AppState) -> Vec<String> {
    state
        .session()
        .document()
        .objects()
        .iter()
        .filter_map(|o| match &o.entity {
            Entity::Insert(i) => Some(i.block.name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn renaming_a_drawing_block_is_one_undo_step_that_every_insert_follows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut state, id) = two_boxes(dir.path());
    let before = inserted_names(&state);
    assert!(before.len() >= 2 && before.iter().all(|n| n == "Box"));

    let said = state.rename_block(id, " Crate ");
    assert_eq!(said, Ok(Some("Box".to_owned())));
    let doc = state.session().document();
    assert_eq!(doc.block(id).map(|b| b.name.as_str()), Some("Crate"));
    assert!(doc.block_named("Box").is_none());
    assert!(inserted_names(&state).iter().all(|n| n == "Crate"));

    assert_eq!(state.undo().as_deref(), Some("Rename block"));
    assert_eq!(inserted_names(&state), before);
    assert_eq!(
        state
            .session()
            .document()
            .block(id)
            .map(|b| b.name.as_str()),
        Some("Box")
    );
    state.redo();
    assert!(inserted_names(&state).iter().all(|n| n == "Crate"));
}

#[test]
fn a_refused_name_leaves_the_drawing_and_its_history_alone() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut state, id) = two_boxes(dir.path());
    let revision = state.session().revision();
    for name in ["", "*U3", "a:b"] {
        assert!(state.rename_block(id, name).is_err(), "{name:?}");
    }
    assert_eq!(state.rename_block(id, "Box"), Ok(None), "unchanged");
    assert_eq!(state.session().revision(), revision);
    assert_eq!(
        state.rename_block(BlockId(9999), "Crate"),
        Err("That block is no longer in the drawing.".to_owned())
    );
}

#[test]
fn a_renamed_block_survives_saving_to_dxf_and_reopening() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut state, id) = two_boxes(dir.path());
    state.rename_block(id, "Crate").expect("a free name");
    let path = dir.path().join("renamed.dxf");
    opendraft_files::save(&path, state.session().document()).expect("saves");
    let back = opendraft_files::load(&path).expect("opens").document;
    assert!(back.block_named("Crate").is_some());
    assert!(back.block_named("Box").is_none());
    let names: Vec<&str> = back
        .objects()
        .iter()
        .filter_map(|o| match &o.entity {
            Entity::Insert(i) => Some(i.block.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(names.len() >= 2 && names.iter().all(|&n| n == "Crate"));
}
