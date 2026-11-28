//! Screen-only markers for external references that draw nothing: unloaded
//! ones and ones whose source has never been resolved. They sit at each
//! insert's insertion point, never enter the layer cache and never plot.

use egui::{vec2, Align2, FontId, Stroke};
use opendraft_core::{Document, Entity};

use super::{CanvasStyle, Rect, View};

/// Half the cross's arm, and the circle's radius, in screen points.
const ARM: f32 = 5.0;

impl View {
    /// A cross and ring at every insert of an unloaded or empty reference,
    /// with its name when the view is zoomed enough to read it.
    pub(super) fn paint_xref_markers(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        style: &CanvasStyle,
        document: &Document,
    ) {
        let color = style.snap.gamma_multiply(0.75);
        let stroke = Stroke::new(1.5_f32, color);
        for xref in document.xrefs() {
            let Some(root) = document.block(xref.block) else {
                continue;
            };
            if !(xref.unloaded || root.entities.is_empty()) {
                continue;
            }
            for id in document.xref_references(xref.block) {
                let Some(object) = document.object(id) else {
                    continue;
                };
                let Entity::Insert(insert) = &object.entity else {
                    continue;
                };
                let at = self.camera.world_to_screen(insert.insertion, rect);
                if !rect.expand(ARM).contains(at) {
                    continue;
                }
                painter.line_segment([at + vec2(-ARM, -ARM), at + vec2(ARM, ARM)], stroke);
                painter.line_segment([at + vec2(-ARM, ARM), at + vec2(ARM, -ARM)], stroke);
                painter.circle_stroke(at, ARM + 2.0, stroke);
                if self.camera.zoom >= 1.0 {
                    painter.text(
                        at + vec2(ARM + 6.0, -ARM - 4.0),
                        Align2::LEFT_BOTTOM,
                        &root.name,
                        FontId::proportional(12.0),
                        color,
                    );
                }
            }
        }
    }
}
