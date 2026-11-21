//! Colour pickers shared by the Layers and Properties panels: a swatch or a
//! swatch-and-name field that opens a short list (the named colours, the
//! ones used lately and "Select Colors..."), and the Select Color dialog with
//! every AutoCAD index colour.

mod index_dialog;
mod names;

use eframe::egui::{
    self, Align2, Color32, FontId, Id, Painter, Rect, Response, Sense, Shape, Stroke, Ui,
};
use opendraft_core::Color;
use opendraft_ui::{menu, Theme};

pub use names::name;

/// What the user picked in the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    ByLayer,
    ByBlock,
    Color(Color),
}

/// How many colours "used lately" the list keeps.
const RECENT: usize = 4;
const ROW_HEIGHT: f32 = 22.0;
const LIST_WIDTH: f32 = 170.0;

/// The colour as drawn on screen.
pub fn display(theme: &Theme, color: Color) -> Color32 {
    match color {
        Color::Foreground => theme.colors.text,
        Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// A colour's swatch with its border. Colour 7 is half white, half black
/// whatever the theme, so it reads as "white on a dark background, black on
/// a light one".
pub fn paint_swatch(painter: &Painter, theme: &Theme, rect: Rect, color: Color) {
    painter.extend(swatch_shapes(theme, rect, color));
}

fn swatch_shapes(theme: &Theme, rect: Rect, color: Color) -> Vec<Shape> {
    let mut shapes = if color == Color::Foreground {
        index_dialog::half_and_half(rect, 1.0).to_vec()
    } else {
        vec![Shape::rect_filled(rect, 2.0, display(theme, color))]
    };
    shapes.push(Shape::rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0_f32, theme.colors.border),
        egui::StrokeKind::Inside,
    ));
    shapes
}

/// A square swatch that opens the palette (layer table cells). `by_layer`
/// adds ByLayer and ByBlock entries, for objects.
pub fn swatch_button(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    color: Color,
    by_layer: bool,
) -> Option<Pick> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 20.0), Sense::click());
    let inner = rect.shrink2(egui::vec2(3.0, 2.0));
    paint_swatch(ui.painter(), theme, inner, color);
    if response.hovered() {
        ui.painter().rect_stroke(
            inner.expand(1.0),
            2.0,
            Stroke::new(1.0_f32, theme.colors.text_weak),
            egui::StrokeKind::Outside,
        );
    }
    palette_popup(ui, theme, id, &response, Some(Pick::Color(color)), by_layer)
}

/// A drop-down field showing an optional swatch and a label ("Red", "ByLayer",
/// "Varies"). Opens the palette.
pub fn field(
    ui: &mut Ui,
    theme: &Theme,
    id: Id,
    size: egui::Vec2,
    swatch_color: Option<Color>,
    label: &str,
    by_layer: bool,
) -> Option<Pick> {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    menu::paint_combo(ui, theme, rect, "", response.hovered());
    let mut text_x = rect.left() + 8.0;
    if let Some(color) = swatch_color {
        let box_rect = Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            egui::vec2(12.0, 12.0),
        );
        paint_swatch(ui.painter(), theme, box_rect, color);
        text_x = rect.left() + 28.0;
    }
    ui.painter().text(
        egui::pos2(text_x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(theme.font_size_body),
        theme.colors.text,
    );
    // A ByLayer field shows the layer's swatch, but its colour is ByLayer.
    let current = match (label, swatch_color) {
        ("ByLayer", _) => Some(Pick::ByLayer),
        ("ByBlock", _) => Some(Pick::ByBlock),
        (_, Some(color)) => Some(Pick::Color(color)),
        _ => None,
    };
    palette_popup(ui, theme, id, &response, current, by_layer)
}

/// What a row of the short list does.
enum Choice {
    Pick(Pick),
    MoreColours,
}

fn palette_popup(
    ui: &Ui,
    theme: &Theme,
    id: Id,
    anchor: &Response,
    current: Option<Pick>,
    by_layer: bool,
) -> Option<Pick> {
    let dialog_id = id.with("select_color");
    let listed = menu::popup(ui, theme, id, anchor, LIST_WIDTH, |ui| {
        short_list(ui, theme, by_layer)
    });
    let picked = match listed {
        Some(Choice::Pick(pick)) => Some(pick),
        Some(Choice::MoreColours) => {
            let state = index_dialog::State::new(current);
            ui.data_mut(|d| d.insert_temp(dialog_id, state));
            None
        }
        None => index_dialog::show(ui, theme, dialog_id, by_layer),
    };
    if let Some(Pick::Color(color)) = picked {
        remember(ui, color);
    }
    picked
}

/// ByLayer and ByBlock (for objects), the seven named colours, the colours
/// used lately, and "Select Colors...".
fn short_list(ui: &mut Ui, theme: &Theme, by_layer: bool) -> Option<Choice> {
    let mut picked = None;
    if by_layer && menu::menu_row(ui, theme, "ByLayer", false) {
        picked = Some(Choice::Pick(Pick::ByLayer));
    }
    if by_layer && menu::menu_row(ui, theme, "ByBlock", false) {
        picked = Some(Choice::Pick(Pick::ByBlock));
    }
    let standard = (1..=7).map(names::from_index);
    let recent: Vec<Color> = recent(ui)
        .into_iter()
        .filter(|&c| !names::index_of(c).is_some_and(|i| i <= 7))
        .collect();
    for (k, color) in standard.chain(recent.iter().copied()).enumerate() {
        if k == 7 {
            separator(ui, theme);
        }
        if colour_row(ui, theme, color) {
            picked = Some(Choice::Pick(Pick::Color(color)));
        }
    }
    separator(ui, theme);
    if menu::menu_row(ui, theme, "Select Colors...", false) {
        picked = Some(Choice::MoreColours);
    }
    picked
}

/// A list row with the colour's swatch and name; returns whether clicked.
fn colour_row(ui: &mut Ui, theme: &Theme, color: Color) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 2.0, theme.colors.hover);
    }
    let box_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        egui::vec2(12.0, 12.0),
    );
    paint_swatch(ui.painter(), theme, box_rect, color);
    ui.painter().text(
        egui::pos2(rect.left() + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        name(color),
        FontId::proportional(theme.font_size_body),
        theme.colors.text,
    );
    response.clicked()
}

fn separator(ui: &mut Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 7.0), Sense::hover());
    ui.painter().hline(
        rect.x_range().shrink(4.0),
        rect.center().y,
        Stroke::new(1.0_f32, theme.colors.border),
    );
}

fn recent_id() -> Id {
    Id::new("colour_ui_recent")
}

fn recent(ui: &Ui) -> Vec<Color> {
    ui.data(|d| d.get_temp::<Vec<Color>>(recent_id()))
        .unwrap_or_default()
}

/// Puts `color` first among the colours used lately.
fn remember(ui: &Ui, color: Color) {
    let list = with_recent(recent(ui), color);
    ui.data_mut(|d| d.insert_temp(recent_id(), list));
}

fn with_recent(mut list: Vec<Color>, color: Color) -> Vec<Color> {
    if names::index_of(color).is_some_and(|i| i <= 7) {
        return list;
    }
    list.retain(|&c| c != color);
    list.insert(0, color);
    list.truncate(RECENT);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_colours_skip_the_named_ones_and_keep_four() {
        let mut list = Vec::new();
        for i in [30u8, 1, 40, 33, 30, 60, 70] {
            list = with_recent(list, names::from_index(i));
        }
        let expected: Vec<Color> = [70u8, 60, 30, 33].map(names::from_index).to_vec();
        assert_eq!(list, expected);
    }

    /// The fill colours of a swatch's shapes, border left out.
    fn fills(color: Color) -> Vec<Color32> {
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(12.0, 12.0));
        swatch_shapes(&Theme::dark(), rect, color)
            .into_iter()
            .filter_map(|shape| match shape {
                Shape::Rect(r) => Some(r.fill),
                Shape::Path(p) => Some(p.fill),
                _ => None,
            })
            .filter(|c| *c != Color32::TRANSPARENT)
            .collect()
    }

    #[test]
    fn colour_7_is_half_white_half_black_and_others_are_plain() {
        assert_eq!(fills(Color::Foreground), [Color32::WHITE, Color32::BLACK]);
        assert_eq!(fills(Color::Rgb(255, 0, 0)), [Color32::from_rgb(255, 0, 0)]);
    }
}
