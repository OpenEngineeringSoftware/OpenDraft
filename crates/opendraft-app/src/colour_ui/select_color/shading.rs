//! Colour ramps for the True Color tab, drawn as one mesh blended between
//! colours set at evenly spaced points (the hue field, the luminance bar and
//! the red, green and blue bars), and the markers drawn over them.

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke};

/// Fills `rect` blending between `rows` of colours: the rows evenly spaced
/// top to bottom, each row's colours evenly spaced left to right. Every row
/// needs the same number of colours, at least two, and there must be two
/// rows or more.
pub fn ramp(painter: &Painter, rect: Rect, rows: &[Vec<Color32>]) {
    let columns = rows.first().map_or(0, Vec::len);
    if rows.len() < 2 || columns < 2 || rows.iter().any(|row| row.len() != columns) {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let step = |n: usize, of: usize| n as f32 / (of - 1) as f32;
    for (r, row) in rows.iter().enumerate() {
        for (c, &color) in row.iter().enumerate() {
            let at = rect.lerp_inside(egui::vec2(step(c, columns), step(r, rows.len())));
            mesh.colored_vertex(at, color);
        }
    }
    let index = |r: usize, c: usize| (r * columns + c) as u32;
    for r in 0..rows.len() - 1 {
        for c in 0..columns - 1 {
            mesh.add_triangle(index(r, c), index(r, c + 1), index(r + 1, c));
            mesh.add_triangle(index(r, c + 1), index(r + 1, c + 1), index(r + 1, c));
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A ring at `at`, black outside and white inside, to read on any colour.
pub fn ring(painter: &Painter, at: Pos2) {
    painter.circle_stroke(at, 5.0, Stroke::new(3.0_f32, Color32::BLACK));
    painter.circle_stroke(at, 5.0, Stroke::new(1.0_f32, Color32::WHITE));
}

/// A line across a bar at `along` (0 to 1 from the start), black-edged white.
pub fn notch(painter: &Painter, rect: Rect, along: f32, vertical: bool) {
    let (a, b) = if vertical {
        let y = egui::lerp(rect.y_range(), along);
        (
            egui::pos2(rect.left() - 3.0, y),
            egui::pos2(rect.right() + 3.0, y),
        )
    } else {
        let x = egui::lerp(rect.x_range(), along);
        (
            egui::pos2(x, rect.top() - 3.0),
            egui::pos2(x, rect.bottom() + 3.0),
        )
    };
    painter.line_segment([a, b], Stroke::new(4.0_f32, Color32::BLACK));
    painter.line_segment([a, b], Stroke::new(2.0_f32, Color32::WHITE));
}

/// Where the pointer is pressed or dragged on `response`, as fractions of
/// its rectangle from the top left (each 0 to 1). A click counts too, for a
/// press and release that fall in one frame.
pub fn pressed_at(response: &egui::Response) -> Option<egui::Vec2> {
    if !(response.is_pointer_button_down_on() || response.clicked()) {
        return None;
    }
    let p = response.interact_pointer_pos()?;
    let rect = response.rect;
    Some(egui::vec2(
        ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    ))
}
