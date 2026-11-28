//! The palette's frame: the toolbar, the two titled sections and the handle
//! between them.

use eframe::egui::{self, Align, Layout, Rect, Sense, Stroke, Ui};
use opendraft_ui::{icons, menu, widgets, Theme};

use super::actions::Action;
use super::rows::Row;

/// The height of a section's title strip.
const TITLE: f32 = 26.0;

/// The height of the handle between the sections.
pub(super) const HANDLE: f32 = 7.0;

/// The least height of a section.
pub(super) const LEAST: f32 = 70.0;

/// A framed section `height` tall with a title strip. `content` gets the
/// strip's right-hand end (for buttons) and the body.
pub(super) fn section(
    ui: &mut Ui,
    theme: &Theme,
    height: f32,
    title: &str,
    content: impl FnOnce(&mut Ui, &mut Ui),
) {
    let c = &theme.colors;
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 0.0, c.panel_bg);
    let strip = Rect::from_min_size(rect.min, egui::vec2(rect.width(), TITLE));
    painter.rect_filled(strip, 0.0, c.tab_strip());
    painter.text(
        strip.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(theme.font_size_body),
        c.text,
    );
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, c.border),
        egui::StrokeKind::Inside,
    );
    let mut head = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("xref-head", title))
            .max_rect(strip.shrink2(egui::vec2(4.0, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    let body_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 1.0, strip.bottom()),
        egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0),
    );
    let mut body = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("xref-body", title))
            .max_rect(body_rect),
    );
    body.set_clip_rect(body_rect.intersect(ui.clip_rect()));
    content(&mut head, &mut body);
}

/// The handle between the sections; the change in the first one's height
/// that a drag of it asks for.
pub(super) fn handle(ui: &mut Ui, theme: &Theme) -> f32 {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), HANDLE), Sense::drag());
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let c = &theme.colors;
    let colour = if response.hovered() || response.dragged() {
        c.text_weak
    } else {
        c.border_strong
    };
    // Four dots, as AutoCAD's handle has.
    for i in -2..2 {
        ui.painter().circle_filled(
            rect.center() + egui::vec2(i as f32 * 5.0 + 2.5, 0.0),
            1.2,
            colour,
        );
    }
    if response.dragged() {
        response.drag_delta().y
    } else {
        0.0
    }
}

/// The toolbar: what was asked for, if anything.
pub(super) fn toolbar(ui: &mut Ui, theme: &Theme, selected: Option<&Row>) -> Option<Action> {
    let mut asked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        let button = |ui: &mut Ui, action: Action, asked: &mut Option<Action>| {
            let refusal = action.refused(selected);
            let tip = match refusal {
                Some(why) => format!(
                    "{}: {}",
                    opendraft_ui::i18n::tr(action.label()),
                    opendraft_ui::i18n::tr(why)
                ),
                None => opendraft_ui::i18n::tr(action.label()).to_string(),
            };
            let clicked = ui
                .add_enabled_ui(refusal.is_none(), |ui| {
                    widgets::icon_button(ui, theme, action.icon(), &tip).clicked()
                })
                .inner;
            if clicked {
                *asked = Some(action);
            }
        };
        button(ui, Action::Attach, &mut asked);
        button(ui, Action::Reload, &mut asked);
        let arrow = widgets::icon_button(ui, theme, icons::DROPDOWN_ARROW, &rust_i18n::t!("More"));
        let picked = menu::popup(
            ui,
            theme,
            egui::Id::new("xref-reload-menu"),
            &arrow,
            150.0,
            |ui| {
                [Action::Reload, Action::ReloadAll, Action::Refresh]
                    .into_iter()
                    .find(|a| {
                        ui.add_enabled_ui(a.refused(selected).is_none(), |ui| {
                            menu::menu_row(ui, theme, &opendraft_ui::i18n::tr(a.label()), false)
                        })
                        .inner
                    })
            },
        );
        asked = asked.or(picked);
        ui.add_space(6.0);
        for action in [
            Action::Unload,
            Action::Detach,
            Action::Path,
            Action::Bind,
            Action::Clip,
        ] {
            button(ui, action, &mut asked);
        }
    });
    asked
}
