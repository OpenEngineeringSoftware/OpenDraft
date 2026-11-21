//! The Select Color dialog: every AutoCAD index colour, laid out like
//! AutoCAD's Index Color tab, with a field that takes a name, a number or
//! R,G,B.

use eframe::egui::{self, Color32, Id, Rect, Sense, Stroke, Ui};
use opendraft_core::aci::aci_rgb;
use opendraft_ui::{widgets, Theme};

use super::names::{self, from_index, index_of};
use super::{paint_swatch, Pick};

const CELL: f32 = 18.0;
const HUES: u8 = 24;
const WIDTH: f32 = HUES as f32 * CELL + 32.0;
/// The shades of each hue, top row first: the full colours from light to
/// dark, then the pale ones from dark to light.
const SHADE_ROWS: [u8; 10] = [0, 2, 4, 6, 8, 9, 7, 5, 3, 1];

/// What the dialog holds while it is open.
#[derive(Clone, Default)]
pub struct State {
    pub chosen: Option<Pick>,
    pub text: String,
}

impl State {
    pub fn new(current: Option<Pick>) -> Self {
        Self {
            chosen: current,
            text: current.map(label).unwrap_or_default(),
        }
    }
}

fn label(pick: Pick) -> String {
    match pick {
        Pick::ByLayer => "ByLayer".to_owned(),
        Pick::ByBlock => "ByBlock".to_owned(),
        Pick::Color(color) => names::name(color),
    }
}

/// Shows the dialog kept under `id`; returns the colour once the user
/// presses OK (or double-clicks a colour). Closes itself on OK and Cancel.
pub fn show(ui: &Ui, theme: &Theme, id: Id, by_layer: bool) -> Option<Pick> {
    let mut state = ui.data(|d| d.get_temp::<State>(id))?;
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    widgets::dialog(ui.ctx(), theme, "Select Color", &mut open, WIDTH, |ui| {
        let chosen = chosen_index(&state);
        widgets::field_label(ui, theme, "Index colors");
        let grid: Vec<Vec<u8>> = SHADE_ROWS
            .iter()
            .map(|shade| (0..HUES).map(|hue| 10 + hue * 10 + shade).collect())
            .collect();
        let mut hit = block(ui, theme, &grid, chosen);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let standard = block(ui, theme, &[(1..=9).collect()], chosen);
            ui.add_space(CELL);
            let greys = block(ui, theme, &[(250..=255).collect()], chosen);
            hit = std::mem::take(&mut hit).or(standard).or(greys);
        });
        if let Some(i) = hit.clicked {
            let pick = Pick::Color(from_index(i));
            state.chosen = Some(pick);
            state.text = label(pick);
        }
        ui.add_space(6.0);
        ui.label(describe(hit.hovered.or_else(|| chosen_index(&state))));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::field_label(ui, theme, "Color:");
            let typed = ui.add(egui::TextEdit::singleline(&mut state.text).desired_width(140.0));
            if typed.changed() {
                state.chosen = names::parse(&state.text, by_layer);
            }
            if typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = state.chosen.is_some();
            }
            let (rect, _) = ui.allocate_exact_size(egui::vec2(32.0, 20.0), Sense::hover());
            match state.chosen {
                Some(Pick::Color(color)) => paint_swatch(ui.painter(), theme, rect, color),
                _ => {
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0_f32, theme.colors.border),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            if by_layer {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for pick in [Pick::ByBlock, Pick::ByLayer] {
                        if widgets::secondary_button(ui, theme, &label(pick)).clicked() {
                            state.chosen = Some(pick);
                            state.text = label(pick);
                        }
                    }
                });
            }
        });
        if state.chosen.is_none() && !state.text.trim().is_empty() {
            ui.colored_label(
                ui.visuals().error_fg_color,
                "Type a color name, a number from 1 to 255, or R,G,B.",
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = widgets::secondary_button(ui, theme, "Cancel").clicked();
                ui.add_enabled_ui(state.chosen.is_some(), |ui| {
                    ok |= widgets::primary_button(ui, theme, "OK").clicked();
                });
            });
        });
        ok |= hit.double;
    });
    let picked = state.chosen.filter(|_| ok);
    ui.data_mut(|d| {
        if ok || cancel || !open {
            d.remove::<State>(id);
        } else {
            d.insert_temp(id, state);
        }
    });
    picked
}

fn chosen_index(state: &State) -> Option<u8> {
    match state.chosen {
        Some(Pick::Color(color)) => index_of(color),
        _ => None,
    }
}

/// "Index color: 34   Red, Green, Blue: 255,170,0".
fn describe(index: Option<u8>) -> String {
    match index {
        Some(7) => "Index color: 7   White or black, whichever shows on the background".to_owned(),
        Some(i) => {
            let (r, g, b) = aci_rgb(i);
            format!("Index color: {i}   Red, Green, Blue: {r},{g},{b}")
        }
        None => " ".to_owned(),
    }
}

/// What the pointer did over a block of cells.
#[derive(Default)]
struct Hit {
    hovered: Option<u8>,
    clicked: Option<u8>,
    double: bool,
}

impl Hit {
    fn or(self, other: Hit) -> Hit {
        Hit {
            hovered: self.hovered.or(other.hovered),
            clicked: self.clicked.or(other.clicked),
            double: self.double || other.double,
        }
    }
}

/// Rows of colour cells drawn edge to edge in a frame; colour 7 is half
/// white, half black. The chosen cell is ringed, and so is the one under
/// the pointer.
fn block(ui: &mut Ui, theme: &Theme, rows: &[Vec<u8>], chosen: Option<u8>) -> Hit {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let size = egui::vec2(columns as f32 * CELL, rows.len() as f32 * CELL);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let pointer = response.hover_pos();
    let mut hit = Hit::default();
    for (r, row) in rows.iter().enumerate() {
        for (c, &i) in row.iter().enumerate() {
            let at = cell_rect(rect, r, c);
            if i == 7 {
                painter.extend(half_and_half(at, 0.0));
            } else {
                let (red, green, blue) = aci_rgb(i);
                painter.rect_filled(at, 0.0, Color32::from_rgb(red, green, blue));
            }
            if pointer.is_some_and(|p| at.contains(p)) {
                hit.hovered = Some(i);
            }
        }
    }
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0_f32, theme.colors.border),
        egui::StrokeKind::Outside,
    );
    for (r, row) in rows.iter().enumerate() {
        for (c, &i) in row.iter().enumerate() {
            let selected = chosen == Some(i);
            if selected || hit.hovered == Some(i) {
                painter.rect_stroke(
                    cell_rect(rect, r, c),
                    0.0,
                    Stroke::new(if selected { 2.0_f32 } else { 1.5 }, theme.colors.text),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
    if response.clicked() || response.double_clicked() {
        hit.clicked = hit.hovered;
        hit.double = hit.hovered.is_some() && response.double_clicked();
    }
    hit
}

/// The square of row `r`, column `c` in a block whose corner is `rect.min`.
fn cell_rect(rect: Rect, r: usize, c: usize) -> Rect {
    let min = rect.min + egui::vec2(c as f32 * CELL, r as f32 * CELL);
    Rect::from_min_size(min, egui::vec2(CELL, CELL))
}

/// The swatch of colour 7: white above the diagonal, black below, its
/// corners rounded by `rounding`.
pub fn half_and_half(rect: Rect, rounding: f32) -> [egui::Shape; 2] {
    [
        egui::Shape::rect_filled(rect, rounding, Color32::WHITE),
        egui::Shape::convex_polygon(
            vec![rect.right_top(), rect.right_bottom(), rect.left_bottom()],
            Color32::BLACK,
            Stroke::NONE,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_holds_every_shade_once() {
        let mut seen: Vec<u8> = SHADE_ROWS
            .iter()
            .flat_map(|shade| (0..HUES).map(move |hue| 10 + hue * 10 + shade))
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (10..=249).collect::<Vec<u8>>());
    }

    #[test]
    fn the_cells_touch() {
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(100.0, 100.0));
        assert_eq!(cell_rect(rect, 0, 0).right(), cell_rect(rect, 0, 1).left());
        assert_eq!(cell_rect(rect, 0, 0).bottom(), cell_rect(rect, 1, 0).top());
        assert_eq!(
            cell_rect(rect, 2, 3).min,
            egui::pos2(10.0 + 3.0 * CELL, 20.0 + 2.0 * CELL)
        );
    }

    #[test]
    fn the_description_gives_the_rgb() {
        assert_eq!(
            describe(Some(1)),
            "Index color: 1   Red, Green, Blue: 255,0,0"
        );
        assert!(describe(Some(7)).contains("White or black"));
    }

    #[test]
    fn a_new_dialog_shows_the_current_colour() {
        let state = State::new(Some(Pick::Color(from_index(34))));
        assert_eq!(state.text, "34");
        assert_eq!(State::new(Some(Pick::ByLayer)).text, "ByLayer");
        assert_eq!(State::new(None).text, "");
    }
}
