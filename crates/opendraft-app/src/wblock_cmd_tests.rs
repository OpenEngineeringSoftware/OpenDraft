use opendraft_content::oblock::BlockFile;
use opendraft_core::{Entity, Point};

use super::*;
use crate::block_cmd::define_block;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn submit_all(state: &mut AppState, answers: &[&str]) {
    for answer in answers {
        state.cmd_submit(answer);
        state.settle_command();
    }
}

fn start(state: &mut AppState, folder: &std::path::Path) {
    let folder = folder.to_owned();
    state.start_command("WBLOCK", move |ed| wblock(ed, Some(folder.clone())));
    state.settle_command();
}

/// A drawing with a rectangle, selected.
fn with_rectangle() -> AppState {
    let mut state = AppState::new();
    state.add_entity(Entity::Rectangle {
        min: p(10.0, 10.0),
        max: p(12.0, 11.0),
    });
    state.select_all();
    state
}

/// A drawing with block "Box", nothing selected.
fn with_box() -> AppState {
    let mut state = with_rectangle();
    state.start_command("BLOCK", define_block);
    state.settle_command();
    submit_all(&mut state, &["Box", "10,10"]);
    state.docs[state.active].selection.clear();
    state
}

fn said(state: &AppState, text: &str) -> bool {
    state.cmdline.log.iter().any(|l| l.contains(text))
}

#[test]
fn the_selected_objects_are_written_as_a_block_file() {
    let dir = tempfile::tempdir().expect("dir");
    let mut state = with_rectangle();
    let before = state.docs[state.active].session.document().len();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", "10,10", "Crate"]);
    let file = BlockFile::load(&dir.path().join("Crate.oblock")).expect("the file");
    assert_eq!(file.name(), "Box");
    assert_eq!(file.definition().expect("def").entities.len(), 1);
    assert!(said(&state, "written to"));
    assert_eq!(state.docs[state.active].session.document().len(), before);
    assert!(state.docs[state.active]
        .session
        .document()
        .blocks()
        .is_empty());
}

#[test]
fn a_block_of_the_drawing_is_written_with_its_own_name_by_default() {
    let dir = tempfile::tempdir().expect("dir");
    let mut state = with_box();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", ""]);
    let file = BlockFile::load(&dir.path().join("Box.oblock")).expect("the file");
    assert_eq!(file.name(), "Box");
    let base = file.definition().expect("def").base;
    assert_eq!(base, p(10.0, 10.0));
}

#[test]
fn a_file_that_exists_is_replaced_only_when_asked() {
    let dir = tempfile::tempdir().expect("dir");
    let target = dir.path().join("Box.oblock");
    std::fs::write(&target, b"keep me").expect("write");
    let mut state = with_box();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", "", "No", "Other"]);
    assert_eq!(std::fs::read(&target).expect("read"), b"keep me");
    assert!(dir.path().join("Other.oblock").exists());

    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", "", "Yes"]);
    assert!(BlockFile::load(&target).is_ok());
}

#[test]
fn another_kind_of_file_name_is_refused_and_asked_again() {
    let dir = tempfile::tempdir().expect("dir");
    let mut state = with_box();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", "Box.txt"]);
    assert!(said(&state, "ends in .oblock"));
    assert!(!dir.path().join("Box.txt").exists());
    submit_all(&mut state, &["Fine"]);
    assert!(dir.path().join("Fine.oblock").exists());
}

#[test]
fn a_block_that_is_not_there_writes_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let mut state = with_box();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Nope"]);
    assert!(said(&state, "not found"));
    assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);
}

#[test]
fn a_dxf_name_writes_a_dxf_that_comes_back_as_the_same_block() {
    let dir = tempfile::tempdir().expect("dir");
    let mut state = with_box();
    start(&mut state, dir.path());
    submit_all(&mut state, &["Box", "Box.dxf"]);
    let path = dir.path().join("Box.dxf");
    assert!(said(&state, "written to"));
    let bytes = std::fs::read(&path).expect("the dxf");
    let opendraft_content::oblock::DxfBlock::One { file, .. } =
        BlockFile::from_dxf(&bytes, "other").expect("read")
    else {
        unreachable!("the symbol is in model space");
    };
    assert_eq!(file.name(), "Box");
    assert_eq!(file.definition().expect("def").base, p(10.0, 10.0));
}
