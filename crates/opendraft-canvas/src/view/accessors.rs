//! The small setters and readers of a view's state: prompts, constraints,
//! the last point, picked grips, and the zoom requests.

use std::sync::Arc;

use egui::{Pos2, Rect};
use opendraft_core::{EntityId, Point};

use super::{PointPrompt, Request, View, STEP_ZOOM};
use crate::layer_cache::CacheConfig;

impl View {
    /// UI overlays can constrain previews and keep their clicks out of drawing input.
    pub fn set_cursor_constraint(
        &mut self,
        constraint: Option<Arc<dyn Fn(Point) -> Point + Send + Sync>>,
    ) {
        self.constraint = constraint;
    }

    pub fn set_input_exclusions(&mut self, rects: Vec<Rect>) {
        self.input_exclusions = rects;
    }

    /// Let the wheel zoom where clicks are excluded (an in-place editor
    /// covers the drawing but the view should stay navigable).
    /// Leave `id` out of the drawing (its in-place editor draws it instead).
    pub fn set_hidden_entity(&mut self, id: Option<EntityId>) {
        self.hidden = id;
    }

    pub fn set_wheel_through_exclusions(&mut self, through: bool) {
        self.wheel_through = through;
    }

    /// Whether a dialog, menu or other window drawn above the drawing is at
    /// `p`: the pointer there belongs to it, so there is no cursor, hover or
    /// wheel on the canvas.
    pub(super) fn covered(ctx: &egui::Context, p: Pos2) -> bool {
        ctx.layer_id_at(p)
            .is_some_and(|l| l.order != egui::Order::Background)
    }

    /// With an editor over the drawing, its own area takes the drag from
    /// the canvas, so a middle drag that began on the drawing or the editor
    /// (not a toolbar or dialog above it) pans from the raw pointer.
    pub(super) fn pan_through_editor(&mut self, ui: &egui::Ui, rect: Rect) {
        if !self.wheel_through {
            return;
        }
        let ctx = ui.ctx();
        let (origin, delta) = ui.input(|i| {
            let origin = i.pointer.middle_down().then(|| i.pointer.press_origin());
            (origin.flatten(), i.pointer.delta())
        });
        let over_drawing = |p: Pos2| {
            rect.contains(p)
                && ctx
                    .layer_id_at(p)
                    .is_none_or(|l| l.order == egui::Order::Background)
        };
        if origin.is_some_and(over_drawing) {
            self.camera.pan_by_pixels(delta);
        }
    }
    /// Make the canvas collect a point for a running command (`Some`), or
    /// stop doing so (`None`).
    pub fn set_point_prompt(&mut self, prompt: Option<PointPrompt>) {
        self.point_prompt = prompt;
    }

    /// Whether a running command is waiting for a point.
    pub fn is_prompting(&self) -> bool {
        self.point_prompt.is_some()
    }

    /// Tell the canvas whether a running command waits for objects to be
    /// selected ("Select objects"), so the cursor is the pickbox alone.
    pub fn set_selecting_objects(&mut self, selecting: bool) {
        self.selecting_objects = selecting;
    }

    /// Whether a running command waits for objects to be selected.
    pub fn is_selecting_objects(&self) -> bool {
        self.selecting_objects
    }

    pub fn last_point(&self) -> Option<Point> {
        self.last_point
    }

    /// Remember a point given by typed entry, as the origin for relative
    /// coordinates typed next.
    pub fn set_last_point(&mut self, at: Point) {
        self.last_point = Some(at);
    }

    /// Frame the whole drawing on the next frame.
    /// Whether any grips are picked and waiting.
    pub fn has_warm_grips(&self) -> bool {
        !self.warm.is_empty()
    }

    /// Forget the picked grips (Esc).
    pub fn clear_warm_grips(&mut self) {
        self.warm.clear();
    }

    /// Forget what these hatches drew, so they are worked out again. The
    /// cache also checks its entries against the hatch, so this only frees
    /// memory early.
    pub fn invalidate_hatches(&self, ids: impl IntoIterator<Item = EntityId>) {
        self.hatch_cache.borrow_mut().invalidate(ids);
    }

    /// Say what the drawing is now (`Session::draw_token`): the kept picture
    /// is reused while it is the same. Without one nothing is kept.
    pub fn set_document_token(&mut self, token: Option<(u64, u64)>) {
        self.document_token = token;
    }

    /// The drawing `show` is given, shared, so that its picture can be made
    /// on another thread (see `builder.rs`). Without it, it is made here.
    pub fn set_shared_document(&mut self, document: std::sync::Weak<opendraft_core::Document>) {
        self.shared_document = document;
    }

    /// How the kept picture behaves (the Performance settings).
    pub fn set_cache_config(&mut self, config: CacheConfig) {
        if config != self.cache_config {
            self.layer_cache.get_mut().clear();
        }
        self.cache_config = config;
    }

    /// What the kept picture has done so far.
    pub fn cache_stats(&self) -> crate::layer_cache::CacheStats {
        self.layer_cache.borrow().stats()
    }

    /// Forget the kept picture, so the next frame builds it again (REGEN).
    pub fn clear_layer_cache(&self) {
        self.layer_cache.borrow_mut().clear();
    }

    pub fn request_zoom_extents(&mut self) {
        self.request(Request::Extents);
    }

    /// Zoom about the viewport center on the next frame.
    pub fn request_zoom_step(&mut self, zoom_in: bool) {
        let factor = if zoom_in { STEP_ZOOM } else { 1.0 / STEP_ZOOM };
        self.request(Request::Zoom(factor));
    }

    /// A zoom for the next frame; it is a step of its own for ZOOM Previous.
    fn request(&mut self, request: Request) {
        self.previous.new_step();
        self.request = Some(request);
    }
}
