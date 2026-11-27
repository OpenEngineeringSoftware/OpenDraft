//! LIBADD and LIBINSERT, driven through `AppState` like the INSERT tests.

use opendraft_core::{Entity, Point};

use crate::block_cmd::define_block;
use crate::library_add_cmd::library_add;
use crate::library_insert_cmd::library_insert;
use crate::state::AppState;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn submit_all(state: &mut AppState, answers: &[&str]) {
    for answer in answers {
        state.cmd_submit(answer);
        state.settle_command();
    }
}

/// An app whose library is in `folder`.
pub(crate) fn app_with_library(folder: &std::path::Path) -> AppState {
    let state = AppState::new();
    state.defaults.library.set_folder(Some(folder.to_owned()));
    state
}

/// A drawing with block "Box": a rectangle of the given width, base at its
/// lower left corner.
pub(crate) fn drawing_with_box(folder: &std::path::Path, width: f64) -> AppState {
    let mut state = app_with_library(folder);
    state.add_entity(Entity::Rectangle {
        min: p(10.0, 10.0),
        max: p(10.0 + width, 11.0),
    });
    state.select_all();
    state.start_command("BLOCK", define_block);
    state.settle_command();
    submit_all(&mut state, &["Box", "10,10"]);
    assert!(!state.command_running());
    state
}

/// The width of the rectangle a test block holds.
fn rect_width(def: &opendraft_core::BlockDef) -> f64 {
    match def.entities.first().map(|e| &e.entity) {
        Some(Entity::Rectangle { min, max }) => max.x - min.x,
        _ => f64::NAN,
    }
}

fn library_names(state: &AppState) -> Vec<String> {
    state
        .defaults
        .library
        .with(|lib| lib.blocks().into_iter().map(|b| b.name).collect())
        .expect("library opens")
}

fn block_names(state: &AppState) -> Vec<String> {
    state
        .session()
        .document()
        .blocks()
        .iter()
        .map(|b| b.name.clone())
        .collect()
}

fn said(state: &AppState, text: &str) -> bool {
    state.cmdline.log.iter().any(|l| l.contains(text))
}

#[test]
fn a_block_of_the_drawing_is_added_by_name_and_the_drawing_is_unchanged() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = drawing_with_box(dir.path(), 2.0);
    let objects = state.session().document().len();

    state.start_command("LIBADD", library_add);
    state.settle_command();
    submit_all(&mut state, &["Box", ""]);
    assert!(!state.command_running());
    assert_eq!(library_names(&state), ["Box"]);
    assert!(said(&state, "added to the library"));
    assert_eq!(state.session().document().len(), objects);
    assert_eq!(block_names(&state), ["Box"]);
}

#[test]
fn selected_objects_are_made_into_a_library_block_without_touching_the_drawing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = app_with_library(dir.path());
    state.add_entity(Entity::Rectangle {
        min: p(0.0, 0.0),
        max: p(4.0, 2.0),
    });
    state.select_all();
    state.start_command("LIBADD", library_add);
    state.settle_command();
    submit_all(&mut state, &["Plate", "0,0", "Steel plate"]);
    assert!(!state.command_running());
    assert_eq!(library_names(&state), ["Steel plate"]);
    assert!(block_names(&state).is_empty(), "no block in the drawing");
    assert_eq!(state.session().document().len(), 1);
}

#[test]
fn a_taken_name_asks_and_both_adds_the_new_one_under_another_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = drawing_with_box(dir.path(), 2.0);
    for _ in 0..2 {
        state.start_command("LIBADD", library_add);
        state.settle_command();
        submit_all(&mut state, &["Box", ""]);
        if state.command_running() {
            // The second time the name is taken: keep both.
            state.answer_keyword("Both");
            state.settle_command();
        }
        assert!(!state.command_running());
    }
    assert_eq!(library_names(&state), ["Box", "Box_2"]);
}

#[test]
fn a_taken_name_can_be_cancelled() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = drawing_with_box(dir.path(), 2.0);
    for answer in [None, Some("Cancel")] {
        state.start_command("LIBADD", library_add);
        state.settle_command();
        submit_all(&mut state, &["Box", ""]);
        if let Some(keyword) = answer {
            state.answer_keyword(keyword);
            state.settle_command();
        }
        assert!(!state.command_running());
    }
    assert_eq!(library_names(&state), ["Box"]);
}

#[test]
fn an_empty_library_says_so() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = app_with_library(dir.path());
    state.start_command("LIBINSERT", library_insert);
    state.settle_command();
    assert!(!state.command_running());
    assert!(said(&state, "library has no blocks"));
}

#[test]
fn no_library_folder_says_so_and_does_not_fail() {
    let mut state = AppState::new();
    state.start_command("LIBINSERT", library_insert);
    state.settle_command();
    assert!(!state.command_running());
    assert!(said(&state, "no library folder"));
}

/// Adds "Box" (width `width`) to the library in `dir`.
pub(crate) fn stocked(dir: &std::path::Path, width: f64) {
    let mut state = drawing_with_box(dir, width);
    state.start_command("LIBADD", library_add);
    state.settle_command();
    submit_all(&mut state, &["Box", ""]);
    assert!(!state.command_running());
}

#[test]
fn a_library_block_is_placed_in_another_drawing_in_one_undo_step() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);

    let mut other = app_with_library(dir.path());
    assert!(block_names(&other).is_empty());
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    // Nothing is in the drawing until the point is picked.
    assert!(other.command_running());
    assert!(block_names(&other).is_empty());
    assert_eq!(other.session().document().len(), 0);
    submit_all(&mut other, &["50,50"]);
    assert!(!other.command_running());

    let doc = other.session().document();
    let Some(Entity::Insert(i)) = doc.objects().last().map(|o| &o.entity) else {
        unreachable!("an insert was added")
    };
    assert_eq!((i.block.name.as_str(), i.insertion), ("Box", p(50.0, 50.0)));
    assert_eq!(block_names(&other), ["Box"]);
    assert_eq!(doc.validate(), Ok(()));

    other.undo();
    assert_eq!(other.session().document().len(), 0);
    assert!(block_names(&other).is_empty());
    other.redo();
    assert_eq!(other.session().document().len(), 1);
    assert_eq!(block_names(&other), ["Box"]);
    // Recent now names it.
    let recent = other
        .defaults
        .library
        .with(|l| l.recents().len())
        .expect("opens");
    assert_eq!(recent, 1);
}

#[test]
fn a_taken_name_is_added_as_new_by_default_and_the_drawings_block_is_left_alone() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);

    // Another drawing has its own, wider "Box".
    let mut other = drawing_with_box(dir.path(), 9.0);
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    assert!(other.cmd_prompt().text.contains("already has"));
    // New, not kept for the session, then the point.
    submit_all(&mut other, &["", "", "0,0"]);
    assert!(!other.command_running());
    assert_eq!(block_names(&other), ["Box", "Box_2"]);
    let doc = other.session().document();
    let widest = |name: &str| doc.block_named(name).map(|b| rect_width(b));
    assert_eq!(widest("Box"), Some(9.0));
    assert_eq!(widest("Box_2"), Some(2.0));
}

#[test]
fn replace_changes_the_definition_every_reference_follows_and_undo_restores_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);

    let mut other = drawing_with_box(dir.path(), 9.0);
    // A reference to the drawing's own wide Box.
    other.start_command("INSERT", crate::insert_cmd::insert_block);
    other.settle_command();
    submit_all(&mut other, &["Box", "100,100"]);
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    other.answer_keyword("Replace");
    other.settle_command();
    submit_all(&mut other, &["", "0,0"]);
    assert!(!other.command_running());
    assert!(said(&other, "definition was replaced"));
    assert_eq!(block_names(&other), ["Box"]);
    let width = |state: &AppState| {
        state
            .session()
            .document()
            .block_named("Box")
            .map(|b| rect_width(b))
    };
    assert_eq!(width(&other), Some(2.0));
    assert_eq!(other.session().document().validate(), Ok(()));

    other.undo();
    assert_eq!(width(&other), Some(9.0));
    assert_eq!(other.session().document().validate(), Ok(()));
}

#[test]
fn escape_before_the_point_leaves_the_drawing_untouched() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);
    let mut other = app_with_library(dir.path());
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    other.cancel_command();
    other.settle_command();
    assert!(!other.command_running());
    assert!(block_names(&other).is_empty());
    assert_eq!(other.session().document().len(), 0);
    assert!(!other.session().history().can_undo());
}

#[test]
fn an_unknown_name_is_asked_again() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);
    let mut other = app_with_library(dir.path());
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Nothing"]);
    assert!(other.command_running());
    assert!(said(&other, "\"Nothing\" is not in the library"));
}

#[test]
fn a_block_the_drawing_has_that_is_the_same_is_used_without_asking() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);

    // Another drawing has the same "Box".
    let mut other = drawing_with_box(dir.path(), 2.0);
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    assert!(
        !other.cmd_prompt().text.contains("already has"),
        "asked about {:?}",
        other.cmd_prompt().text
    );
    submit_all(&mut other, &["0,0"]);
    assert!(!other.command_running());
    assert_eq!(block_names(&other), ["Box"], "the drawing's own is used");
    assert_eq!(other.session().document().len(), 2);
}

#[test]
fn a_different_block_is_still_asked_about_and_the_answer_can_be_kept_for_the_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);

    let mut other = drawing_with_box(dir.path(), 9.0);
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    assert!(other.cmd_prompt().text.contains("already has"));
    // New, kept for the session.
    submit_all(&mut other, &["", "Yes", "0,0"]);
    assert!(!other.command_running());
    assert_eq!(block_names(&other), ["Box", "Box_2"]);
    assert!(said(&other, "kept until OpenDraft is closed"));

    // The next one is not asked about.
    other.start_command("LIBINSERT", library_insert);
    other.settle_command();
    submit_all(&mut other, &["Box"]);
    assert!(
        !other.cmd_prompt().text.contains("already has"),
        "asked again: {:?}",
        other.cmd_prompt().text
    );
    submit_all(&mut other, &["5,5"]);
    assert!(!other.command_running());
    assert_eq!(other.session().document().validate(), Ok(()));
}

#[test]
fn a_block_of_the_drawing_is_inserted_by_name_without_asking_for_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = drawing_with_box(dir.path(), 2.0);
    let before = state.session().document().len();
    state.insert_drawing_block("Box");
    state.settle_command();
    assert!(state.command_running());
    assert!(
        state.cmd_prompt().text.contains("insertion point"),
        "{:?}",
        state.cmd_prompt().text
    );
    submit_all(&mut state, &["30,30"]);
    assert!(!state.command_running());
    assert_eq!(state.session().document().len(), before + 1);
    assert_eq!(block_names(&state), ["Box"]);
    // Nothing went to the library.
    assert!(library_names(&state).is_empty());
}

#[test]
fn inserting_a_block_the_drawing_has_lost_says_so() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut state = drawing_with_box(dir.path(), 2.0);
    state.insert_drawing_block("Gone");
    state.settle_command();
    assert!(!state.command_running());
    assert!(said(&state, "\"Gone\" not found"));
}
