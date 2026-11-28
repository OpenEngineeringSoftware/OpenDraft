//! A dialog shown as a real OS window instead of a window inside the main one.
//! The user chooses it in Settings, Display; `containers::dialog` chooses
//! between the two. Without a choice made on the context, a dialog is a window
//! inside the main one.

use egui::{Margin, Ui};

use super::placement;

use crate::Theme;

fn mode_id() -> egui::Id {
    egui::Id::new("dialogs_in_windows")
}

/// Choose where dialogs open, for what `ctx` shows: `true` is their own OS
/// windows. Kept on the context, so two contexts (two tests) do not share it.
pub fn set_dialogs_in_windows(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(mode_id(), on));
}

/// Keep the window `ctx` is drawing above the main window while this program
/// is the active one, and an ordinary window when another program is (so it
/// does not cover that program). `key` is the window's own, to remember which
/// it is in.
pub fn keep_above_main(ctx: &egui::Context, key: egui::Id) {
    let mine = ctx.input(|i| i.viewport().focused).unwrap_or(false);
    let root = ctx
        .input_for(egui::ViewportId::ROOT, |i| i.viewport().focused)
        .unwrap_or(false);
    let top = mine || root;
    if ctx.data(|d| d.get_temp::<bool>(key)) != Some(top) {
        ctx.data_mut(|d| d.insert_temp(key, top));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        }));
    }
}

fn epoch_id() -> egui::Id {
    egui::Id::new("dialog_positions_epoch")
}

fn epoch(ctx: &egui::Context) -> u64 {
    ctx.data(|d| d.get_temp::<u64>(epoch_id())).unwrap_or(0)
}

/// Forget where dialog windows were left, so the next one opens in the middle
/// of the main window: for a dialog left on a monitor that has gone.
pub fn bring_dialogs_back(ctx: &egui::Context) {
    let next = epoch(ctx) + 1;
    ctx.data_mut(|d| d.insert_temp(epoch_id(), next));
}

/// The pixels (of the whole screen) at which a window of `size` points has its
/// top-left corner when it is in the middle of the main window.
fn centred_px(ctx: &egui::Context, size: egui::Vec2) -> Option<egui::Pos2> {
    let main = ctx.input(|i| i.viewport().outer_rect)?;
    let ppp = ctx.pixels_per_point();
    Some(((main.center().to_vec2() - size / 2.0) * ppp).to_pos2())
}

pub(super) fn viewport_mode(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(mode_id())).unwrap_or(false)
}

/// The dialog as its own OS window (an immediate viewport): the same body,
/// in a native frame the user can drag to another monitor.
pub(super) fn dialog_window<R>(
    ctx: &egui::Context,
    theme: &Theme,
    title: &str,
    open: &mut bool,
    width: f32,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let c = &theme.colors;
    let margin = 16.0;
    let size_id = egui::Id::new(("dialog_vp_size", title));
    let known = ctx.data(|d| d.get_temp::<egui::Vec2>(size_id));
    let size = known.unwrap_or(egui::vec2(width + 2.0 * margin, 160.0));
    // Position, in pixels of the whole screen (see `placement`): where it was
    // last left, else centred over the main window. Fixed for as long as the
    // dialog stays open, so it does not fight the user's drag.
    let pass = ctx.cumulative_pass_nr();
    let seen_id = egui::Id::new(("dialog_vp_seen", title));
    let last_pos_id = egui::Id::new(("dialog_vp_last_pos", title));
    let start_id = egui::Id::new(("dialog_vp_start", title));
    let centred_id = egui::Id::new(("dialog_vp_centred", title));
    let placed_id = egui::Id::new(("dialog_vp_placed", title));
    let epoch_now = epoch(ctx);
    let seen = ctx.data(|d| d.get_temp::<u64>(seen_id));
    ctx.data_mut(|d| d.insert_temp(seen_id, pass));
    let fresh = !matches!(seen, Some(s) if s + 1 >= pass);
    if fresh {
        let remembered = ctx
            .data(|d| d.get_temp::<(egui::Pos2, u64)>(last_pos_id))
            .filter(|(_, e)| *e == epoch_now)
            .map(|(p, _)| p);
        let start = remembered.or_else(|| centred_px(ctx, size));
        placement::forget(ctx, placed_id);
        ctx.data_mut(|d| d.insert_temp(centred_id, remembered.is_none()));
        ctx.data_mut(|d| match start {
            Some(p) => d.insert_temp(start_id, p),
            None => d.remove::<egui::Pos2>(start_id),
        });
    }
    let builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_resizable(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    let id = egui::ViewportId::from_hash_of(("dialog_vp", title));
    let mut add = Some(add_contents);
    let mut result = None;
    let mut used = None;
    ctx.show_viewport_immediate(id, builder, |ctx, _class| {
        keep_above_main(ctx, egui::Id::new(("dialog_vp_top", title)));
        // Put where it is meant to be, by commands from in here, where the
        // window's own scale is known; remember where it is only once it is.
        let placed = match ctx.data(|d| d.get_temp::<egui::Pos2>(start_id)) {
            Some(at) => placement::settle(ctx, placed_id, at),
            None => true,
        };
        if let (true, Some(outer)) = (placed, ctx.input(|i| i.viewport().outer_rect)) {
            let at = (outer.min.to_vec2() * ctx.pixels_per_point()).to_pos2();
            ctx.data_mut(|d| d.insert_temp(last_pos_id, (at, epoch_now)));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(c.panel_bg)
                    .inner_margin(Margin::same(margin as i8)),
            )
            .show(ctx, |ui| {
                // The panel fills the window, so its own size says nothing
                // about the content; measure the content in a scope of its own.
                let content = ui.scope(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(theme.unit, theme.unit * 0.5);
                    ui.set_width(width);
                    add.take().map(|f| f(ui))
                });
                result = content.inner;
                used = Some(content.response.rect.size() + egui::vec2(2.0 * margin, 2.0 * margin));
            });
        if ctx.input(|i| i.viewport().close_requested()) {
            *open = false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !ctx.memory(|m| m.any_popup_open()) {
            *open = false;
        }
    });
    if let Some(used) = used {
        if known != Some(used) {
            ctx.data_mut(|d| d.insert_temp(size_id, used));
            ctx.send_viewport_cmd_to(id, egui::ViewportCommand::InnerSize(used));
            // A first guess of the size centred it; centre it on the real one.
            let centred =
                known.is_none() && ctx.data(|d| d.get_temp::<bool>(centred_id)) == Some(true);
            if let (true, Some(at)) = (centred, centred_px(ctx, used)) {
                ctx.data_mut(|d| d.insert_temp(start_id, at));
            }
        }
    }
    result
}
