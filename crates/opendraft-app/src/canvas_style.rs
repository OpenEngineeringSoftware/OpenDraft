//! The colours the canvas paints with, from the theme and where the user is.
//!
//! Model space is drawn on the palette's background. A layout's sheet is
//! light paper with black lines unless the user chose dark paper
//! ([`LayoutPaper`]), as AutoCAD shows layouts: a preview of the printed
//! sheet. The paper is white unless the user chose another light preset
//! ([`PaperColour`]). The model seen through the sheet's viewports is on that
//! paper too. The area around the sheet stays dark either way.
//!
//! On light paper the marks drawn over the drawing change with it, each
//! readable on any paper offered (down to `paper_colour::FLOOR`): the
//! selection is the accent itself (not lightened), the snap marker and its
//! label a dark amber, the crossing box a darker green, the grid dark, and the
//! crosshair dark over the sheet unless the user chose its colour (off the
//! sheet the canvas keeps it off-white).

use eframe::egui::Color32;
use opendraft_canvas::{CanvasStyle, Crosshair, Space};
use opendraft_commands::Command;
use opendraft_ui::Colors;

use crate::paper_colour::PaperColour;
use crate::settings::LayoutPaper;
use crate::state::AppState;
use opendraft_ui::key;

const AXIS_X: Color32 = Color32::from_rgb(70, 190, 110);
const AXIS_Y: Color32 = Color32::from_rgb(220, 80, 80);
const SNAP: Color32 = Color32::from_rgb(255, 214, 90);
const CROSSING: Color32 = Color32::from_rgb(70, 190, 110);

/// The colour of lines on light paper, and the marks over it.
const INK: Color32 = Color32::BLACK;
const SNAP_ON_PAPER: Color32 = Color32::from_rgb(140, 78, 0);
const CROSSING_ON_PAPER: Color32 = Color32::from_rgb(30, 135, 70);
const CROSSHAIR_ON_PAPER: Color32 = Color32::from_gray(40);

/// LAYOUTPAPER, the Layout tab's Light Paper button: light paper on or off.
/// It is a setting of the user's, not of the drawing, so it works mid-command
/// and from the Home page.
pub fn commands() -> Vec<Command<AppState>> {
    vec![
        Command::new("view.layout_paper", "Light Paper", toggle_paper)
            .checked(|s| s.settings.layout_paper == LayoutPaper::Light)
            .transparent()
            .global(),
    ]
}

fn toggle_paper(s: &mut AppState) {
    let (paper, said) = match s.settings.layout_paper {
        LayoutPaper::Light => (LayoutPaper::Dark, key!("Layouts are shown on dark paper")),
        LayoutPaper::Dark => (LayoutPaper::Light, key!("Layouts are shown on light paper")),
    };
    s.settings.layout_paper = paper;
    s.message = said.to_owned();
}

/// Whether `space` is shown on light paper.
pub fn light_paper(space: Space, paper: LayoutPaper) -> bool {
    space.layout().is_some() && paper == LayoutPaper::Light
}

pub fn canvas_style(
    colors: &Colors,
    space: Space,
    paper: LayoutPaper,
    colour: PaperColour,
) -> CanvasStyle {
    if light_paper(space, paper) {
        light(colors, colour.color32())
    } else {
        dark(colors)
    }
}

fn dark(c: &Colors) -> CanvasStyle {
    CanvasStyle {
        background: c.canvas_bg,
        surround: c.canvas_bg.lerp_to_gamma(Color32::BLACK, 0.4),
        grid_minor: c.text.gamma_multiply(0.03),
        grid_major: c.text.gamma_multiply(0.06),
        axis_x: AXIS_X.gamma_multiply(0.3),
        axis_y: AXIS_Y.gamma_multiply(0.3),
        entity: c.text,
        hover: c.accent.gamma_multiply(0.25),
        selected: c.accent.lerp_to_gamma(Color32::WHITE, 0.35),
        selected_glow: c.accent.gamma_multiply(0.5),
        snap: SNAP,
        window: c.accent,
        crossing: CROSSING,
        crosshair: Crosshair::OFF_WHITE,
    }
}

fn light(c: &Colors, paper: Color32) -> CanvasStyle {
    CanvasStyle {
        background: paper,
        grid_minor: INK.gamma_multiply(0.05),
        grid_major: INK.gamma_multiply(0.1),
        axis_x: AXIS_X.gamma_multiply(0.6),
        axis_y: AXIS_Y.gamma_multiply(0.6),
        entity: INK,
        selected: c.accent,
        snap: SNAP_ON_PAPER,
        crossing: CROSSING_ON_PAPER,
        crosshair: CROSSHAIR_ON_PAPER,
        ..dark(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_colour;
    use opendraft_core::{LayoutId, ViewportId};

    const MODEL: Space = Space::Model;
    const SHEET: Space = Space::Paper(LayoutId(1));
    const THROUGH: Space = Space::Viewport(LayoutId(1), ViewportId(2));

    const WHITE: PaperColour = PaperColour::White;

    /// WCAG contrast ratio (as `opendraft-ui`'s theme contrast tests).
    fn contrast(a: Color32, b: Color32) -> f64 {
        let lum = |c: Color32| paper_colour::luminance(c);
        let (a, b) = (lum(a), lum(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn layouts_are_on_light_paper_unless_dark_is_chosen_and_the_model_never_is() {
        let grey = PaperColour::LightGrey;
        for c in [Colors::neutral(), Colors::slate()] {
            for paper in [LayoutPaper::Light, LayoutPaper::Dark] {
                assert_eq!(canvas_style(&c, MODEL, paper, grey).background, c.canvas_bg);
            }
            for space in [SHEET, THROUGH] {
                let light = canvas_style(&c, space, LayoutPaper::Light, WHITE);
                assert_eq!((light.background, light.entity), (Color32::WHITE, INK));
                let tinted = canvas_style(&c, space, LayoutPaper::Light, grey);
                assert_eq!(tinted.background, Color32::from_gray(0xEE));
                let dark = canvas_style(&c, space, LayoutPaper::Dark, grey);
                assert_eq!((dark.background, dark.entity), (c.canvas_bg, c.text));
                // Around the sheet it is dark on either paper.
                assert_eq!(light.surround, dark.surround);
                assert_eq!(light.crosshair, CROSSHAIR_ON_PAPER);
                assert_eq!(dark.crosshair, Crosshair::OFF_WHITE);
            }
        }
    }

    #[test]
    fn what_is_drawn_on_light_paper_can_be_seen_on_every_paper_offered() {
        for c in [Colors::neutral(), Colors::slate()] {
            for paper in PaperColour::ALL {
                let s = canvas_style(&c, SHEET, LayoutPaper::Light, paper);
                let marks = [
                    ("lines", s.entity),
                    ("selection", s.selected),
                    ("window box", s.window),
                    ("crossing box", s.crossing),
                    ("crosshair", s.crosshair),
                ];
                for (what, colour) in marks {
                    let ratio = contrast(colour, s.background);
                    assert!(ratio >= 3.0, "{paper:?} {what}: {ratio:.2}");
                }
                // The snap label is small text.
                let ratio = contrast(s.snap, s.background);
                assert!(ratio >= 4.5, "{paper:?} snap: {ratio:.2}");
            }
        }
    }

    #[test]
    fn layoutpaper_flips_the_paper_and_its_button_shows_light() {
        let registry = crate::commands::build_registry();
        let mut state = AppState::new();
        assert!(
            registry.is_checked("view.layout_paper", &state),
            "light to begin with"
        );
        registry.execute("view.layout_paper", &mut state).unwrap();
        assert_eq!(state.settings.layout_paper, LayoutPaper::Dark);
        assert!(!registry.is_checked("view.layout_paper", &state));
        assert_eq!(state.message, "Layouts are shown on dark paper");
        registry.execute("view.layout_paper", &mut state).unwrap();
        assert_eq!(state.settings.layout_paper, LayoutPaper::Light);
        assert!(registry.is_checked("view.layout_paper", &state));
    }
}
