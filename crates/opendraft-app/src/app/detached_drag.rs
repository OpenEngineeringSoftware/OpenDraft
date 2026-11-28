//! The handle on a detached panel's window that drags it back into the dock.
//! The title bar is the system's, and a drag of it gives the app no release
//! to act on, so docking is done from this handle: the window follows the
//! pointer (shrunk to its strip, so it does not hide the drop targets drawn
//! in the main window) and the release says where it goes.

use eframe::egui::{self, Pos2, Vec2};
use opendraft_ui::{icons, widgets, Theme};

use crate::dock::PanelTab;

/// The window while it is carried: just its header.
const CARRIED: Vec2 = Vec2::new(200.0, 28.0);

/// Where the pointer is held on the carried window.
const GRIP: Vec2 = Vec2::new(100.0, 12.0);

/// The size the window had, and the pointer as last seen, while carried.
#[derive(Clone, Copy)]
struct Carry {
    size: Vec2,
    pointer: Pos2,
}

/// A handle was dragged this frame.
pub(super) struct Moved {
    /// The pointer on the screen (not in a window).
    pub pointer: Pos2,
    pub released: bool,
}

/// The least size of a panel's window.
pub(super) const LEAST: [f32; 2] = [200.0, 120.0];

/// Whether the app can place a window where it likes. Wayland lets only the
/// compositor do that, so there the handle is not offered and the Dock
/// button is the way back.
fn can_place_windows() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("WAYLAND_DISPLAY").is_none()
}

/// What the header of a window asked for.
pub(super) struct Header {
    pub dock: bool,
    pub moved: Option<Moved>,
}

/// The header of a detached window: the panel's title and a Dock button, in
/// the tone of a docked tab bar. The title and the space beside it are the
/// handle that carries the window back to the dock (a double click docks it
/// where it was).
pub(super) fn header(ctx: &egui::Context, theme: &Theme, panel: PanelTab, title: &str) -> Header {
    let mut out = Header {
        dock: false,
        moved: None,
    };
    let c = &theme.colors;
    egui::TopBottomPanel::top(egui::Id::new(("panel_header", panel)))
        .frame(
            egui::Frame::NONE
                .fill(c.tab_strip())
                .inner_margin(egui::Margin::symmetric(8, 1)),
        )
        .show_separator_line(false)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let side = ui.spacing().interact_size.y;
                if widgets::icon_button(ui, theme, icons::PANEL_DOCK, "Dock").clicked() {
                    out.dock = true;
                }
                let space = egui::vec2(ui.available_width(), side);
                let grip = ui.allocate_response(space, egui::Sense::click_and_drag());
                let at = grip.rect.left_center();
                ui.painter().text(
                    at,
                    egui::Align2::LEFT_CENTER,
                    title,
                    egui::FontId::proportional(theme.font_size_body),
                    c.text,
                );
                // No tooltip while carrying: it would hang over the drop targets.
                let grip = grip.on_hover_cursor(egui::CursorIcon::Grab);
                let grip = match grip.dragged() {
                    true => grip,
                    false => grip.on_hover_text("Drag to dock, or double-click"),
                };
                out.dock |= grip.double_clicked();
                out.moved = carry(ui.ctx(), &grip, panel);
            });
        });
    out
}

/// Carry the window with the pointer while the handle is dragged, and report
/// where the pointer is.
fn carry(ctx: &egui::Context, drag: &egui::Response, panel: PanelTab) -> Option<Moved> {
    if !can_place_windows() {
        return None;
    }
    let key = egui::Id::new(("panel_carry", panel));
    let (inner, outer, local) = ctx.input(|i| {
        (
            i.viewport().inner_rect,
            i.viewport().outer_rect,
            i.pointer.latest_pos(),
        )
    });
    let pointer = inner.zip(local).map(|(inner, p)| inner.min + p.to_vec2());
    if drag.drag_started() {
        let size = inner.map_or(Vec2::from(LEAST), |r| r.size());
        let pointer = pointer.unwrap_or_default();
        ctx.data_mut(|d| d.insert_temp(key, Carry { size, pointer }));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(CARRIED));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(CARRIED));
    }
    if drag.dragged() {
        let (pointer, inner, outer) = (pointer?, inner?, outer?);
        ctx.data_mut(|d| {
            if let Some(carry) = d.get_temp::<Carry>(key) {
                d.insert_temp(key, Carry { pointer, ..carry });
            }
        });
        let frame = inner.min - outer.min;
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pointer - GRIP - frame));
        ctx.request_repaint();
        return Some(Moved {
            pointer,
            released: false,
        });
    }
    if drag.drag_stopped() {
        let carry: Carry = ctx.data(|d| d.get_temp(key))?;
        ctx.data_mut(|d| d.remove::<Carry>(key));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(Vec2::from(LEAST)));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(carry.size));
        return Some(Moved {
            pointer: carry.pointer,
            released: true,
        });
    }
    None
}
