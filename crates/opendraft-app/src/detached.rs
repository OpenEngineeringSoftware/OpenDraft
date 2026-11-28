//! Panels shown in operating system windows of their own, which can be moved
//! onto another monitor. They are not in the dock's state (egui_dock cannot
//! draw one surface alone), so this list is saved beside the dock layout:
//! without it the 30-second autosave would lose a panel that is only
//! detached.

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::dock::PanelTab;

const KEY: &str = "detached_panels_v1";

/// The size a panel's window opens at when it has none yet.
pub const DEFAULT_SIZE: [f32; 2] = [380.0, 520.0];

/// One detached panel and where its window was last, in logical pixels of
/// the screen. `at` is `None` until the window has a position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub panel: PanelTab,
    pub at: Option<[f32; 2]>,
    pub size: [f32; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Detached(
    Vec<Window>,
    #[serde(skip)] Starts,
    #[serde(skip)] Vec<egui::Event>,
);

/// Where each window was when it was first shown this session. A viewport's
/// builder must not change while it is open (egui would move the window
/// back), so it is built from this, not from the live `at`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Starts(std::collections::HashMap<PanelTab, (Option<[f32; 2]>, [f32; 2])>);

impl Starts {
    fn of(&mut self, w: &Window) -> (Option<[f32; 2]>, [f32; 2]) {
        *self.0.entry(w.panel).or_insert((w.at, w.size))
    }
}

impl Detached {
    pub fn contains(&self, panel: PanelTab) -> bool {
        self.0.iter().any(|w| w.panel == panel)
    }

    pub fn windows(&self) -> &[Window] {
        &self.0
    }

    /// Show `panel` in a window of its own. The drawing view never detaches;
    /// a panel already detached keeps its window.
    pub fn add(&mut self, panel: PanelTab, at: Option<[f32; 2]>, size: [f32; 2]) {
        if panel.closeable() && !self.contains(panel) {
            self.0.push(Window { panel, at, size });
        }
    }

    pub fn remove(&mut self, panel: PanelTab) {
        self.0.retain(|w| w.panel != panel);
        self.1 .0.remove(&panel);
    }

    /// Where the window of `w` opens: where it was when first shown this
    /// session, which stays the same however it is moved after.
    pub fn start(&mut self, w: &Window) -> (Option<[f32; 2]>, [f32; 2]) {
        self.1.of(w)
    }

    /// Remove every window, returning their panels.
    pub fn take_all(&mut self) -> Vec<PanelTab> {
        self.1 = Starts::default();
        self.0.drain(..).map(|w| w.panel).collect()
    }

    /// Remember where the window of `panel` is now.
    pub fn moved(&mut self, panel: PanelTab, at: [f32; 2], size: [f32; 2]) {
        if let Some(w) = self.0.iter_mut().find(|w| w.panel == panel) {
            w.at = Some(at);
            w.size = size;
        }
    }

    /// Keys pressed in a detached window that none of its widgets used: the
    /// main window's shortcuts and command line get them next frame.
    pub fn forward(&mut self, mut events: Vec<egui::Event>) {
        self.2.append(&mut events);
    }

    pub fn take_forwarded(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.2)
    }

    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let saved: Option<Self> = storage
            .and_then(|s| s.get_string(KEY))
            .and_then(|text| ron::from_str(&text).ok());
        let mut found = Self::default();
        // A saved list is trusted for its content only: no repeats, no
        // drawing view, no window of no size.
        for w in saved.into_iter().flat_map(|d| d.0) {
            if w.size.iter().all(|s| s.is_finite() && *s >= 50.0) {
                found.add(
                    w.panel,
                    w.at.filter(|p| p.iter().all(|v| v.is_finite())),
                    w.size,
                );
            }
        }
        found
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(text) = ron::to_string(self) {
            storage.set_string(KEY, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawing_view_and_repeats_are_not_added() {
        let mut d = Detached::default();
        d.add(PanelTab::Canvas, None, DEFAULT_SIZE);
        d.add(PanelTab::Layers, None, DEFAULT_SIZE);
        d.add(PanelTab::Layers, Some([1.0, 2.0]), [100.0, 100.0]);
        assert_eq!(d.windows().len(), 1);
        assert_eq!(d.windows()[0].at, None);
    }

    #[test]
    fn a_moved_window_is_remembered_and_a_removed_one_is_gone() {
        let mut d = Detached::default();
        d.add(PanelTab::Layers, None, DEFAULT_SIZE);
        d.moved(PanelTab::Layers, [10.0, 20.0], [300.0, 400.0]);
        assert_eq!(d.windows()[0].at, Some([10.0, 20.0]));
        assert_eq!(d.windows()[0].size, [300.0, 400.0]);
        d.remove(PanelTab::Layers);
        assert!(d.windows().is_empty());
    }

    #[test]
    fn the_list_round_trips_through_text() {
        let mut d = Detached::default();
        d.add(PanelTab::Properties, Some([-1900.0, 40.0]), [320.0, 600.0]);
        let text = ron::to_string(&d).unwrap();
        assert_eq!(ron::from_str::<Detached>(&text).unwrap(), d);
    }

    #[test]
    fn forwarded_keys_are_handed_over_once() {
        let mut d = Detached::default();
        d.forward(vec![egui::Event::Text("l".into())]);
        d.forward(vec![egui::Event::Copy]);
        assert_eq!(d.take_forwarded().len(), 2);
        assert!(d.take_forwarded().is_empty());
    }
}
