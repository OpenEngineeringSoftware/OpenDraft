//! The frame of a detached panel's window, drawn by the app: the system's
//! title bar is off, so the window gets a one-point border and edges that
//! start a resize by the system.

use eframe::egui::{self, CursorIcon, Pos2, ResizeDirection, Vec2};
use opendraft_ui::Theme;

use crate::dock::PanelTab;

/// How wide the edge and corner zones that start a resize are.
const EDGE: f32 = 5.0;

/// A border round the window, in the tone of a docked panel's.
pub(super) fn border(ctx: &egui::Context, theme: &Theme) {
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("popout_border"));
    ctx.layer_painter(layer).rect_stroke(
        ctx.screen_rect(),
        0.0,
        egui::Stroke::new(1.0_f32, theme.colors.border),
        egui::StrokeKind::Inside,
    );
}

/// The edges and corners: a press on one hands the resize to the system.
pub(super) fn resize_zones(ctx: &egui::Context, panel: PanelTab) {
    let r = ctx.screen_rect();
    let (w, h) = (r.width(), r.height());
    let across = Vec2::new(w - 2.0 * EDGE, EDGE);
    let down = Vec2::new(EDGE, h - 2.0 * EDGE);
    let corner = Vec2::splat(EDGE);
    let zones = [
        (
            ResizeDirection::North,
            Pos2::new(EDGE, 0.0),
            across,
            CursorIcon::ResizeVertical,
        ),
        (
            ResizeDirection::South,
            Pos2::new(EDGE, h - EDGE),
            across,
            CursorIcon::ResizeVertical,
        ),
        (
            ResizeDirection::West,
            Pos2::new(0.0, EDGE),
            down,
            CursorIcon::ResizeHorizontal,
        ),
        (
            ResizeDirection::East,
            Pos2::new(w - EDGE, EDGE),
            down,
            CursorIcon::ResizeHorizontal,
        ),
        (
            ResizeDirection::NorthWest,
            Pos2::ZERO,
            corner,
            CursorIcon::ResizeNwSe,
        ),
        (
            ResizeDirection::NorthEast,
            Pos2::new(w - EDGE, 0.0),
            corner,
            CursorIcon::ResizeNeSw,
        ),
        (
            ResizeDirection::SouthWest,
            Pos2::new(0.0, h - EDGE),
            corner,
            CursorIcon::ResizeNeSw,
        ),
        (
            ResizeDirection::SouthEast,
            Pos2::new(w - EDGE, h - EDGE),
            corner,
            CursorIcon::ResizeNwSe,
        ),
    ];
    for (n, (direction, at, size, cursor)) in zones.into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("popout_resize", panel, n)))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| {
                let zone = ui.allocate_response(size, egui::Sense::drag());
                if zone.hovered() || zone.dragged() {
                    ui.ctx().set_cursor_icon(cursor);
                }
                if zone.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
                }
            });
    }
}
