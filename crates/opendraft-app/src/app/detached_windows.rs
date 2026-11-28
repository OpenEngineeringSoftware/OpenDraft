//! Panels in operating system windows of their own (see `detached.rs`).

use eframe::egui;
use egui_dock::DockState;

use super::detached_drag;
use super::panels::PanelViewer;
use super::OpenDraftApp;
use crate::detached::{Detached, DEFAULT_SIZE, LEAST, OPENS_AT_LEAST};
use crate::dock::{self, LayoutRequest, PanelTab};

/// What the windows asked for this frame.
#[derive(Default)]
struct Asked {
    /// The Dock button was pressed.
    pub dock: Vec<PanelTab>,
    /// The window's own close button was pressed.
    pub close: Vec<PanelTab>,
    /// A window's drag handle is being carried, or was let go.
    pub moved: Option<(PanelTab, detached_drag::Moved)>,
    /// Keys no widget of a window used (see [`unused_keys`]).
    pub keys: Vec<egui::Event>,
}

impl OpenDraftApp {
    /// Whether the panel is shown, docked or in a window of its own.
    pub(super) fn panel_open(&self, panel: PanelTab) -> bool {
        dock::is_open(&self.dock, panel) || self.detached.contains(panel)
    }

    /// The layout change a command asked for. One about a panel that is in a
    /// window of its own is for that window, not for the dock.
    pub(super) fn apply_layout_request(&mut self, ctx: &egui::Context) {
        let Some(request) = self.state.layout_request.take() else {
            return;
        };
        let handled = match request {
            LayoutRequest::Reset => {
                self.detached.take_all();
                false
            }
            LayoutRequest::Toggle(panel) if self.detached.contains(panel) => {
                self.detached.remove(panel);
                true
            }
            LayoutRequest::BringBack => {
                self.detached.ask_bring_back();
                opendraft_ui::widgets::bring_dialogs_back(ctx);
                true
            }
            LayoutRequest::Toggle(_) | LayoutRequest::Float(_) => false,
        };
        if !handled {
            dock::apply(&mut self.dock, &mut self.places, request);
        }
    }
}

/// With the setting on, a panel that floats in the main window becomes a
/// window of its own; with it off, windows go back into the dock.
fn settle(
    ctx: &egui::Context,
    on: bool,
    dock: &mut DockState<PanelTab>,
    places: &mut dock::Places,
    detached: &mut Detached,
) {
    if !on {
        for panel in detached.take_all() {
            dock::dock_back(dock, places, panel);
        }
        return;
    }
    let ppp = ctx.pixels_per_point();
    let origin = ctx
        .input(|i| i.viewport().inner_rect.or(i.viewport().outer_rect))
        .map(|r| r.min.to_vec2());
    for (panel, rect) in dock::floating(dock, ctx) {
        let big = |r: egui::Rect| {
            [
                r.width().max(OPENS_AT_LEAST[0]),
                r.height().max(OPENS_AT_LEAST[1]),
            ]
        };
        let (at, size) = match (rect, origin) {
            (Some(r), Some(o)) => {
                // Points of this window, to the pixels of the whole screen.
                let p = (r.min.to_vec2() + o) * ppp;
                (Some([p.x, p.y]), big(r))
            }
            (Some(r), None) => (None, big(r)),
            _ => (None, DEFAULT_SIZE),
        };
        if let Some(found) = dock.find_tab(&panel) {
            dock.remove_tab(found);
            detached.add(panel, at, size);
        }
    }
}

/// Show each detached panel in its window. The panel draws itself as it
/// does in the dock; a header above it holds its title and the Dock button.
fn show(
    ctx: &egui::Context,
    fill: egui::Color32,
    detached: &mut Detached,
    viewer: &mut PanelViewer<'_>,
) -> Asked {
    let mut asked = Asked::default();
    for w in detached.windows().to_vec() {
        let panel = w.panel;
        let (at, size) = detached.start(&w);
        let theme = viewer.theme;
        let title = opendraft_ui::i18n::tr(panel.title()).to_string();
        let builder = egui::ViewportBuilder::default()
            .with_title(title.as_str())
            .with_inner_size(size)
            .with_min_inner_size(LEAST)
            .with_decorations(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        // The position is not in the builder: egui would turn it into pixels
        // with the primary monitor's scale. It is applied from inside the
        // window (`placement`), once its own scale is known.
        let id = egui::ViewportId::from_hash_of(("panel_window", panel));
        let placed_key = egui::Id::new(("panel_window_placed", panel));
        let pass = ctx.cumulative_pass_nr();
        let seen_key = egui::Id::new(("panel_window_seen", panel));
        let fresh = !matches!(ctx.data(|d| d.get_temp::<u64>(seen_key)), Some(s) if s + 1 >= pass);
        ctx.data_mut(|d| d.insert_temp(seen_key, pass));
        if fresh {
            opendraft_ui::widgets::forget_placement(ctx, placed_key);
        }
        let mut seen = None;
        let mut tab = panel;
        ctx.show_viewport_immediate(id, builder, |ctx, _class| {
            opendraft_ui::widgets::keep_above_main(ctx, egui::Id::new(("panel_window_top", panel)));
            // Typing with nothing focused here goes to the command line, as
            // in the main window; where that is this window, focus it first.
            let had_focus = ctx.wants_keyboard_input();
            if panel == PanelTab::CommandLine && !had_focus && typed_text(ctx) {
                viewer.state.cmdline.focus = true;
            }
            let header = detached_drag::header(ctx, theme, panel, &title);
            asked.dock.extend(header.dock.then_some(panel));
            asked.close.extend(header.close.then_some(panel));
            super::detached_frame::resize_zones(ctx, panel);
            super::detached_frame::border(ctx, theme);
            asked.moved = header.moved.map(|m| (panel, m));
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(fill))
                .show(ctx, |ui| {
                    egui_dock::TabViewer::ui(viewer, ui, &mut tab);
                });
            if !had_focus {
                asked.keys.extend(unused_keys(ctx, panel));
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                asked.close.push(panel);
            }
            let placed = match at {
                Some(p) => {
                    opendraft_ui::widgets::settle_placement(ctx, placed_key, egui::pos2(p[0], p[1]))
                }
                None => true,
            };
            let ppp = ctx.pixels_per_point();
            let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
            // Not before it is where it was meant to be, and not while it is
            // shrunk to be carried.
            if let (true, Some(o), Some(i)) = (placed, outer, inner.filter(|i| i.height() > 60.0)) {
                let at = o.min.to_vec2() * ppp;
                seen = Some(([at.x, at.y], [i.width(), i.height()]));
            }
        });
        if let Some((at, size)) = seen {
            detached.moved(panel, at, size);
        }
    }
    asked
}

fn typed_text(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, egui::Event::Text(t) if !t.trim().is_empty()))
    })
}

/// The key presses, typed text and clipboard keys of a window that no widget
/// of it took, for the main window's shortcuts and command line. Typed text
/// in the command line's own window went to its field, not here.
fn unused_keys(ctx: &egui::Context, panel: PanelTab) -> Vec<egui::Event> {
    ctx.input(|i| {
        i.events
            .iter()
            .filter(|e| match e {
                egui::Event::Key { pressed, .. } => *pressed,
                egui::Event::Text(_) => panel != PanelTab::CommandLine,
                egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_) => true,
                _ => false,
            })
            .cloned()
            .collect()
    })
}

/// Settle which panels are windows, show them, and act on their buttons.
pub(super) fn run(
    ctx: &egui::Context,
    fill: egui::Color32,
    on: bool,
    (dock, places): (&mut DockState<PanelTab>, &mut dock::Places),
    detached: &mut Detached,
    viewer: &mut PanelViewer<'_>,
) {
    settle(ctx, on, dock, places, detached);
    let ppp = ctx.pixels_per_point();
    if detached.take_bring_back() {
        // Over the main window, wherever the windows were left.
        if let Some(main) = ctx.input(|i| i.viewport().inner_rect) {
            detached.bring_back(main * ppp, ppp);
        }
    }
    let asked = show(ctx, fill, detached, viewer);
    if let Some((panel, moved)) = asked.moved {
        // The pointer is in pixels of the whole screen, whatever monitor the
        // window it came from is on; so is the main window's inside here.
        let main = ctx.input(|i| i.viewport().inner_rect).map(|r| r * ppp);
        let near_id = detached_drag::near_key(panel);
        let mut near = false;
        if let Some(main) = main.filter(|r| r.contains(moved.pointer)) {
            let at = ((moved.pointer - main.min) / ppp).to_pos2();
            // Wider to leave than to enter, so the window does not flicker.
            let was = ctx.data(|d| d.get_temp::<bool>(near_id)) == Some(true);
            let margin = if was { 110.0 } else { 70.0 };
            near = dock::near_dock_target(dock, panel, at, margin);
            let theme = viewer.theme;
            if dock::offer_detached(ctx, theme, dock, panel, at, moved.released) {
                detached.remove(panel);
            }
        }
        ctx.data_mut(|d| d.insert_temp(near_id, near));
    }
    if !asked.keys.is_empty() {
        detached.forward(asked.keys);
        ctx.request_repaint();
    }
    for panel in asked.close {
        detached.remove(panel);
    }
    for panel in asked.dock {
        detached.remove(panel);
        dock::dock_back(dock, places, panel);
    }
}
