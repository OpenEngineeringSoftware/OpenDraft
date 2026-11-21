//! The Select Color dialog: an Index Color tab with every AutoCAD index
//! colour (`index_tab`) and a True Color tab for any colour by HSL or RGB
//! (`true_tab`), over a Color field that takes a name, a number or R,G,B.

mod index_tab;
mod shading;
mod true_tab;

use eframe::egui::{self, Color32, Id, Rect, Sense, Stroke, Ui};
use opendraft_core::hsl::Hsl;
use opendraft_core::Color;
use opendraft_ui::{widgets, Theme};

use super::names::{self, index_of};
use super::{paint_swatch, Pick};
use true_tab::Model;

/// The dialog body's side margins and the tab page's, added to the grid's
/// width.
const MARGINS: f32 = 32.0 + 26.0;
const TABS: [&str; 2] = ["Index Color", "True Color"];
const INDEX_TAB: usize = 0;
const TRUE_TAB: usize = 1;
/// The tab pages' height, so the dialog keeps its size between tabs.
const PAGE_HEIGHT: f32 = 262.0;

/// What the dialog holds while it is open.
#[derive(Clone, Default)]
pub struct State {
    pub chosen: Option<Pick>,
    pub text: String,
    tab: usize,
    /// The True Color tab's colour, kept apart from `chosen` so a hue
    /// outlives a saturation of 0 (where every hue is the same grey).
    hsl: Hsl,
    model: Model,
}

impl State {
    /// Opens on the True Color tab for a colour with no index, as AutoCAD
    /// does, and on the Index Color tab otherwise.
    pub fn new(current: Option<Pick>) -> Self {
        let true_colour = matches!(current, Some(Pick::Color(c)) if index_of(c).is_none());
        let mut state = Self {
            tab: if true_colour { TRUE_TAB } else { INDEX_TAB },
            // ByLayer, ByBlock or nothing leave the True Color tab on white.
            hsl: Hsl::from_rgb((255, 255, 255)),
            ..Self::default()
        };
        if let Some(pick) = current {
            state.choose(pick);
        }
        state
    }

    /// Chooses `pick`, writing it in the Color field in the tab's form.
    fn choose(&mut self, pick: Pick) {
        self.chosen = Some(pick);
        self.text = self.text_for(pick);
        self.follow(pick);
    }

    /// Chooses the True Color tab's colour by hue, saturation and luminance.
    fn choose_hsl(&mut self, hsl: Hsl) {
        let (r, g, b) = hsl.to_rgb();
        let pick = Pick::Color(Color::Rgb(r, g, b));
        self.chosen = Some(pick);
        self.text = self.text_for(pick);
        self.hsl = hsl;
    }

    fn choose_rgb(&mut self, (r, g, b): (u8, u8, u8)) {
        self.choose(Pick::Color(Color::Rgb(r, g, b)));
    }

    /// Keeps the True Color tab on a colour chosen some other way.
    fn follow(&mut self, pick: Pick) {
        if let Pick::Color(color) = pick {
            if Hsl::from_rgb(rgb_of(color)).to_rgb() != self.hsl.to_rgb() {
                self.hsl = Hsl::from_rgb(rgb_of(color));
            }
        }
    }

    /// The chosen colour's RGB, or the True Color tab's when none is.
    fn rgb(&self) -> (u8, u8, u8) {
        match self.chosen {
            Some(Pick::Color(color)) => rgb_of(color),
            _ => self.hsl.to_rgb(),
        }
    }

    /// The Color field's text: the True Color tab gives every colour as
    /// R,G,B, the Index Color tab its name or number.
    fn text_for(&self, pick: Pick) -> String {
        match pick {
            Pick::Color(color) if self.tab == TRUE_TAB => {
                let (r, g, b) = rgb_of(color);
                format!("{r},{g},{b}")
            }
            _ => label(pick),
        }
    }
}

/// White stands for colour 7 where a colour has to be RGB.
fn rgb_of(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Foreground => (255, 255, 255),
        Color::Rgb(r, g, b) => (r, g, b),
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
/// presses OK (or double-clicks an index colour). Closes itself on OK and
/// Cancel. The ByLayer and ByBlock buttons are there only when `by_layer`.
pub fn show(ui: &Ui, theme: &Theme, id: Id, by_layer: bool) -> Option<Pick> {
    let mut state = ui.data(|d| d.get_temp::<State>(id))?;
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    let width = index_tab::WIDTH + MARGINS;
    widgets::dialog(ui.ctx(), theme, "Select Color", &mut open, width, |ui| {
        let mut tab = state.tab;
        ok |= widgets::tabbed_page(ui, theme, &mut tab, &TABS, PAGE_HEIGHT, |ui, tab| {
            if tab == TRUE_TAB {
                true_tab::show(ui, theme, &mut state);
                false
            } else {
                index_tab::show(ui, theme, &mut state)
            }
        });
        if tab != state.tab {
            state.tab = tab;
            if let Some(pick) = state.chosen {
                state.text = state.text_for(pick);
            }
        }
        ui.add_space(10.0);
        ok |= colour_row(ui, theme, &mut state, by_layer);
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

/// The Color field, the chosen colour's swatch, and ByLayer and ByBlock for
/// an object's colour; returns whether Enter was pressed on a colour the
/// field reads.
fn colour_row(ui: &mut Ui, theme: &Theme, state: &mut State, by_layer: bool) -> bool {
    let mut entered = false;
    ui.horizontal(|ui| {
        widgets::field_label(ui, theme, "Color:");
        let typed = ui.add(egui::TextEdit::singleline(&mut state.text).desired_width(140.0));
        if typed.changed() {
            state.chosen = names::parse(&state.text, by_layer);
            if let Some(pick) = state.chosen {
                state.follow(pick);
            }
        }
        if typed.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            entered = state.chosen.is_some();
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
                        state.choose(pick);
                    }
                }
            });
        }
    });
    entered
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
    use super::super::names::from_index;
    use super::*;

    #[test]
    fn a_new_dialog_shows_the_current_colour() {
        let state = State::new(Some(Pick::Color(from_index(34))));
        assert_eq!(state.text, "34");
        assert_eq!(state.tab, INDEX_TAB);
        assert_eq!(State::new(Some(Pick::ByLayer)).text, "ByLayer");
        assert_eq!(State::new(None).text, "");
    }

    #[test]
    fn a_true_colour_opens_on_the_true_color_tab_as_r_g_b() {
        let state = State::new(Some(Pick::Color(Color::Rgb(10, 20, 30))));
        assert_eq!(state.tab, TRUE_TAB);
        assert_eq!(state.text, "10,20,30");
        assert_eq!(state.hsl.to_rgb(), (10, 20, 30));
    }

    #[test]
    fn a_hue_outlives_a_grey() {
        let mut state = State::new(None);
        let teal = Hsl {
            hue: 165.0,
            saturation: 0.0,
            luminance: 0.5,
        };
        state.choose_hsl(teal);
        assert_eq!(state.chosen, Some(Pick::Color(Color::Rgb(128, 128, 128))));
        // Choosing the same grey again (as the field would) keeps the hue.
        state.choose(Pick::Color(Color::Rgb(128, 128, 128)));
        assert_eq!(state.hsl.hue, 165.0);
        // A different colour moves the True Color tab to it.
        state.choose_rgb((255, 0, 0));
        assert_eq!(state.hsl.hue, 0.0);
        assert_eq!(state.hsl.saturation, 1.0);
    }
}
