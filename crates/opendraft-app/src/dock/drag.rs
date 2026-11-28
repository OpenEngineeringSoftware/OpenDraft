//! Shows the dock area and steers a panel dragged by its tab: draws the
//! drop targets of [`super::targets`] and a picture of where the panel will
//! go, and puts it there when the tab is let go.

use eframe::egui::{self, Pos2, Rect, Vec2};
use egui_dock::{DockArea, DockState, SurfaceIndex, TabViewer};
use opendraft_ui::Theme;

use super::targets::{self, Drop, Side, Target};
use super::PanelTab;

/// How far the pointer must move before egui_dock treats a pressed tab as
/// dragged (its own thresholds, across and down).
const DRAG_START: Vec2 = Vec2::new(30.0, 6.0);

/// A tab being dragged, as last seen while the button was held.
#[derive(Clone, Copy, Debug)]
struct Dragged {
    panel: PanelTab,
    /// The size of the window the panel would float in.
    size: Vec2,
    pointer: Pos2,
}

fn area_id() -> egui::Id {
    egui::Id::new("opendraft_dock")
}

/// The dock area with the panels in it.
pub fn show_area(
    ui: &mut egui::Ui,
    dock: &mut DockState<PanelTab>,
    style: egui_dock::Style,
    theme: &Theme,
    viewer: &mut impl TabViewer<Tab = PanelTab>,
) {
    let ctx = ui.ctx().clone();
    let memory = area_id().with("dragged");
    let released = ctx.input(|i| i.pointer.primary_released());
    // egui_dock acts on a release with what it saw the frame before; so do we.
    let before: Option<Dragged> = ctx.data(|d| d.get_temp(memory));
    let now = dragged(&ctx, dock);
    ctx.data_mut(|d| match now {
        Some(now) => d.insert_temp(memory, now),
        None => d.remove::<Dragged>(memory),
    });
    let drag = if released { before } else { now };
    // Targets come from the layout as drawn, before the drop changes it.
    let shown = drag.map(|d| {
        let found = targets::targets(dock, d.panel, d.size, d.pointer);
        let drop = targets::drop_at(dock, &found, d.pointer);
        (found, drop)
    });
    ui.scope(|ui| {
        floating_frame(ui.style_mut());
        DockArea::new(dock)
            .id(area_id())
            .style(style)
            .allowed_splits(egui_dock::AllowedSplits::None)
            .show_close_buttons(true)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .show_inside(ui, viewer);
    });
    let (Some(d), Some((found, drop))) = (drag, shown) else {
        return;
    };
    if released {
        targets::finish(dock, d.panel, drop, d.pointer, d.size);
    } else {
        paint(&ctx, theme, &found, drop, d);
    }
}

/// A detached panel's window dragged by its handle over the main window,
/// the pointer `at` in the main window's coordinates: the targets and the
/// place it would take are drawn, and on `released` it is docked where the
/// pointer is. True when it was docked (it is then in `dock`).
pub fn offer_detached(
    ctx: &egui::Context,
    theme: &Theme,
    dock: &mut DockState<PanelTab>,
    panel: PanelTab,
    at: Pos2,
    released: bool,
) -> bool {
    // The panel's own size matters only for how much of the drawing it takes.
    let size = Vec2::from(crate::detached::OPENS_AT_LEAST);
    let found = targets::targets(dock, panel, size, at);
    let drop = targets::drop_at(dock, &found, at);
    if !released {
        let d = Dragged {
            panel,
            size: Vec2::ZERO,
            pointer: at,
        };
        paint(ctx, theme, &found, drop, d);
        return false;
    }
    match drop {
        Drop::Dock(target) => {
            targets::insert(dock, panel, target);
            true
        }
        Drop::TabBar => targets::join_bar(dock, panel, at),
        Drop::Float => false,
    }
}

/// A floating panel's window is framed like a docked panel: square, with
/// its one-point border right round the tab bar and contents, not the
/// padding and rounded corners of a dialog. egui_dock frames its windows
/// from the style of the `Ui` it is shown in.
pub(super) fn floating_frame(style: &mut egui::Style) {
    style.spacing.window_margin = egui::Margin::ZERO;
    style.visuals.window_corner_radius = egui::CornerRadius::ZERO;
}

/// The panel whose tab is being dragged, once it has moved far enough for
/// egui_dock to treat it as a drag. Its tab's id is made as egui_dock makes
/// it, from the area id and where the tab is.
fn dragged(ctx: &egui::Context, dock: &DockState<PanelTab>) -> Option<Dragged> {
    let (pointer, origin) = ctx.input(|i| (i.pointer.latest_pos(), i.pointer.press_origin()));
    let (pointer, origin) = (pointer?, origin?);
    let moved = (pointer - origin).abs();
    if moved.x <= DRAG_START.x && moved.y <= DRAG_START.y {
        return None;
    }
    let tabs: Vec<PanelTab> = dock.iter_all_tabs().map(|(_, tab)| *tab).collect();
    tabs.into_iter().find_map(|panel| {
        let (surface, node, tab) = dock.find_tab(&panel)?;
        let id = area_id()
            .with((surface, "surface"))
            .with((node, "node"))
            .with((tab.0, "tab"));
        if !ctx.is_being_dragged(id) {
            return None;
        }
        let rect = dock[surface][node].rect().unwrap_or(Rect::ZERO);
        // A docked panel's window opens at 0.8 of its docked size.
        let scale = if surface == SurfaceIndex::main() {
            0.8
        } else {
            1.0
        };
        let size = (rect.size() * scale).max(Vec2::splat(120.0));
        Some(Dragged {
            panel,
            size,
            pointer,
        })
    })
}

/// The targets, the place the panel will take, or the outline of the
/// window it will float in.
fn paint(ctx: &egui::Context, theme: &Theme, found: &[Target], drop: Drop, d: Dragged) {
    let c = &theme.colors;
    let layer = egui::LayerId::new(egui::Order::Foreground, area_id().with("targets"));
    let painter = ctx.layer_painter(layer);
    let fill = c.accent.gamma_multiply(0.3);
    match drop {
        Drop::Dock(t) => {
            painter.rect_filled(t.preview, 0.0, fill);
            painter.rect_stroke(t.preview, 0.0, (1.0, c.accent), egui::StrokeKind::Inside);
        }
        Drop::Float => {
            let window = Rect::from_min_size(d.pointer, d.size);
            painter.rect_stroke(window, 0.0, (1.0, c.accent), egui::StrokeKind::Inside);
        }
        Drop::TabBar => {}
    }
    for t in found {
        let on = drop == Drop::Dock(*t);
        let (back, ink) = match on {
            true => (c.accent, c.window_bg),
            false => (c.window_bg, c.text_weak),
        };
        painter.rect(
            t.button,
            3.0,
            back,
            (1.0, c.accent),
            egui::StrokeKind::Inside,
        );
        // A small panel outline with the docked side filled in.
        let frame = Rect::from_center_size(t.button.center(), egui::vec2(18.0, 14.0));
        painter.rect_stroke(frame, 1.0, (1.0, ink), egui::StrokeKind::Inside);
        let part = match t.side {
            Side::Left => frame.with_max_x(frame.left() + 6.0),
            Side::Right => frame.with_min_x(frame.right() - 6.0),
            Side::Above => frame.with_max_y(frame.top() + 5.0),
            Side::Below => frame.with_min_y(frame.bottom() - 5.0),
        };
        painter.rect_filled(part, 1.0, ink);
    }
}
