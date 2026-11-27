use opendraft_content::oblock::BlockFile;
use opendraft_core::{Entity, Point};

use super::*;
use crate::files::Choice;
use crate::picker::{Script, ScriptedPicker};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// A drawing with block "Box" in it, as a block file at `dir/Box.oblock`.
fn box_file(dir: &Path) -> PathBuf {
    let mut state = AppState::new();
    state.add_entity(Entity::Rectangle {
        min: p(10.0, 10.0),
        max: p(12.0, 11.0),
    });
    state.select_all();
    state.start_command("BLOCK", crate::block_cmd::define_block);
    state.settle_command();
    for answer in ["Box", "10,10"] {
        state.cmd_submit(answer);
        state.settle_command();
    }
    let doc = state.docs[state.active].session.document();
    let id = doc.block_named("Box").expect("the block").id;
    let (file, _) = BlockFile::from_block(doc, id, &[]).expect("a block file");
    let path = dir.join("Box.oblock");
    file.save(&path).expect("save");
    path
}

fn state_with_picker() -> (AppState, Script) {
    let mut state = AppState::new();
    let script = Script::default();
    state.picker = Box::new(ScriptedPicker(script.clone()));
    (state, script)
}

fn editor_objects(state: &AppState) -> usize {
    state.docs[state.active].session.document().len()
}

#[test]
fn a_block_file_opens_in_the_block_editor_and_nowhere_else() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    assert!(state.open_path(&path));
    assert!(state.in_block_editor());
    let doc = &state.docs[state.active];
    assert!(doc.oblock.is_some());
    assert_eq!(doc.path.as_deref(), Some(path.as_path()));
    assert_eq!(doc.title, "Box");
    assert!(doc.tab_title().ends_with("Block Editor: Box"));
    assert!(!doc.is_modified());
    // The editor shows the block at the origin.
    let b = doc.session.document().objects()[0].entity.bounds();
    assert_eq!((b.min, b.max), (p(0.0, 0.0), p(2.0, 1.0)));
}

#[test]
fn a_file_that_is_not_a_block_is_refused_and_opens_nothing() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("bad.oblock");
    std::fs::write(&path, b"{\"format\": \"odraft\"}").expect("write");
    let (mut state, _) = state_with_picker();
    let tabs = state.docs.len();
    assert!(!state.open_path(&path));
    assert_eq!(state.docs.len(), tabs);
    assert!(!state.in_block_editor());
    assert!(state.message.contains("Could not open"));
}

#[test]
fn save_writes_the_edited_block_back_to_the_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    state.open_path(&path);
    let uuid = state.docs[state.active]
        .oblock
        .as_ref()
        .expect("link")
        .file
        .uuid();
    state.add_entity(Entity::Line {
        start: p(0.0, 0.0),
        end: p(3.0, 3.0),
    });
    assert!(state.docs[state.active].is_modified());
    assert!(state.save_active());
    assert!(!state.docs[state.active].is_modified());
    let back = BlockFile::load(&path).expect("load");
    assert_eq!(back.definition().expect("def").entities.len(), 2);
    assert_eq!(back.uuid(), uuid, "the block keeps its id");
    assert!(!dir.path().join("Box.oblock.tmp").exists());
}

#[test]
fn save_as_writes_a_new_file_and_the_tab_follows_it() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, script) = state_with_picker();
    state.open_path(&path);
    state.add_entity(Entity::Circle {
        center: p(1.0, 1.0),
        radius: 1.0,
    });
    let other = dir.path().join("Crate");
    script.will_save_to(Some(other));
    assert!(state.save_active_as());
    let written = dir.path().join("Crate.oblock");
    assert_eq!(
        BlockFile::load(&written)
            .expect("load")
            .definition()
            .expect("def")
            .entities
            .len(),
        2
    );
    assert_eq!(
        BlockFile::load(&path)
            .expect("old")
            .definition()
            .expect("def")
            .entities
            .len(),
        1
    );
    let doc = &state.docs[state.active];
    assert_eq!(doc.path.as_deref(), Some(written.as_path()));
    assert_eq!(doc.title, "Crate");
    assert_eq!(script.filters(), ["OpenDraft block"]);
}

#[test]
fn save_as_to_another_kind_of_file_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, script) = state_with_picker();
    state.open_path(&path);
    script.will_save_to(Some(dir.path().join("Box.dxf")));
    assert!(!state.save_active_as());
    let doc = &state.docs[state.active];
    assert_eq!(doc.path.as_deref(), Some(path.as_path()));
    assert!(!dir.path().join("Box.dxf").exists());
}

#[test]
fn bclose_closes_the_tab_and_asks_first_when_there_are_changes() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    state.open_path(&path);
    state.bclose();
    assert!(
        state.docs.iter().all(|d| d.oblock.is_none()),
        "closed at once"
    );

    state.open_path(&path);
    state.add_entity(Entity::Line {
        start: p(0.0, 0.0),
        end: p(3.0, 3.0),
    });
    state.bclose();
    assert!(state.confirm.is_some(), "asked about the unsaved change");
    assert!(state.docs.iter().any(|d| d.oblock.is_some()), "still open");
    state.answer_confirm(Choice::Save);
    assert!(
        state.docs.iter().all(|d| d.oblock.is_none()),
        "closed after saving"
    );
    let back = BlockFile::load(&path).expect("load");
    assert_eq!(back.definition().expect("def").entities.len(), 2);
}

#[test]
fn discarding_leaves_the_file_as_it_was() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    state.open_path(&path);
    state.add_entity(Entity::Line {
        start: p(0.0, 0.0),
        end: p(3.0, 3.0),
    });
    state.request_close(state.active);
    state.answer_confirm(Choice::Discard);
    assert!(state.docs.iter().all(|d| d.oblock.is_none()));
    let back = BlockFile::load(&path).expect("load");
    assert_eq!(back.definition().expect("def").entities.len(), 1);
}

#[test]
fn opening_the_same_file_again_uses_the_open_tab() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    state.open_path(&path);
    let tabs = state.docs.len();
    state.open_path(&path);
    assert_eq!(state.docs.len(), tabs);
    assert_eq!(editor_objects(&state), 1);
}

#[test]
fn a_block_file_is_not_autosaved_as_a_drawing() {
    let dir = tempfile::tempdir().expect("dir");
    let path = box_file(dir.path());
    let (mut state, _) = state_with_picker();
    state.open_path(&path);
    state.add_entity(Entity::Line {
        start: p(0.0, 0.0),
        end: p(3.0, 3.0),
    });
    assert!(state.docs[state.active].is_modified());
    assert_eq!(state.docs[state.active].autosaved, None);
}

#[test]
fn names_of_block_files() {
    assert!(is_oblock(Path::new("a/Valve.OBLOCK")));
    assert!(!is_oblock(Path::new("a/Valve.odraft")));
    assert_eq!(
        oblock_path(PathBuf::from("Valve")),
        Ok(PathBuf::from("Valve.oblock"))
    );
    assert!(oblock_path(PathBuf::from("Valve.dxf")).is_err());
}
