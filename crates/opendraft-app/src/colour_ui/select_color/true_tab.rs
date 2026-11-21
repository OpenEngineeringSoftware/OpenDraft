//! The True Color tab: any colour, by hue, saturation and luminance (a hue
//! field beside a luminance bar) or by red, green and blue (three bars), as
//! AutoCAD's two colour models give it.

use eframe::egui::{self, Color32, Sense, Stroke, Ui, Vec2};
use opendraft_core::hsl::Hsl;
use opendraft_ui::{menu, widgets, Theme};

use super::{shading, State};

const FIELD: Vec2 = egui::vec2(250.0, 160.0);
const BAR_WIDTH: f32 = 18.0;
const MODELS: [&str; 2] = ["HSL", "RGB"];

/// How the tab offers the colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Model {
    #[default]
    Hsl,
    Rgb,
}

pub fn show(ui: &mut Ui, theme: &Theme, state: &mut State) {
    ui.horizontal(|ui| {
        widgets::field_label(ui, theme, "Color model:");
        let current = usize::from(state.model == Model::Rgb);
        let entries = MODELS.map(str::to_owned);
        let size = egui::vec2(90.0, theme.control_height);
        let id = ui.id().with("colour_model");
        if let Some(k) = menu::combo(
            ui,
            theme,
            id,
            size,
            MODELS[current],
            &entries,
            Some(current),
        ) {
            state.model = if k == 1 { Model::Rgb } else { Model::Hsl };
        }
    });
    ui.add_space(8.0);
    match state.model {
        Model::Hsl => hsl_page(ui, theme, state),
        Model::Rgb => rgb_page(ui, theme, state),
    }
}

fn rgb32((r, g, b): (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The hue field (hue left to right, saturation full at the top), the
/// luminance bar (white at the top, black at the bottom), and the three
/// numbers.
fn hsl_page(ui: &mut Ui, theme: &Theme, state: &mut State) {
    let mut hsl = state.hsl;
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, field) = ui.allocate_exact_size(FIELD, Sense::click_and_drag());
        shading::ramp(ui.painter(), rect, &hue_field());
        if let Some(at) = shading::pressed_at(&field) {
            hsl.hue = f64::from(at.x) * 360.0;
            hsl.saturation = 1.0 - f64::from(at.y);
            changed = true;
        }
        let spot = rect.lerp_inside(egui::vec2(
            (hsl.hue / 360.0) as f32,
            1.0 - hsl.saturation as f32,
        ));
        shading::ring(&ui.painter().with_clip_rect(rect.expand(6.0)), spot);

        ui.add_space(12.0);
        let size = egui::vec2(BAR_WIDTH, FIELD.y);
        let (bar_rect, bar) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let middle = rgb32(
            Hsl {
                luminance: 0.5,
                ..hsl
            }
            .to_rgb(),
        );
        let stops = [Color32::WHITE, middle, Color32::BLACK].map(|c| vec![c, c]);
        shading::ramp(ui.painter(), bar_rect, &stops);
        if let Some(at) = shading::pressed_at(&bar) {
            hsl.luminance = 1.0 - f64::from(at.y);
            changed = true;
        }
        shading::notch(ui.painter(), bar_rect, 1.0 - hsl.luminance as f32, true);

        ui.add_space(20.0);
        ui.vertical(|ui| {
            changed |= spinner(ui, theme, "Hue:", &mut hsl.hue, 359.0, 1.0);
            changed |= spinner(ui, theme, "Saturation:", &mut hsl.saturation, 100.0, 100.0);
            changed |= spinner(ui, theme, "Luminance:", &mut hsl.luminance, 100.0, 100.0);
        });
    });
    if changed {
        state.choose_hsl(hsl);
    }
    ui.add_space(8.0);
    let (r, g, b) = state.rgb();
    ui.label(format!("True color stored as RGB: {r},{g},{b}"));
}

/// The hue field's colours, sampled finely enough that blending between
/// them shows no seams: hue left to right, saturation full at the top, all
/// at luminance 0.5.
fn hue_field() -> Vec<Vec<Color32>> {
    const COLUMNS: u32 = 48;
    const ROWS: u32 = 16;
    (0..=ROWS)
        .map(|r| {
            (0..=COLUMNS)
                .map(|c| {
                    let hsl = Hsl {
                        hue: f64::from(c) * 360.0 / f64::from(COLUMNS),
                        saturation: 1.0 - f64::from(r) / f64::from(ROWS),
                        luminance: 0.5,
                    };
                    rgb32(hsl.to_rgb())
                })
                .collect()
        })
        .collect()
}

/// A labelled number shown as `value * scale`, from 0 to `max`; returns
/// whether it was changed.
fn spinner(ui: &mut Ui, theme: &Theme, label: &str, value: &mut f64, max: f64, scale: f64) -> bool {
    widgets::field_label(ui, theme, label);
    let mut shown = *value * scale;
    let response = ui.add(
        egui::DragValue::new(&mut shown)
            .range(0.0..=max)
            .speed(1.0)
            .fixed_decimals(0),
    );
    ui.add_space(4.0);
    if response.changed() {
        *value = shown / scale;
    }
    response.changed()
}

/// A bar for each of red, green and blue, running from none to full with
/// the other two as they are, and its number.
fn rgb_page(ui: &mut Ui, theme: &Theme, state: &mut State) {
    let (r, g, b) = state.rgb();
    let mut rgb = [r, g, b];
    let mut changed = false;
    for (k, name) in ["Red:", "Green:", "Blue:"].into_iter().enumerate() {
        ui.horizontal(|ui| {
            ui.add_sized(egui::vec2(48.0, 20.0), egui::Label::new(name));
            let width = ui.available_width() - 64.0;
            let size = egui::vec2(width, 18.0);
            let (rect, bar) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let with = |v: u8| {
                let mut c = rgb;
                c[k] = v;
                Color32::from_rgb(c[0], c[1], c[2])
            };
            let row = vec![with(0), with(255)];
            shading::ramp(ui.painter(), rect, &[row.clone(), row]);
            ui.painter().rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0_f32, theme.colors.border),
                egui::StrokeKind::Outside,
            );
            if let Some(at) = shading::pressed_at(&bar) {
                rgb[k] = (at.x * 255.0).round() as u8;
                changed = true;
            }
            shading::notch(ui.painter(), rect, f32::from(rgb[k]) / 255.0, false);
            ui.add_space(8.0);
            changed |= ui
                .add(egui::DragValue::new(&mut rgb[k]).range(0..=255))
                .changed();
        });
        ui.add_space(10.0);
    }
    if changed {
        state.choose_rgb((rgb[0], rgb[1], rgb[2]));
    }
}
