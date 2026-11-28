//! Panels shown in operating system windows of their own, which can be moved
//! onto another monitor. They are not in the dock's state (egui_dock cannot
//! draw one surface alone), so this list is saved beside the dock layout:
//! without it the 30-second autosave would lose a panel that is only
//! detached.

use std::collections::HashMap;

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::dock::PanelTab;

const KEY: &str = "detached_panels_v1";

/// The size a panel's window opens at when it has none yet.
pub const DEFAULT_SIZE: [f32; 2] = [380.0, 520.0];

/// The least size of a panel's window.
pub const LEAST: [f32; 2] = [200.0, 120.0];

/// The least size a panel's window opens at when it comes out of the dock:
/// the dock's own window is small, and a window of the OS needs room for its
/// header and a readable panel.
pub const OPENS_AT_LEAST: [f32; 2] = [300.0, 260.0];

/// One detached panel and where its window was last, in logical pixels of
/// the screen. `at` is `None` until the window has a position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub panel: PanelTab,
    pub at: Option<[f32; 2]>,
    pub size: [f32; 2],
}

/// What is saved: the windows, as a newtype so the saved text keeps the shape
/// it has had since the list was first saved.
#[derive(Serialize, Deserialize)]
struct Saved(Vec<Window>);

/// The panels in windows of their own, and what the app keeps about them
/// that is not saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Detached {
    windows: Vec<Window>,
    starts: HashMap<PanelTab, (Option<[f32; 2]>, [f32; 2])>,
    /// Keys pressed in a window that none of its widgets used.
    keys: Vec<egui::Event>,
    /// "Bring windows back" was asked for.
    bring_back: bool,
}

impl Detached {
    pub fn contains(&self, panel: PanelTab) -> bool {
        self.windows.iter().any(|w| w.panel == panel)
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Show `panel` in a window of its own. The drawing view never detaches;
    /// a panel already detached keeps its window.
    pub fn add(&mut self, panel: PanelTab, at: Option<[f32; 2]>, size: [f32; 2]) {
        if panel.closeable() && !self.contains(panel) {
            self.windows.push(Window { panel, at, size });
        }
    }

    pub fn remove(&mut self, panel: PanelTab) {
        self.windows.retain(|w| w.panel != panel);
        self.starts.remove(&panel);
    }

    /// Where the window of `w` opens: where it was when first shown this
    /// session. A viewport's builder must not change while it is open (egui
    /// would move the window back), so it is built from this, not from the
    /// live `at`, which follows the window.
    pub fn start(&mut self, w: &Window) -> (Option<[f32; 2]>, [f32; 2]) {
        *self.starts.entry(w.panel).or_insert((w.at, w.size))
    }

    /// Remove every window, returning their panels.
    pub fn take_all(&mut self) -> Vec<PanelTab> {
        self.starts.clear();
        self.windows.drain(..).map(|w| w.panel).collect()
    }

    /// Remember where the window of `panel` is now.
    pub fn moved(&mut self, panel: PanelTab, at: [f32; 2], size: [f32; 2]) {
        if let Some(w) = self.windows.iter_mut().find(|w| w.panel == panel) {
            w.at = Some(at);
            w.size = size;
        }
    }

    /// Keys pressed in a detached window that none of its widgets used: the
    /// main window's shortcuts and command line get them next frame.
    pub fn forward(&mut self, mut events: Vec<egui::Event>) {
        self.keys.append(&mut events);
    }

    pub fn take_forwarded(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.keys)
    }

    /// Ask for every window to be brought back over the main window.
    pub fn ask_bring_back(&mut self) {
        self.bring_back = true;
    }

    /// Whether bringing back was asked for since the last call.
    pub fn take_bring_back(&mut self) -> bool {
        std::mem::take(&mut self.bring_back)
    }

    /// Put every window where it can be seen, one below and right of the
    /// last, over `main` (the main window's inside, in screen coordinates),
    /// and no bigger than that window. The windows move at once: their
    /// builders change, which egui turns into a move.
    pub fn bring_back(&mut self, main: egui::Rect) {
        for (n, w) in self.windows.iter_mut().enumerate() {
            let (at, size) = cascade(main, n, w.size);
            w.at = Some(at);
            w.size = size;
            self.starts.insert(w.panel, (Some(at), size));
        }
    }

    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let saved: Option<Saved> = storage
            .and_then(|s| s.get_string(KEY))
            .and_then(|text| ron::from_str(&text).ok());
        let mut found = Self::default();
        // A saved list is trusted for its content only: no repeats, no
        // drawing view, no window of no size.
        for w in saved.into_iter().flat_map(|s| s.0) {
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
        if let Ok(text) = ron::to_string(&Saved(self.windows.clone())) {
            storage.set_string(KEY, text);
        }
    }
}

/// Where the `index`th window goes when brought back over `main`: stepped
/// down and right from its top-left corner, and no bigger than the main
/// window, so its title and buttons can be reached.
fn cascade(main: egui::Rect, index: usize, size: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let room = main.size() * 0.9;
    let (w, h) = (
        size[0].clamp(LEAST[0], room.x.max(LEAST[0])),
        size[1].clamp(LEAST[1], room.y.max(LEAST[1])),
    );
    let step = (index % 6) as f32 * 32.0;
    let x = (main.left() + 48.0 + step).min(main.right() - w);
    let y = (main.top() + 72.0 + step).min(main.bottom() - h);
    ([x.max(main.left()), y.max(main.top())], [w, h])
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
    fn the_list_is_saved_in_the_shape_it_has_always_had() {
        let mut d = Detached::default();
        d.add(PanelTab::Properties, Some([-1900.0, 40.0]), [320.0, 600.0]);
        d.add(PanelTab::Layers, None, [300.0, 260.0]);
        let text = ron::to_string(&Saved(d.windows().to_vec())).unwrap();
        // As written before the list had named fields: a profile saved then
        // must still load.
        let old = "([(panel:Properties,at:Some((-1900.0,40.0)),size:(320.0,600.0)),\
                   (panel:Layers,at:None,size:(300.0,260.0))])";
        assert_eq!(text, old);
        let loaded: Saved = ron::from_str(old).unwrap();
        assert_eq!(loaded.0, d.windows());
    }

    #[test]
    fn forwarded_keys_are_handed_over_once() {
        let mut d = Detached::default();
        d.forward(vec![egui::Event::Text("l".into())]);
        d.forward(vec![egui::Event::Copy]);
        assert_eq!(d.take_forwarded().len(), 2);
        assert!(d.take_forwarded().is_empty());
    }

    fn main_window() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(1000.0, 700.0))
    }

    #[test]
    fn brought_back_windows_cascade_over_the_main_window() {
        let mut d = Detached::default();
        // One left on a monitor that is gone, one far too big.
        d.add(PanelTab::Layers, Some([-4000.0, 9000.0]), [300.0, 400.0]);
        d.add(PanelTab::Properties, Some([5000.0, 10.0]), [3000.0, 2000.0]);
        let first = d.start(&d.windows()[0].clone());
        assert_eq!(first.0, Some([-4000.0, 9000.0]));
        d.bring_back(main_window());
        let [layers, properties] = [&d.windows()[0], &d.windows()[1]];
        assert_eq!(layers.at, Some([148.0, 122.0]));
        assert_eq!(layers.size, [300.0, 400.0]);
        // Stepped to 154, but held up so its bottom edge is inside.
        assert_eq!(properties.at, Some([180.0, 120.0]));
        assert_eq!(properties.size, [900.0, 630.0]);
        // What the windows open at follows, so the move happens.
        assert_eq!(d.start(&d.windows()[0].clone()).0, Some([148.0, 122.0]));
    }

    #[test]
    fn a_brought_back_window_stays_inside_a_small_main_window() {
        let main = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let (at, size) = cascade(main, 5, [380.0, 520.0]);
        assert!(at[0] >= 0.0 && at[1] >= 0.0);
        assert!(at[0] + size[0] <= 400.0 && at[1] + size[1] <= 300.0);
    }

    #[test]
    fn bring_back_is_asked_once_and_taken_once() {
        let mut d = Detached::default();
        assert!(!d.take_bring_back());
        d.ask_bring_back();
        assert!(d.take_bring_back());
        assert!(!d.take_bring_back());
    }
}
