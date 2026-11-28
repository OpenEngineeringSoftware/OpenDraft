use super::*;
use crate::dock::{LayoutRequest, PanelTab};

fn frames(app: &mut OpenDraftApp, ctx: &egui::Context, time: &mut f64) {
    for _ in 0..4 {
        frame(app, ctx, Vec::new(), *time);
        *time += 0.02;
    }
}

/// The place on screen of the floating window `panel` is in.
fn window_of(app: &OpenDraftApp, ctx: &egui::Context, panel: PanelTab) -> Rect {
    let (surface, ..) = app.dock.find_tab(&panel).unwrap();
    assert_ne!(surface, egui_dock::SurfaceIndex::main());
    let id = egui::Id::from(format!("window {surface:?}"));
    ctx.memory(|m| m.area_rect(id)).unwrap()
}

fn toggle(app: &mut OpenDraftApp, panel: PanelTab) {
    dock::apply(&mut app.dock, &mut app.places, LayoutRequest::Toggle(panel));
}

#[test]
fn a_floating_panel_reopens_where_it_floated_at_its_size() {
    let (mut app, ctx, mut time) = (app(), egui::Context::default(), 0.0);
    // Floating inside the main window is the setting's other state.
    app.state.settings.popout_windows = false;
    frames(&mut app, &ctx, &mut time);
    let found = app.dock.find_tab(&PanelTab::Layers).unwrap();
    let asked = Rect::from_min_size(Pos2::new(420.0, 180.0), Vec2::new(300.0, 380.0));
    app.dock.detach_tab(found, asked);
    frames(&mut app, &ctx, &mut time);
    let before = window_of(&app, &ctx, PanelTab::Layers);
    assert_eq!(before.min, asked.min);

    toggle(&mut app, PanelTab::Layers);
    frames(&mut app, &ctx, &mut time);
    assert!(!dock::is_open(&app.dock, PanelTab::Layers));
    // Another window takes its slot in the dock meanwhile, and with it
    // what egui remembers of the window that was there.
    let found = app.dock.find_tab(&PanelTab::Properties).unwrap();
    let elsewhere = Rect::from_min_size(Pos2::new(40.0, 500.0), Vec2::new(500.0, 200.0));
    app.dock.detach_tab(found, elsewhere);
    frames(&mut app, &ctx, &mut time);
    toggle(&mut app, PanelTab::Layers);
    frames(&mut app, &ctx, &mut time);

    let after = window_of(&app, &ctx, PanelTab::Layers);
    assert_eq!(after.min, before.min);
    assert!(
        (after.size() - before.size()).abs().max_elem() < 1.0,
        "{after:?} for {before:?}"
    );
}

#[test]
fn a_panel_closed_by_its_tab_is_remembered_too() {
    // Nothing tells the app that the dock closed a tab, so the places are
    // noted on every frame the panel is shown.
    let (mut app, ctx, mut time) = (app(), egui::Context::default(), 0.0);
    toggle(&mut app, PanelTab::Properties);
    let tabs = vec![PanelTab::Properties];
    app.dock
        .main_surface_mut()
        .split_right(egui_dock::NodeIndex::root(), 0.85, tabs);
    frames(&mut app, &ctx, &mut time);
    let before = app.dock.find_tab(&PanelTab::Properties).unwrap();
    // As the tab's close button does it, behind the app's back.
    app.dock.remove_tab(before);
    frames(&mut app, &ctx, &mut time);
    toggle(&mut app, PanelTab::Properties);
    assert_eq!(app.dock.find_tab(&PanelTab::Properties), Some(before));
}
