//! Panels in operating system windows of their own (see `detached.rs`).

use eframe::egui;
use egui_dock::DockState;

use super::detached_drag;
use super::panels::PanelViewer;
use super::OpenDraftApp;
use crate::detached::{Detached, DEFAULT_SIZE};
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
    pub(super) fn apply_layout_request(&mut self) {
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
    let origin = ctx
        .input(|i| i.viewport().inner_rect.or(i.viewport().outer_rect))
        .map(|r| r.min.to_vec2());
    for (panel, rect) in dock::floating(dock, ctx) {
        // The dock's window is small (0.8 of the docked size); a window of
        // the OS needs room for its Dock strip and a readable panel.
        let big = |r: egui::Rect| [r.width().max(300.0), r.height().max(260.0)];
        let (at, size) = match (rect, origin) {
            (Some(r), Some(o)) => {
                let p = r.min + o;
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
/// does in the dock; a strip above it holds the Dock button.
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
        let title = opendraft_ui::i18n::tr(panel.title());
        let mut builder = egui::ViewportBuilder::default()
            .with_title(title.to_string())
            .with_inner_size(size)
            .with_min_inner_size(detached_drag::LEAST)
            .with_window_level(egui::WindowLevel::AlwaysOnTop);
        if let Some(p) = at {
            builder = builder.with_position(egui::pos2(p[0], p[1]));
        }
        let id = egui::ViewportId::from_hash_of(("panel_window", panel));
        let mut seen = None;
        let mut tab = panel;
        ctx.show_viewport_immediate(id, builder, |ctx, _class| {
            keep_above_main(ctx, panel);
            // Typing with nothing focused here goes to the command line, as
            // in the main window; where that is this window, focus it first.
            let had_focus = ctx.wants_keyboard_input();
            if panel == PanelTab::CommandLine && !had_focus && typed_text(ctx) {
                viewer.state.cmdline.focus = true;
            }
            egui::TopBottomPanel::top(egui::Id::new(("dock_strip", panel)))
                .frame(
                    egui::Frame::NONE
                        .fill(fill)
                        .inner_margin(egui::Margin::symmetric(6, 3)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(opendraft_ui::i18n::tr("Dock")).clicked() {
                            asked.dock.push(panel);
                        }
                        if let Some(moved) = detached_drag::handle(ui, panel) {
                            asked.moved = Some((panel, moved));
                        }
                    });
                });
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
            let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
            // Not while the window is shrunk to be carried.
            if let (Some(o), Some(i)) = (outer, inner.filter(|i| i.height() > 60.0)) {
                seen = Some(([o.min.x, o.min.y], [i.width(), i.height()]));
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

/// Above the main window while this program is the active one, an ordinary
/// window when another program is (so it does not cover that program).
fn keep_above_main(ctx: &egui::Context, panel: PanelTab) {
    let mine = ctx.input(|i| i.viewport().focused).unwrap_or(false);
    let root = ctx
        .input_for(egui::ViewportId::ROOT, |i| i.viewport().focused)
        .unwrap_or(false);
    let top = mine || root;
    let key = egui::Id::new(("panel_window_top", panel));
    if ctx.data(|d| d.get_temp::<bool>(key)) != Some(top) {
        ctx.data_mut(|d| d.insert_temp(key, top));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
    }
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
    let asked = show(ctx, fill, detached, viewer);
    if let Some((panel, moved)) = asked.moved {
        let main = ctx.input(|i| i.viewport().inner_rect);
        if let Some(main) = main.filter(|r| r.contains(moved.pointer)) {
            let at = moved.pointer - main.min.to_vec2();
            let theme = viewer.theme;
            if dock::offer_detached(ctx, theme, dock, panel, at, moved.released) {
                detached.remove(panel);
            }
        }
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
