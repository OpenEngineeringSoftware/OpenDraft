//! Panels that leave the dock: the ones floating in its own windows, and
//! putting one back in the main area.

use eframe::egui;
use egui_dock::DockState;

use super::{places, restore, PanelTab, Places};

/// The panels in the dock's own floating windows, with each window's inside
/// in the main window's coordinates when egui knows it.
pub fn floating(
    dock: &DockState<PanelTab>,
    ctx: &egui::Context,
) -> Vec<(PanelTab, Option<egui::Rect>)> {
    dock.iter_all_tabs()
        .filter(|((surface, _), tab)| {
            *surface != egui_dock::SurfaceIndex::main() && tab.closeable()
        })
        .map(|((surface, _), tab)| (*tab, places::window_rect(ctx, surface)))
        .collect()
}

/// Show a hidden panel docked in the main area, where it was or where the
/// default layout keeps it, never in a floating window.
pub fn dock_back(dock: &mut DockState<PanelTab>, places: &mut Places, panel: PanelTab) {
    if dock.find_tab(&panel).is_some() {
        return;
    }
    if places.restore(dock, panel) {
        match dock.find_tab(&panel) {
            Some(found) if found.0 != egui_dock::SurfaceIndex::main() => {
                dock.remove_tab(found);
            }
            _ => return,
        }
    }
    restore(dock, panel);
}
