use eframe::egui;
use opendraft_content::blocks_payload::BlockUuid;
use opendraft_ui::Theme;

use super::actions::{Action, Taken};
use super::*;
use crate::library_cmd_tests::{app_with_library, drawing_with_box, stocked};
use crate::state::AppState;

pub(super) fn theme() -> Theme {
    Theme::default()
}

/// Runs the window for one frame, as the app would.
pub(super) fn frame(win: &mut Window, state: &AppState) -> Vec<Request> {
    let ctx = egui::Context::default();
    let theme = theme();
    theme.apply(&ctx);
    let mut requests = Vec::new();
    let defaults = std::sync::Arc::clone(state.defaults());
    let session = state.session();
    let env = Env {
        library: &defaults.library,
        document: session.document(),
        revision: session.revision(),
    };
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        requests = win.show(ctx, &theme, &env);
    });
    requests
}

/// A window on a drawing that has block "Box", with "Box" and "Other" in the
/// library.
pub(super) fn setup(dir: &std::path::Path) -> (AppState, Window) {
    stocked(dir, 2.0);
    let state = drawing_with_box(dir, 5.0);
    let mut win = Window::default();
    win.open_on(Place::default());
    (state, win)
}

pub(super) fn names(state: &AppState) -> Vec<String> {
    state
        .defaults()
        .library
        .with(|l| l.blocks().into_iter().map(|b| b.name).collect())
        .expect("opens")
}

pub(super) fn apply(win: &mut Window, state: &AppState, actions: Vec<Action>) -> Vec<Request> {
    let defaults = std::sync::Arc::clone(state.defaults());
    let session = state.session();
    let env = Env {
        library: &defaults.library,
        document: session.document(),
        revision: session.revision(),
    };
    win.apply(actions, &env)
}

#[test]
fn every_place_and_view_draws_without_trouble() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    let id = state
        .defaults()
        .library
        .with(|l| l.blocks()[0].id)
        .expect("a block");
    for place in [
        Place::default(),
        Place::Drawing,
        Place::Favourites,
        Place::Recent,
        Place::Trash,
    ] {
        for view in [View::Grid, View::List] {
            for selected in [
                Selected::Nothing,
                Selected::Block(id),
                Selected::Drawing("Box".into()),
                Selected::Trashed(7),
            ] {
                win.place = place.clone();
                win.view = view;
                win.selected = selected;
                // A few frames, so thumbnails get made.
                for _ in 0..3 {
                    assert!(frame(&mut win, &state).is_empty());
                }
            }
        }
    }
    win.search = "box".into();
    assert!(frame(&mut win, &state).is_empty());
}

#[test]
fn a_closed_window_draws_nothing_and_asks_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    win.open = false;
    assert!(frame(&mut win, &state).is_empty());
}

#[test]
fn a_library_that_cannot_open_says_so_instead_of_failing() {
    let state = AppState::new();
    let mut win = Window::default();
    win.open_on(Place::default());
    assert!(frame(&mut win, &state).is_empty());
    let defaults = std::sync::Arc::clone(state.defaults());
    let env = Env {
        library: &defaults.library,
        document: state.session().document(),
        revision: 0,
    };
    let snapshot = snapshot::Snapshot::take(&win, &env);
    assert!(snapshot.problem.is_some());
}

#[test]
fn navigation_goes_back_forward_and_up() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    apply(&mut win, &state, vec![Action::NewFolder("A/B".into())]);
    apply(
        &mut win,
        &state,
        vec![Action::Go(Place::Mine("A/B".into()))],
    );
    apply(&mut win, &state, vec![Action::Go(Place::Trash)]);
    assert_eq!(win.place, Place::Trash);
    apply(&mut win, &state, vec![Action::Back]);
    assert_eq!(win.place, Place::Mine("A/B".into()));
    apply(&mut win, &state, vec![Action::Up]);
    assert_eq!(win.place, Place::Mine("A".into()));
    apply(&mut win, &state, vec![Action::Forward]);
    assert_eq!(
        win.place,
        Place::Mine("A".into()),
        "going Up cleared Forward"
    );
    apply(
        &mut win,
        &state,
        vec![Action::Back, Action::Back, Action::Back],
    );
    assert_eq!(win.place, Place::default());
}

#[test]
fn insert_and_add_are_left_to_the_app() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    let id = BlockUuid::new();
    let requests = apply(
        &mut win,
        &state,
        vec![Action::Insert(id), Action::AddObjects],
    );
    assert_eq!(requests, [Request::Insert(id), Request::AddObjects]);
}

#[test]
fn blocks_are_renamed_moved_favourited_and_deleted_and_restored() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    let id = state
        .defaults()
        .library
        .with(|l| l.blocks()[0].id)
        .expect("a block");

    apply(
        &mut win,
        &state,
        vec![Action::RenameBlock(id, "Crate".into())],
    );
    assert_eq!(names(&state), ["Crate"]);
    apply(
        &mut win,
        &state,
        vec![
            Action::NewFolder("Boxes".into()),
            Action::Move(id, "Boxes".into()),
        ],
    );
    let folder = state
        .defaults()
        .library
        .with(|l| l.block(id).map(|b| b.folder))
        .expect("opens");
    assert_eq!(folder, Some("Boxes".into()));
    apply(&mut win, &state, vec![Action::Favourite(id, true)]);
    assert_eq!(
        state
            .defaults()
            .library
            .with(|l| l.favourites().len())
            .expect("opens"),
        1
    );

    apply(&mut win, &state, vec![Action::Delete(id)]);
    assert!(names(&state).is_empty());
    assert!(win.status.contains("Trash"), "{}", win.status);
    let item = state
        .defaults()
        .library
        .with(|l| l.trash_items()[0].id)
        .expect("an item");
    apply(&mut win, &state, vec![Action::Restore(item)]);
    assert_eq!(names(&state), ["Crate"]);
    assert_eq!(win.selected, Selected::Block(id));
}

#[test]
fn a_failure_is_said_in_the_status_and_changes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    let id = state
        .defaults()
        .library
        .with(|l| l.blocks()[0].id)
        .expect("a block");
    apply(
        &mut win,
        &state,
        vec![Action::RenameBlock(id, "a/b".into())],
    );
    assert!(!win.status.is_empty());
    assert_eq!(names(&state), ["Box"]);
}

#[test]
fn a_folder_renamed_while_it_is_shown_keeps_being_shown() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    apply(
        &mut win,
        &state,
        vec![
            Action::NewFolder("Old".into()),
            Action::Go(Place::Mine("Old".into())),
        ],
    );
    apply(
        &mut win,
        &state,
        vec![Action::RenameFolder("Old".into(), "New".into())],
    );
    assert_eq!(win.place, Place::Mine("New".into()));
    apply(&mut win, &state, vec![Action::DeleteFolder("New".into())]);
    assert_eq!(win.place, Place::default());
}

#[test]
fn a_block_of_the_drawing_is_added_and_a_taken_name_offers_a_choice() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    // The library has "Box" already.
    apply(
        &mut win,
        &state,
        vec![Action::AddFromDrawing("Box".into(), Taken::Cancel)],
    );
    assert_eq!(win.taken.as_deref(), Some("Box"));
    assert_eq!(names(&state), ["Box"]);

    apply(
        &mut win,
        &state,
        vec![Action::AddFromDrawing("Box".into(), Taken::KeepBoth)],
    );
    assert!(win.taken.is_none());
    assert_eq!(names(&state), ["Box", "Box_2"]);

    apply(
        &mut win,
        &state,
        vec![
            Action::AddFromDrawing("Box".into(), Taken::Cancel),
            Action::DropOffer,
        ],
    );
    assert!(win.taken.is_none());
    apply(
        &mut win,
        &state,
        vec![Action::AddFromDrawing("Nothing".into(), Taken::Cancel)],
    );
    assert!(!win.status.is_empty());
}

#[test]
fn the_window_inserts_through_the_command_the_app_starts() {
    let dir = tempfile::tempdir().expect("temp dir");
    stocked(dir.path(), 2.0);
    let mut other = app_with_library(dir.path());
    let id = other
        .defaults()
        .library
        .with(|l| l.blocks()[0].id)
        .expect("a block");
    other.defaults().library.queue_pick(id);
    other.start_command("LIBINSERT", crate::library_insert_cmd::library_insert);
    other.settle_command();
    // No name was asked for: the point is.
    other.cmd_submit("5,5");
    other.settle_command();
    assert!(!other.command_running());
    assert_eq!(other.session().document().len(), 1);
}

#[test]
fn a_block_of_the_drawing_is_renamed_from_its_menu_and_stays_chosen() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut state, mut win) = setup(dir.path());
    win.open_on(Place::Drawing);
    let id = state
        .session()
        .document()
        .block_named("Box")
        .map(|b| b.id)
        .expect("the drawing's block");

    // Rename on the menu chooses the block and opens the name bar on its name.
    apply(&mut win, &state, drawing::rename(id, "Box").to_vec());
    assert_eq!(win.selected, Selected::Drawing("Box".into()));
    assert!(
        matches!(&win.editing, Some(Editing::RenameDrawingBlock { id: i, text }) if *i == id && text == "Box")
    );

    // A name a block cannot hold keeps the bar open and says why.
    let bad = apply(
        &mut win,
        &state,
        vec![Action::RenameDrawingBlock(id, "a/b".into())],
    );
    assert_eq!(bad, [Request::RenameDrawingBlock(id, "a/b".into())]);
    let refused = state.rename_block(id, "a/b");
    win.drawing_block_renamed(refused, "a/b", (0, 0));
    assert!(win.editing.is_some() && win.status.contains("cannot contain"));
    assert!(state.session().document().block_named("Box").is_some());

    // OK asks the app, which renames it; the window follows the new name.
    let asked = apply(
        &mut win,
        &state,
        vec![Action::RenameDrawingBlock(id, "Crate".into())],
    );
    let [Request::RenameDrawingBlock(asked_id, name)] = asked.as_slice() else {
        panic!("one request: {asked:?}")
    };
    let before = state.session().revision();
    let result = state.rename_block(*asked_id, name);
    let after = state.session().revision();
    win.drawing_block_renamed(result, name, (before, after));
    assert!(win.editing.is_none());
    assert_eq!(win.selected, Selected::Drawing("Crate".into()));
    assert_eq!(win.status, "Block \"Box\" renamed to \"Crate\".");
    assert!(state.session().document().block_named("Crate").is_some());
    // The window still draws, the details of the renamed block included.
    let _ = frame(&mut win, &state);
}

#[test]
fn a_block_is_renamed_in_its_details_and_choosing_another_closes_the_field() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (state, mut win) = setup(dir.path());
    win.open_on(Place::Drawing);
    let id = state
        .session()
        .document()
        .block_named("Box")
        .map(|b| b.id)
        .expect("the drawing's block");
    apply(&mut win, &state, drawing::rename(id, "Box").to_vec());
    // The field is drawn in the details, where the name is, not above the tiles.
    let _ = frame(&mut win, &state);
    assert!(matches!(
        win.editing,
        Some(Editing::RenameDrawingBlock { .. })
    ));
    apply(&mut win, &state, vec![Action::Select(Selected::Nothing)]);
    assert!(win.editing.is_none());
}
