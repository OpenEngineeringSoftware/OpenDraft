//! The views ZOOM Previous goes back to: one list for the model tab and one
//! for each layout's sheet, empty when the drawing opens.
//!
//! Wheel notches and drags come in small moves, so a run of moves with less
//! than `SETTLE_SECONDS` between them is one step; a zoom command (Extents,
//! In, Out, Window) always starts a step of its own. Switching tabs is not a
//! step.

use std::collections::HashMap;

use opendraft_core::LayoutId;

use super::{Space, View};
use crate::Camera;

/// A change after this long without one starts a new step.
pub const SETTLE_SECONDS: f64 = 1.0;
/// How many views each tab remembers, as in AutoCAD.
const KEPT: usize = 10;

#[derive(Default)]
pub(super) struct Previous {
    /// Past views of each tab, latest last; `None` is the model tab.
    tabs: HashMap<Option<LayoutId>, Vec<Camera>>,
    /// The tab and its view as last seen.
    seen: Option<(Option<LayoutId>, Camera)>,
    /// When the view last changed; none starts a new step at the next change.
    changed_at: Option<f64>,
}

impl Previous {
    /// Note the view at the end of a frame at time `now` (seconds).
    pub(super) fn observe(&mut self, tab: Option<LayoutId>, camera: Camera, now: f64) {
        match self.seen {
            Some((seen_tab, before)) if seen_tab == tab => {
                if before == camera {
                    return;
                }
                let new_step = self.changed_at.is_none_or(|at| now - at >= SETTLE_SECONDS);
                if new_step {
                    let past = self.tabs.entry(tab).or_default();
                    past.push(before);
                    if past.len() > KEPT {
                        past.remove(0);
                    }
                }
                self.changed_at = Some(now);
            }
            // A new tab, or the first frame: nothing to go back to yet.
            _ => self.changed_at = None,
        }
        self.seen = Some((tab, camera));
    }

    /// The next change starts a step of its own.
    pub(super) fn new_step(&mut self) {
        self.changed_at = None;
    }

    /// Take the latest past view of `tab`, which becomes the one seen.
    pub(super) fn back(&mut self, tab: Option<LayoutId>) -> Option<Camera> {
        let camera = self.tabs.get_mut(&tab)?.pop()?;
        self.seen = Some((tab, camera));
        self.changed_at = None;
        Some(camera)
    }
}

impl View {
    /// The camera the history follows in the current tab: the model's, or
    /// the sheet's in a layout (also while working in one of its viewports).
    fn tab_camera(&self) -> (Option<LayoutId>, Camera) {
        match self.space {
            Space::Model => (None, self.camera),
            space => (space.layout(), self.sheet),
        }
    }

    pub(super) fn note_view(&mut self, now: f64) {
        let (tab, camera) = self.tab_camera();
        self.previous.observe(tab, camera, now);
    }

    /// ZOOM Previous: go back to the view before the last change in this
    /// tab. False when there is none.
    pub fn zoom_previous(&mut self) -> bool {
        let tab = self.tab_camera().0;
        let Some(camera) = self.previous.back(tab) else {
            return false;
        };
        match self.space {
            Space::Model => self.camera = camera,
            _ => {
                self.sheet = camera;
                self.camera = camera;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendraft_core::Point;

    fn at(zoom: f64) -> Camera {
        Camera {
            center: Point::new(0.0, 0.0),
            zoom,
        }
    }

    #[test]
    fn the_first_view_and_a_still_view_are_not_steps() {
        let mut p = Previous::default();
        p.observe(None, at(1.0), 0.0);
        p.observe(None, at(1.0), 5.0);
        assert_eq!(p.back(None), None);
    }

    #[test]
    fn moves_close_together_are_one_step() {
        let mut p = Previous::default();
        p.observe(None, at(1.0), 0.0);
        // Five wheel notches, 0.2 s apart.
        for (i, t) in [10.0, 10.2, 10.4, 10.6, 10.8].iter().enumerate() {
            p.observe(None, at(2.0 + i as f64), *t);
        }
        assert_eq!(p.back(None), Some(at(1.0)));
        assert_eq!(p.back(None), None);
    }

    #[test]
    fn a_pause_of_a_second_starts_a_new_step() {
        let mut p = Previous::default();
        p.observe(None, at(1.0), 0.0);
        p.observe(None, at(2.0), 10.0);
        p.observe(None, at(3.0), 10.5);
        p.observe(None, at(4.0), 11.5);
        assert_eq!(p.back(None), Some(at(3.0)));
        assert_eq!(p.back(None), Some(at(1.0)));
    }

    #[test]
    fn a_zoom_command_is_a_step_of_its_own() {
        let mut p = Previous::default();
        p.observe(None, at(1.0), 0.0);
        p.observe(None, at(2.0), 10.0);
        p.new_step();
        p.observe(None, at(3.0), 10.1);
        assert_eq!(p.back(None), Some(at(2.0)));
    }

    #[test]
    fn going_back_twice_goes_further_back() {
        let mut p = Previous::default();
        p.observe(None, at(1.0), 0.0);
        p.observe(None, at(2.0), 10.0);
        p.observe(None, at(3.0), 20.0);
        let back = p.back(None);
        assert_eq!(back, Some(at(2.0)));
        // The view put back is not itself a change.
        p.observe(None, at(2.0), 20.1);
        assert_eq!(p.back(None), Some(at(1.0)));
    }

    #[test]
    fn each_tab_keeps_its_own_views_and_switching_is_not_a_step() {
        let mut p = Previous::default();
        let layout = Some(LayoutId(1));
        p.observe(None, at(1.0), 0.0);
        p.observe(None, at(2.0), 10.0);
        p.observe(layout, at(50.0), 11.0);
        p.observe(layout, at(60.0), 20.0);
        p.observe(None, at(2.0), 21.0);
        assert_eq!(p.back(layout), Some(at(50.0)));
        assert_eq!(p.back(layout), None);
        assert_eq!(p.back(None), Some(at(1.0)));
    }

    #[test]
    fn the_view_goes_back_in_the_model_and_on_a_sheet() {
        let mut view = View::default();
        assert!(!view.zoom_previous(), "nothing when the drawing opens");
        view.camera = at(1.0);
        view.note_view(0.0);
        view.camera = at(4.0);
        view.note_view(5.0);
        assert!(view.zoom_previous());
        assert_eq!(view.camera, at(1.0));

        view.set_space(Space::Paper(LayoutId(1)));
        view.sheet = at(7.0);
        view.note_view(6.0);
        view.sheet = at(9.0);
        view.note_view(8.0);
        assert!(view.zoom_previous());
        assert_eq!(view.sheet, at(7.0));
        assert!(!view.zoom_previous(), "the model's views stay in the model");
    }

    #[test]
    fn only_the_last_ten_views_are_kept() {
        let mut p = Previous::default();
        for i in 0..15 {
            p.observe(None, at(f64::from(i + 1)), f64::from(i) * 10.0);
        }
        let mut count = 0;
        while p.back(None).is_some() {
            count += 1;
        }
        assert_eq!(count, KEPT);
    }
}
