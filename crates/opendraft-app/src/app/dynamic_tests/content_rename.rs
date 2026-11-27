//! Renaming a block in the Content Manager's details, with the keys going
//! through the whole app as typed.

use super::*;

/// A frame of the app with its windows, as `update` draws them.
fn frame_with_windows(app: &mut OpenDraftApp, ctx: &egui::Context, events: Vec<Event>, t: f64) {
    if t == 0.0 {
        app.theme.apply(ctx);
    }
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0))),
        events,
        time: Some(t),
        ..Default::default()
    };
    app.capture_dynamic_keys(ctx, &mut raw);
    let _ = ctx.run(raw, |ctx| {
        app.state.pump_command();
        app.route_typing_to_command_line(ctx);
        app.handle_shortcuts(ctx);
        app.panels(ctx);
        app.tool_windows(ctx);
    });
}

/// An app on a drawing with block "Box", renaming it in the details, with
/// the app's own shortcuts (Escape cancels in the drawing) and something to
/// cancel: the selection.
fn renaming(dir: &std::path::Path) -> (OpenDraftApp, egui::Context) {
    let mut app = app();
    app.shortcuts = app
        .registry
        .iter()
        .filter_map(|c| {
            let shortcut = opendraft_ui::shortcuts::parse_shortcut(c.shortcut?)?;
            Some((shortcut, c.id))
        })
        .collect();
    app.state = crate::library_cmd_tests::drawing_with_box(dir, 5.0);
    app.state.select_all();
    assert!(app.state.can_cancel(), "Escape would cancel in the drawing");
    let id = app
        .state
        .session()
        .document()
        .block_named("Box")
        .map(|b| b.id)
        .expect("the block");
    app.state
        .command_dialogs
        .content
        .start_drawing_rename(id, "Box");
    let ctx = egui::Context::default();
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.0);
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.02);
    (app, ctx)
}

fn block_names(app: &OpenDraftApp) -> Vec<String> {
    let doc = app.state.session().document();
    doc.blocks().iter().map(|b| b.name.clone()).collect()
}

#[test]
fn enter_keeps_the_name_typed_in_the_details() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut app, ctx) = renaming(dir.path());
    frame_with_windows(&mut app, &ctx, vec![Event::Text("2".into())], 0.04);
    assert_eq!(app.state.cmdline.input, "", "the field has the typing");
    frame_with_windows(&mut app, &ctx, key(Key::Enter), 0.06);
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.08);
    assert_eq!(block_names(&app), ["Box2"]);
}

#[test]
fn escape_leaves_the_name_as_it_was() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut app, ctx) = renaming(dir.path());
    frame_with_windows(&mut app, &ctx, vec![Event::Text("2".into())], 0.04);
    frame_with_windows(&mut app, &ctx, key(Key::Escape), 0.06);
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.08);
    assert!(!app.state.command_dialogs.content.renaming());
    assert_eq!(block_names(&app), ["Box"]);
}

#[test]
fn escape_closes_the_list_of_units_without_cancelling_in_the_drawing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (mut app, ctx) = renaming(dir.path());
    let window = &mut app.state.command_dialogs.content;
    window.show_drawing_block("Box");
    let list = egui::Id::new("content_manager_unit_menu");
    ctx.memory_mut(|m| m.open_popup(list));
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.04);
    assert!(
        app.state.command_dialogs.content.typing_a_name(),
        "the list has the keys"
    );
    let selected = app.state.docs[0].selection.len();
    assert!(selected > 0);
    frame_with_windows(&mut app, &ctx, key(Key::Escape), 0.06);
    frame_with_windows(&mut app, &ctx, Vec::new(), 0.08);
    assert!(
        !ctx.memory(|m| m.is_popup_open(list)),
        "Escape closed the list"
    );
    assert_eq!(
        app.state.docs[0].selection.len(),
        selected,
        "nothing cancelled"
    );
    assert!(!app.state.command_dialogs.content.typing_a_name());
}
