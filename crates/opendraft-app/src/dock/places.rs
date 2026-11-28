//! Where each panel was last docked, so that showing a hidden panel puts it
//! back there. Places are kept by neighbour, not by position in the dock
//! tree, which changes whenever a panel comes or goes.

use std::collections::HashMap;

use eframe::egui;
use egui_dock::{DockState, Node, NodeIndex, SurfaceIndex, TabIndex};
use serde::{Deserialize, Serialize};

use super::PanelTab;

const PLACES_KEY: &str = "dock_places_v1";

/// The side of its neighbours a panel is docked on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    fn split(self) -> egui_dock::Split {
        match self {
            Side::Left => egui_dock::Split::Left,
            Side::Right => egui_dock::Split::Right,
            Side::Above => egui_dock::Split::Above,
            Side::Below => egui_dock::Split::Below,
        }
    }
}

/// One panel's place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Place {
    /// A tab among `beside`, the other tabs of its strip, at `index`.
    Tab { beside: Vec<PanelTab>, index: usize },
    /// One half of a split, on `side` of the panels in the other half.
    /// `share` is the first (left or top) half's.
    Split {
        beside: Vec<PanelTab>,
        side: Side,
        share: f32,
    },
    /// Alone in a floating window.
    Window { at: [f32; 2], size: [f32; 2] },
}

/// The last known place of every panel, open or hidden.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Places(HashMap<PanelTab, Place>);

/// The panels in the part of `tree` under `top`.
fn panels_under(tree: &egui_dock::Tree<PanelTab>, top: NodeIndex) -> Vec<PanelTab> {
    let mut found = Vec::new();
    for (i, node) in tree.iter().enumerate() {
        let under = std::iter::successors(Some(NodeIndex(i)), |n| n.parent()).any(|n| n == top);
        if let (true, Some(tabs)) = (under, node.tabs()) {
            found.extend_from_slice(tabs);
        }
    }
    found
}

/// The lowest node both `a` and `b` are under.
fn common(a: NodeIndex, b: NodeIndex) -> NodeIndex {
    let above_a: Vec<NodeIndex> = std::iter::successors(Some(a), |n| n.parent()).collect();
    std::iter::successors(Some(b), |n| n.parent())
        .find(|n| above_a.contains(n))
        .unwrap_or(NodeIndex::root())
}

/// Where the floating window of `surface` is, and the size of its inside.
/// The dock leaves that to egui, which keeps it under the window's id.
pub(super) fn window_rect(ctx: &egui::Context, surface: SurfaceIndex) -> Option<egui::Rect> {
    let id = egui::Id::from(format!("window {surface:?}"));
    let outside = ctx.memory(|m| m.area_rect(id))?;
    let mut style = (*ctx.style()).clone();
    super::drag::floating_frame(&mut style);
    let frame = egui::Frame::window(&style);
    let inside = outside.size() - frame.total_margin().sum();
    (outside.is_finite() && inside.min_elem() > 0.0)
        .then(|| egui::Rect::from_min_size(outside.min, inside))
}

fn place_of(
    dock: &DockState<PanelTab>,
    panel: PanelTab,
    ctx: Option<&egui::Context>,
) -> Option<Place> {
    let (surface, node, tab) = dock.find_tab(&panel)?;
    let tree = &dock[surface];
    let tabs = tree[node].tabs()?;
    if tabs.len() > 1 {
        let beside = tabs.iter().copied().filter(|t| *t != panel).collect();
        return Some(Place::Tab {
            beside,
            index: tab.0,
        });
    }
    let Some(parent) = node.parent() else {
        // The only panel of a floating window.
        let rect = window_rect(ctx?, surface)?;
        return Some(Place::Window {
            at: [rect.left(), rect.top()],
            size: [rect.width(), rect.height()],
        });
    };
    let first = node == parent.left();
    let (side, share) = match &tree[parent] {
        Node::Horizontal { fraction, .. } if first => (Side::Left, *fraction),
        Node::Horizontal { fraction, .. } => (Side::Right, *fraction),
        Node::Vertical { fraction, .. } if first => (Side::Above, *fraction),
        Node::Vertical { fraction, .. } => (Side::Below, *fraction),
        _ => return None,
    };
    let other = match first {
        true => parent.right(),
        false => parent.left(),
    };
    Some(Place::Split {
        beside: panels_under(tree, other),
        side,
        share,
    })
}

impl Places {
    /// Remember where every open panel is now. Called each frame, so the
    /// place is known however a panel gets closed. Floating windows are
    /// placed by `ctx`; without it theirs are left as last noted.
    pub fn note(&mut self, dock: &DockState<PanelTab>, ctx: Option<&egui::Context>) {
        let open: Vec<PanelTab> = dock.iter_all_tabs().map(|(_, tab)| *tab).collect();
        for panel in open.into_iter().filter(|p| p.closeable()) {
            if let Some(place) = place_of(dock, panel, ctx) {
                self.0.insert(panel, place);
            }
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Put a hidden panel back in its remembered place. False when it has
    /// none, or none of the panels it was beside is open any more.
    pub(super) fn restore(&self, dock: &mut DockState<PanelTab>, panel: PanelTab) -> bool {
        let open = |dock: &DockState<PanelTab>, beside: &[PanelTab]| -> Vec<_> {
            let found = beside.iter().filter_map(|p| dock.find_tab(p));
            found.map(|(surface, node, _)| (surface, node)).collect()
        };
        match self.0.get(&panel) {
            Some(Place::Tab { beside, index }) => {
                let Some(&(surface, node)) = open(dock, beside).first() else {
                    return false;
                };
                let strip = &mut dock[surface][node];
                strip.insert_tab(TabIndex((*index).min(strip.tabs_count())), panel);
                dock.set_focused_node_and_surface((surface, node));
            }
            Some(Place::Split {
                beside,
                side,
                share,
            }) => {
                // Split what now holds the neighbours still open, on the
                // surface (main area or window) of the first of them.
                let found = open(dock, beside);
                let Some(&(surface, first)) = found.first() else {
                    return false;
                };
                let together = found.iter().filter(|(s, _)| *s == surface);
                let node = together.fold(first, |all, (_, node)| common(all, *node));
                let share = share.clamp(0.02, 0.98);
                dock.split((surface, node), side.split(), share, Node::leaf(panel));
            }
            Some(Place::Window { at, size }) => {
                let surface = dock.add_window(vec![panel]);
                if let Some(window) = dock.get_window_state_mut(surface) {
                    window
                        .set_position(egui::pos2(at[0], at[1]))
                        .set_size(egui::vec2(size[0], size[1]));
                }
            }
            None => return false,
        }
        true
    }

    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        storage
            .and_then(|s| s.get_string(PLACES_KEY))
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(text) = ron::to_string(self) {
            storage.set_string(PLACES_KEY, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{apply, default_layout, is_open, is_valid, LayoutRequest};
    use super::*;

    fn toggle(dock: &mut DockState<PanelTab>, places: &mut Places, panel: PanelTab) {
        apply(dock, places, LayoutRequest::Toggle(panel));
    }

    fn node_of(dock: &DockState<PanelTab>, panel: PanelTab) -> (SurfaceIndex, NodeIndex) {
        let (surface, node, _) = dock.find_tab(&panel).unwrap();
        (surface, node)
    }

    /// The default layout with `panel` taken out and docked again by `dock_it`.
    fn moved(
        panel: PanelTab,
        dock_it: impl FnOnce(&mut DockState<PanelTab>),
    ) -> (DockState<PanelTab>, Places) {
        let (mut dock, mut places) = (default_layout(), Places::default());
        toggle(&mut dock, &mut places, panel);
        dock_it(&mut dock);
        (dock, places)
    }

    /// Close `panel` and show it again: the layout must come back the same.
    fn assert_returns(dock: &mut DockState<PanelTab>, places: &mut Places, panel: PanelTab) {
        let before = ron::to_string(dock.main_surface()).unwrap();
        toggle(dock, places, panel);
        assert!(!is_open(dock, panel));
        toggle(dock, places, panel);
        assert_eq!(ron::to_string(dock.main_surface()).unwrap(), before);
        assert!(is_valid(dock));
    }

    #[test]
    fn a_panel_docked_on_the_right_comes_back_on_the_right_at_its_width() {
        let (mut dock, mut places) = moved(PanelTab::Properties, |dock| {
            let tabs = vec![PanelTab::Properties];
            dock.main_surface_mut()
                .split_right(NodeIndex::root(), 0.85, tabs);
        });
        assert_returns(&mut dock, &mut places, PanelTab::Properties);
        assert_eq!(
            node_of(&dock, PanelTab::Properties).1,
            NodeIndex::root().right()
        );
    }

    #[test]
    fn a_panel_docked_along_the_bottom_comes_back_there() {
        let (mut dock, mut places) = moved(PanelTab::CommandLine, |dock| {
            let tabs = vec![PanelTab::CommandLine];
            dock.main_surface_mut()
                .split_below(NodeIndex::root(), 0.8, tabs);
        });
        assert_returns(&mut dock, &mut places, PanelTab::CommandLine);
        // Under the drawing alone, too.
        let (mut dock, mut places) = moved(PanelTab::CommandLine, |dock| {
            let (_, canvas) = node_of(dock, PanelTab::Canvas);
            let tabs = vec![PanelTab::CommandLine];
            dock.main_surface_mut().split_below(canvas, 0.8, tabs);
        });
        assert_returns(&mut dock, &mut places, PanelTab::CommandLine);
    }

    #[test]
    fn a_tab_rejoins_its_strip_at_its_position() {
        let (mut dock, mut places) = moved(PanelTab::Properties, |dock| {
            let (_, layers) = node_of(dock, PanelTab::Layers);
            dock.main_surface_mut()[layers].insert_tab(TabIndex(0), PanelTab::Properties);
        });
        toggle(&mut dock, &mut places, PanelTab::Properties);
        toggle(&mut dock, &mut places, PanelTab::Properties);
        let (_, node, tab) = dock.find_tab(&PanelTab::Properties).unwrap();
        assert_eq!(node, node_of(&dock, PanelTab::Layers).1);
        assert_eq!(tab, TabIndex(0));
        // The strip's last tab comes back last, and in front.
        toggle(&mut dock, &mut places, PanelTab::Layers);
        toggle(&mut dock, &mut places, PanelTab::Layers);
        let (_, node, tab) = dock.find_tab(&PanelTab::Layers).unwrap();
        assert_eq!(node, node_of(&dock, PanelTab::Properties).1);
        assert_eq!(tab, TabIndex(1));
    }

    #[test]
    fn a_floating_panel_floats_again() {
        let (mut dock, mut places) = (default_layout(), Places::default());
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        let rect = egui::Rect::from_min_size(egui::pos2(300.0, 200.0), egui::vec2(260.0, 400.0));
        dock.detach_tab(found, rect);
        // The dock has not drawn the window yet, so its place is given here.
        let window = Place::Window {
            at: [300.0, 200.0],
            size: [260.0, 400.0],
        };
        places.0.insert(PanelTab::Layers, window.clone());
        toggle(&mut dock, &mut places, PanelTab::Layers);
        assert_eq!(dock.iter_surfaces().count(), 1, "the window went with it");
        toggle(&mut dock, &mut places, PanelTab::Layers);
        let (surface, node) = node_of(&dock, PanelTab::Layers);
        assert_ne!(surface, SurfaceIndex::main());
        assert_eq!(node, NodeIndex::root());
        assert_eq!(places.0.get(&PanelTab::Layers), Some(&window));
    }

    #[test]
    fn a_panel_goes_beside_the_neighbours_that_are_still_open() {
        // Properties right of everything; then Layers and the command line go.
        let (mut dock, mut places) = moved(PanelTab::Properties, |dock| {
            let tabs = vec![PanelTab::Properties];
            dock.main_surface_mut()
                .split_right(NodeIndex::root(), 0.85, tabs);
        });
        toggle(&mut dock, &mut places, PanelTab::Properties);
        toggle(&mut dock, &mut places, PanelTab::Layers);
        toggle(&mut dock, &mut places, PanelTab::CommandLine);
        toggle(&mut dock, &mut places, PanelTab::Properties);
        assert_eq!(node_of(&dock, PanelTab::Canvas).1, NodeIndex::root().left());
        assert_eq!(
            node_of(&dock, PanelTab::Properties).1,
            NodeIndex::root().right()
        );
    }

    #[test]
    fn a_panel_whose_neighbours_are_all_gone_takes_its_default_place() {
        // Layers in a window, with Properties as its only neighbour.
        let (mut dock, mut places) = (default_layout(), Places::default());
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        let window = dock.detach_tab(found, egui::Rect::EVERYTHING);
        toggle(&mut dock, &mut places, PanelTab::Properties);
        dock[window].split_left(NodeIndex::root(), 0.5, vec![PanelTab::Properties]);
        toggle(&mut dock, &mut places, PanelTab::Properties);
        toggle(&mut dock, &mut places, PanelTab::Layers);
        assert_eq!(dock.iter_surfaces().count(), 1);
        // Properties was left of Layers, which is hidden: back to the left
        // of the main area, as in the default layout.
        toggle(&mut dock, &mut places, PanelTab::Properties);
        assert_eq!(
            node_of(&dock, PanelTab::Properties),
            (SurfaceIndex::main(), NodeIndex::root().left())
        );
        assert!(is_valid(&dock));
    }

    #[test]
    fn reset_forgets_the_places() {
        let (mut dock, mut places) = moved(PanelTab::Properties, |dock| {
            let tabs = vec![PanelTab::Properties];
            dock.main_surface_mut()
                .split_right(NodeIndex::root(), 0.85, tabs);
        });
        toggle(&mut dock, &mut places, PanelTab::Properties);
        apply(&mut dock, &mut places, LayoutRequest::Reset);
        assert_eq!(places, Places::default());
        // Hidden after a reset, a panel has its default place to return to.
        toggle(&mut dock, &mut places, PanelTab::Properties);
        places.clear();
        toggle(&mut dock, &mut places, PanelTab::Properties);
        assert_eq!(
            node_of(&dock, PanelTab::Properties).1,
            NodeIndex::root().left()
        );
    }

    #[test]
    fn places_survive_saving_and_old_settings_have_none() {
        let (mut dock, mut places) = moved(PanelTab::Properties, |dock| {
            let tabs = vec![PanelTab::Properties];
            dock.main_surface_mut()
                .split_right(NodeIndex::root(), 0.85, tabs);
        });
        toggle(&mut dock, &mut places, PanelTab::Properties);
        let text = ron::to_string(&places).unwrap();
        let back: Places = ron::from_str(&text).unwrap();
        assert_eq!(back, places);
        assert_eq!(Places::load(None), Places::default());
    }
}
