use super::*;
use crate::app::command_line::LOG_CLEARANCE;
use crate::app::command_line_strip::StripRows;
use crate::dock::{LayoutRequest, PanelTab, Strip};

/// An app with the command line dropped along the bottom of the window, and
/// the panel's place once laid out.
fn strip_app() -> (OpenDraftApp, egui::Context, Rect, f64) {
    let (mut app, ctx) = (app(), egui::Context::default());
    toggle(&mut app);
    app.dock.main_surface_mut().split_below(
        egui_dock::NodeIndex::root(),
        0.74,
        vec![PanelTab::CommandLine],
    );
    for line in 0..20 {
        app.state.cmdline.say(format!("history {line}"));
    }
    let mut time = 0.0;
    for _ in 0..3 {
        frame(&mut app, &ctx, Vec::new(), time);
        time += 0.01;
    }
    let panel = panel_rect(&app, PanelTab::CommandLine);
    (app, ctx, panel, time)
}

/// As Ctrl+9 does it.
fn toggle(app: &mut OpenDraftApp) {
    let request = LayoutRequest::Toggle(PanelTab::CommandLine);
    dock::apply(&mut app.dock, &mut app.places, request);
}

fn panel_rect(app: &OpenDraftApp, panel: PanelTab) -> Rect {
    let (_, node, _) = app.dock.find_tab(&panel).unwrap();
    app.dock.main_surface()[node].rect().unwrap()
}

fn rows(app: &OpenDraftApp, ctx: &egui::Context) -> StripRows {
    StripRows::measure(ctx, &app.theme, Some(Strip::Below))
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Press at `from`, move to `to` in steps and let go, a frame for each.
fn drag(app: &mut OpenDraftApp, ctx: &egui::Context, from: Pos2, to: Pos2, time: &mut f64) {
    let mut step = |events| {
        *time += 0.05;
        frame(app, ctx, events, *time);
    };
    step(vec![Event::PointerMoved(from)]);
    step(vec![button(from, true)]);
    for i in 1..=8 {
        step(vec![Event::PointerMoved(from.lerp(to, i as f32 / 8.0))]);
    }
    step(vec![button(to, false)]);
    step(Vec::new());
}

/// Drag the separator above the strip as far down as it goes.
fn shrink(app: &mut OpenDraftApp, ctx: &egui::Context, time: &mut f64) -> Rect {
    let panel = panel_rect(app, PanelTab::CommandLine);
    let separator = Pos2::new(panel.center().x, panel.top());
    let down = Pos2::new(separator.x, panel.bottom() - 2.0);
    drag(app, ctx, separator, down, time);
    panel_rect(app, PanelTab::CommandLine)
}

#[test]
fn the_cross_on_the_grip_closes_the_command_line() {
    let (mut app, ctx, panel, mut time) = strip_app();
    let cross = panel.left_top() + Vec2::splat(9.0);
    drag(&mut app, &ctx, cross, cross, &mut time);
    assert_eq!(
        app.state.layout_request,
        Some(LayoutRequest::Toggle(PanelTab::CommandLine))
    );
}

#[test]
fn dragging_the_grip_undocks_the_command_line() {
    let (mut app, ctx, panel, mut time) = strip_app();
    let handle = Pos2::new(panel.left() + 9.0, panel.top() + 40.0);
    drag(
        &mut app,
        &ctx,
        handle,
        handle + Vec2::new(60.0, -200.0),
        &mut time,
    );
    assert_eq!(
        app.state.layout_request,
        Some(LayoutRequest::Float(PanelTab::CommandLine))
    );
}

#[test]
fn the_tab_hidden_under_the_strip_cannot_be_clicked_or_dragged() {
    let (mut app, ctx, panel, mut time) = strip_app();
    // Along the hidden tab bar: its tab, the tab's close button and beyond.
    for x in (25..260).step_by(6) {
        let at = Pos2::new(panel.left() + x as f32, panel.top() + 12.0);
        drag(&mut app, &ctx, at, at, &mut time);
    }
    let tab = Pos2::new(panel.left() + 50.0, panel.top() + 12.0);
    drag(
        &mut app,
        &ctx,
        tab,
        tab + Vec2::new(200.0, -300.0),
        &mut time,
    );
    assert_eq!(app.state.layout_request, None);
    assert_eq!(dock::command_line_strip(&app.dock), Some(Strip::Below));
    assert_eq!(panel_rect(&app, PanelTab::CommandLine), panel);
}

#[test]
fn a_strip_dropped_along_the_window_stops_at_the_side_panels() {
    let (app, _ctx, panel, _) = strip_app();
    let (canvas, layers, properties) = (
        panel_rect(&app, PanelTab::Canvas),
        panel_rect(&app, PanelTab::Layers),
        panel_rect(&app, PanelTab::Properties),
    );
    // Under the drawing and as wide as it.
    assert_eq!(
        (panel.left(), panel.right()),
        (canvas.left(), canvas.right())
    );
    assert!(panel.top() >= canvas.bottom());
    // The panels beside the drawing run down to the bottom of the window.
    assert!(layers.left() >= panel.right());
    assert!(properties.right() <= panel.left());
    assert_eq!(layers.bottom(), panel.bottom());
    assert_eq!(properties.bottom(), panel.bottom());
}

#[test]
fn the_separator_shrinks_the_strip_to_three_lines_and_no_further() {
    let (mut app, ctx, panel, mut time) = strip_app();
    let least = rows(&app, &ctx).height(2);
    assert!(panel.height() > least + 50.0);
    let height = shrink(&mut app, &ctx, &mut time).height();
    assert!((height - least).abs() < 1.5, "{height} for {least}");
    // The other way, the drawing above keeps the dock's usual least height.
    let panel = panel_rect(&app, PanelTab::CommandLine);
    let separator = Pos2::new(panel.center().x, panel.top());
    let up = Pos2::new(separator.x, 5.0);
    drag(&mut app, &ctx, separator, up, &mut time);
    let top = panel_rect(&app, PanelTab::CommandLine).top();
    assert!(top > 170.0, "{top}");
}

#[test]
fn the_input_row_shows_whole_at_every_height_of_the_strip() {
    let (mut app, ctx, _, mut time) = strip_app();
    let rows = rows(&app, &ctx);
    let input = egui::Id::new("command_line_input");
    let check = |app: &OpenDraftApp, what: &str| {
        let panel = panel_rect(app, PanelTab::CommandLine);
        let row = ctx.read_response(input).expect("the input is drawn").rect;
        // Whole inside the panel, above the Model and layout tabs.
        assert!(row.top() >= panel.top(), "{what}: {row:?} in {panel:?}");
        assert!(
            row.bottom() <= panel.bottom() - rows.tabs,
            "{what}: {row:?} in {panel:?}"
        );
        assert!(row.height() >= 12.0, "{what}: {row:?}");
        panel.bottom() - rows.tabs - row.bottom()
    };
    let under = check(&app, "as docked");
    // At the least height: the same place from the bottom, and room above
    // the row for exactly two lines of history.
    let panel = shrink(&mut app, &ctx, &mut time);
    assert_eq!(check(&app, "at the least height"), under);
    let row = ctx.read_response(input).unwrap().rect;
    let above = row.center().y - rows.input / 2.0 - panel.top();
    let two_lines = 4.0 + 2.0 * rows.log + rows.gap + LOG_CLEARANCE;
    assert!((above - two_lines).abs() < 1.5, "{above} for {two_lines}");
    // Collapsed by the chevron: the row alone.
    app.state.cmdline.resize = Some(true);
    for _ in 0..3 {
        time += 0.05;
        frame(&mut app, &ctx, Vec::new(), time);
    }
    let panel = panel_rect(&app, PanelTab::CommandLine);
    assert!((panel.height() - rows.height(0)).abs() < 1.5);
    assert_eq!(check(&app, "collapsed"), under);
}

#[test]
fn the_model_tab_is_under_the_command_line_docked_below_the_drawing() {
    let (mut app, ctx, panel, mut time) = strip_app();
    let rows = rows(&app, &ctx);
    assert!(rows.tabs > 0.0);
    let before = app.state.docs[app.state.active]
        .session
        .document()
        .layouts()
        .len();
    // The "+" after the tabs, in the band under the command line.
    let band = panel.bottom() - rows.tabs / 2.0;
    let mut added = false;
    for x in (20..200).step_by(4) {
        let at = Pos2::new(panel.left() + x as f32, band);
        drag(&mut app, &ctx, at, at, &mut time);
        let doc = &app.state.docs[app.state.active];
        if doc.session.document().layouts().len() > before {
            added = true;
            break;
        }
    }
    assert!(added, "no layout tab bar under the command line");
}

#[test]
fn a_strip_closed_and_shown_again_is_a_strip_in_the_same_place() {
    let (mut app, ctx, panel, mut time) = strip_app();
    toggle(&mut app);
    for _ in 0..3 {
        time += 0.05;
        frame(&mut app, &ctx, Vec::new(), time);
    }
    assert!(!dock::is_open(&app.dock, PanelTab::CommandLine));
    toggle(&mut app);
    for _ in 0..3 {
        time += 0.05;
        frame(&mut app, &ctx, Vec::new(), time);
    }
    assert_eq!(dock::command_line_strip(&app.dock), Some(Strip::Below));
    let back = panel_rect(&app, PanelTab::CommandLine);
    assert!(
        (back.top() - panel.top()).abs() < 1.5,
        "{back:?} for {panel:?}"
    );
    assert_eq!((back.left(), back.right()), (panel.left(), panel.right()));
}

#[test]
fn a_command_line_undocked_by_its_grip_is_remembered_as_floating() {
    let (mut app, ctx, panel, mut time) = strip_app();
    // Floating inside the main window is the setting's other state.
    app.state.settings.popout_windows = false;
    let handle = Pos2::new(panel.left() + 9.0, panel.top() + 40.0);
    drag(
        &mut app,
        &ctx,
        handle,
        handle + Vec2::new(60.0, -200.0),
        &mut time,
    );
    let request = app.state.layout_request.take().unwrap();
    dock::apply(&mut app.dock, &mut app.places, request);
    let mut frames = |app: &mut OpenDraftApp| {
        for _ in 0..3 {
            time += 0.05;
            frame(app, &ctx, Vec::new(), time);
        }
    };
    frames(&mut app);
    let window = |app: &OpenDraftApp| {
        let (surface, ..) = app.dock.find_tab(&PanelTab::CommandLine).unwrap();
        assert_ne!(surface, egui_dock::SurfaceIndex::main());
        let id = egui::Id::from(format!("window {surface:?}"));
        ctx.memory(|m| m.area_rect(id)).unwrap()
    };
    let floating = window(&app);
    toggle(&mut app);
    frames(&mut app);
    toggle(&mut app);
    frames(&mut app);
    let back = window(&app);
    assert_eq!(back.min, floating.min);
    assert!((back.size() - floating.size()).abs().max_elem() < 1.0);
}
