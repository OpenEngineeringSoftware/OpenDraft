//! The handle on a detached panel's window that drags it back into the dock.
//! The title bar is the system's, and a drag of it gives the app no release
//! to act on, so docking is done from this handle: the window follows the
//! pointer (shrunk to its strip, so it does not hide the drop targets drawn
//! in the main window) and the release says where it goes.

use eframe::egui::{self, Pos2, Vec2};

use crate::dock::PanelTab;

/// The window while it is carried: just its strip.
const CARRIED: Vec2 = Vec2::new(200.0, 34.0);

/// Where the pointer is held on the carried window.
const GRIP: Vec2 = Vec2::new(100.0, 15.0);

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

/// The handle in the window's strip. Moves the window with the pointer while
/// it is dragged, and reports where the pointer is.
pub(super) fn handle(ui: &mut egui::Ui, panel: PanelTab) -> Option<Moved> {
    if !can_place_windows() {
        return None;
    }
    let ctx = ui.ctx().clone();
    let label = egui::RichText::new(opendraft_ui::i18n::tr("Drag to dock"))
        .color(ui.visuals().weak_text_color());
    let drag = ui
        .add(egui::Label::new(label).sense(egui::Sense::drag()))
        .on_hover_cursor(egui::CursorIcon::Grab);
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
