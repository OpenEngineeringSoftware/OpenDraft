//! A dialog shown as a real OS window instead of a window inside the main one.
//! The user chooses it in Settings, Display; `containers::dialog` chooses
//! between the two. Without a choice made on the context, a dialog is a window
//! inside the main one.

use egui::{Margin, Ui};

use crate::Theme;

fn mode_id() -> egui::Id {
    egui::Id::new("dialogs_in_windows")
}

/// Choose where dialogs open, for what `ctx` shows: `true` is their own OS
/// windows. Kept on the context, so two contexts (two tests) do not share it.
pub fn set_dialogs_in_windows(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(mode_id(), on));
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
    let size = known.unwrap_or(egui::vec2(width + 2.0 * margin, 300.0));
    // Position: where it was last left, else centred over the main window.
    // Fixed for as long as the dialog stays open (a builder that keeps
    // changing would fight the user's drag).
    let pass = ctx.cumulative_pass_nr();
    let seen_id = egui::Id::new(("dialog_vp_seen", title));
    let last_pos_id = egui::Id::new(("dialog_vp_last_pos", title));
    let start_id = egui::Id::new(("dialog_vp_start", title));
    let seen = ctx.data(|d| d.get_temp::<u64>(seen_id));
    ctx.data_mut(|d| d.insert_temp(seen_id, pass));
    let fresh = !matches!(seen, Some(s) if s + 1 >= pass);
    if fresh {
        let remembered = ctx.data(|d| d.get_temp::<egui::Pos2>(last_pos_id));
        let main = ctx.input(|i| i.viewport().outer_rect);
        let start = remembered.or(main.map(|m| m.center() - size / 2.0));
        ctx.data_mut(|d| match start {
            Some(p) => d.insert_temp(start_id, p),
            None => d.remove::<egui::Pos2>(start_id),
        });
    }
    let mut builder = egui::ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_resizable(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop);
    if let Some(p) = ctx.data(|d| d.get_temp::<egui::Pos2>(start_id)) {
        builder = builder.with_position(p);
    }
    let id = egui::ViewportId::from_hash_of(("dialog_vp", title));
    let mut add = Some(add_contents);
    let mut result = None;
    let mut used = None;
    ctx.show_viewport_immediate(id, builder, |ctx, _class| {
        // Above the main window while the app is the active program; an
        // ordinary window when another program is (so it does not cover it).
        let mine = ctx.input(|i| i.viewport().focused).unwrap_or(false);
        let root = ctx
            .input_for(egui::ViewportId::ROOT, |i| i.viewport().focused)
            .unwrap_or(false);
        let top = mine || root;
        let level_id = egui::Id::new(("dialog_vp_top", title));
        if ctx.data(|d| d.get_temp::<bool>(level_id)) != Some(top) {
            ctx.data_mut(|d| d.insert_temp(level_id, top));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
        }
        if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
            ctx.data_mut(|d| d.insert_temp(last_pos_id, outer.min));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(c.panel_bg)
                    .inner_margin(Margin::same(margin as i8)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(theme.unit, theme.unit * 0.5);
                ui.set_width(width);
                if let Some(f) = add.take() {
                    result = Some(f(ui));
                }
                used = Some(ui.min_rect().size() + egui::vec2(2.0 * margin, 2.0 * margin));
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
        }
    }
    result
}
