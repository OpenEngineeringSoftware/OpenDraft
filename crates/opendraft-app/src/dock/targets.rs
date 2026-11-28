//! Where a panel dragged by its tab may be dropped, and what the drop does.
//! A drop on a tab bar the user can see is left to egui_dock: the panel
//! joins that strip at that spot. A drop on one of the targets here docks
//! it: left of, right of or below the drawing, or above or below another
//! panel. Anywhere else it floats, so a panel can be put over the drawing
//! without docking by accident.

use eframe::egui::{self, Pos2, Rect, Vec2};
use egui_dock::{DockState, Node, NodeIndex, SurfaceIndex};

use super::{PanelTab, TAB_BAR_HEIGHT};

/// The side of its anchor a dropped panel docks on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
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

/// One drop target: a button on the `anchor` panel's place, and the part
/// of that place the dropped panel will take.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub anchor: PanelTab,
    pub side: Side,
    pub button: Rect,
    pub preview: Rect,
    /// The dropped panel's share of the anchor's place.
    share: f32,
}

/// What letting go of the dragged tab does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drop {
    /// On a tab bar: egui_dock puts the panel among those tabs.
    TabBar,
    Dock(Target),
    /// Into a window of its own at the pointer.
    Float,
}

/// Width and height of a target button.
pub const BUTTON: f32 = 30.0;

/// Gap between a target button and the edge it docks to.
const INSET: f32 = 14.0;

/// Leeway around a button that still counts as on it.
const REACH: f32 = 4.0;

/// The least of the drawing's width or height a panel docked beside it
/// takes, and the most of its width; below it, a panel takes at most what
/// the command line strip is given. Within these it keeps the size it was
/// dragged at.
const SHARE_RANGE: (f32, f32) = (0.12, 0.4);

/// A main-area leaf: its node, rect and tabs.
fn leaves(dock: &DockState<PanelTab>) -> impl Iterator<Item = (NodeIndex, Rect, &[PanelTab])> {
    dock.main_surface()
        .iter()
        .enumerate()
        .filter_map(|(i, node)| match node {
            Node::Leaf { rect, tabs, .. } => Some((NodeIndex(i), *rect, tabs.as_slice())),
            _ => None,
        })
}

/// The leaf of `panel` on the main surface, if it is docked there.
fn main_leaf(dock: &DockState<PanelTab>, panel: PanelTab) -> Option<NodeIndex> {
    dock.find_main_surface_tab(&panel).map(|(node, _)| node)
}

/// A button `BUTTON` square centred `INSET` in from the middle of `side`.
fn button(rect: Rect, side: Side, top_margin: f32) -> Rect {
    let off = INSET + BUTTON / 2.0;
    let centre = match side {
        Side::Left => rect.left_center() + egui::vec2(off, 0.0),
        Side::Right => rect.right_center() - egui::vec2(off, 0.0),
        Side::Above => rect.center_top() + egui::vec2(0.0, off + top_margin),
        Side::Below => rect.center_bottom() - egui::vec2(0.0, off),
    };
    Rect::from_center_size(centre, Vec2::splat(BUTTON))
}

/// The part of `rect` a panel docked on `side` with `share` of it takes.
fn preview(rect: Rect, side: Side, share: f32) -> Rect {
    let (w, h) = (rect.width() * share, rect.height() * share);
    match side {
        Side::Left => rect.with_max_x(rect.left() + w),
        Side::Right => rect.with_min_x(rect.right() - w),
        Side::Above => rect.with_max_y(rect.top() + h),
        Side::Below => rect.with_min_y(rect.bottom() - h),
    }
}

/// The targets for dragging `panel`, last laid out `size` big, with the
/// pointer `at`: the drawing's three always, and above and below the
/// panel under the pointer.
pub fn targets(dock: &DockState<PanelTab>, panel: PanelTab, size: Vec2, at: Pos2) -> Vec<Target> {
    let mut found = Vec::new();
    if !panel.closeable() {
        return found;
    }
    let own = main_leaf(dock, panel);
    let alone = |node: NodeIndex| own == Some(node) && dock.main_surface()[node].tabs_count() == 1;
    for (node, rect, tabs) in leaves(dock) {
        if !rect.is_positive() || alone(node) {
            continue;
        }
        if tabs.contains(&PanelTab::Canvas) {
            for side in [Side::Left, Side::Right, Side::Below] {
                let (along, across, most) = match side {
                    Side::Below => (size.y, rect.height(), 1.0 - super::BELOW_SPLIT),
                    _ => (size.x, rect.width(), SHARE_RANGE.1),
                };
                let share = (along / across).clamp(SHARE_RANGE.0, most);
                found.push(target(PanelTab::Canvas, rect, side, share, 0.0));
            }
        } else if rect.contains(at) {
            // Anchored by a tab that stays put when the dragged one leaves.
            let Some(&anchor) = tabs.iter().find(|&&t| t != panel) else {
                continue;
            };
            for side in [Side::Above, Side::Below] {
                found.push(target(anchor, rect, side, 0.5, TAB_BAR_HEIGHT));
            }
        }
    }
    found
}

fn target(anchor: PanelTab, rect: Rect, side: Side, share: f32, top: f32) -> Target {
    Target {
        anchor,
        side,
        button: button(rect, side, top),
        preview: preview(rect, side, share),
        share,
    }
}

/// The tab bar of a leaf, unless the app draws over it: the drawing's
/// when it is alone, the command line's when it is a strip.
fn visible_bar(dock: &DockState<PanelTab>, rect: Rect, tabs: &[PanelTab]) -> Option<Rect> {
    let hidden = match tabs {
        [PanelTab::Canvas] => true,
        [PanelTab::CommandLine] => super::command_line_strip(dock).is_some(),
        _ => false,
    };
    (!hidden).then(|| rect.with_max_y(rect.top() + TAB_BAR_HEIGHT))
}

/// What letting go `at` would do, given the targets on show.
pub fn drop_at(dock: &DockState<PanelTab>, targets: &[Target], at: Pos2) -> Drop {
    if let Some(t) = targets.iter().find(|t| t.button.expand(REACH).contains(at)) {
        return Drop::Dock(*t);
    }
    let on_bar = dock.iter_all_nodes().any(|(_, node)| match node {
        Node::Leaf { rect, tabs, .. } => {
            visible_bar(dock, *rect, tabs).is_some_and(|bar| bar.contains(at))
        }
        _ => false,
    });
    match on_bar {
        true => Drop::TabBar,
        false => Drop::Float,
    }
}

/// Make the dock match `drop` once egui_dock has handled the release:
/// it floats a panel dropped on a panel's body, and puts one dropped on a
/// tab bar among its tabs, hidden bars included.
pub fn finish(dock: &mut DockState<PanelTab>, panel: PanelTab, drop: Drop, at: Pos2, size: Vec2) {
    let Some(found) = dock.find_tab(&panel) else {
        return;
    };
    match drop {
        Drop::TabBar => {}
        Drop::Float if found.0 == SurfaceIndex::main() => {
            dock.detach_tab(found, Rect::from_min_size(at, size));
        }
        Drop::Float => {}
        Drop::Dock(target) => {
            dock.remove_tab(found);
            insert(dock, panel, target);
        }
    }
}

/// Dock `panel`, which is not in the dock, on `target`'s side of its anchor.
pub fn insert(dock: &mut DockState<PanelTab>, panel: PanelTab, target: Target) {
    let Some((surface, node, _)) = dock.find_tab(&target.anchor) else {
        // The anchor went with it; put the panel back somewhere.
        dock.add_window(vec![panel]);
        return;
    };
    let fraction = match target.side {
        Side::Left | Side::Above => target.share,
        Side::Right | Side::Below => 1.0 - target.share,
    };
    let leaf = Node::leaf(panel);
    dock.split((surface, node), target.side.split(), fraction, leaf);
    if let Some((surface, node, _)) = dock.find_tab(&panel) {
        dock.set_focused_node_and_surface((surface, node));
    }
}

/// Put `panel`, which is not in the dock, among the tabs of the strip whose
/// tab bar `at` is on. False when `at` is on no tab bar.
pub fn join_bar(dock: &mut DockState<PanelTab>, panel: PanelTab, at: Pos2) -> bool {
    let node = leaves(dock)
        .find(|(_, rect, tabs)| visible_bar(dock, *rect, tabs).is_some_and(|bar| bar.contains(at)))
        .map(|(node, _, _)| node);
    let Some(node) = node else {
        return false;
    };
    let strip = &mut dock.main_surface_mut()[node];
    let end = strip.tabs_count();
    strip.insert_tab(egui_dock::TabIndex(end), panel);
    dock.set_focused_node_and_surface((SurfaceIndex::main(), node));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::default_layout;

    /// The default layout as laid out in a 1000 x 600 area: Properties 0 to
    /// 130, the drawing 130 to 826, the command line over Layers beyond.
    fn laid_out() -> DockState<PanelTab> {
        let mut dock = default_layout();
        let rects = [
            (
                PanelTab::Properties,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(130.0, 600.0)),
            ),
            (
                PanelTab::Canvas,
                Rect::from_min_max(egui::pos2(130.0, 0.0), egui::pos2(826.0, 600.0)),
            ),
            (
                PanelTab::CommandLine,
                Rect::from_min_max(egui::pos2(826.0, 0.0), egui::pos2(1000.0, 180.0)),
            ),
            (
                PanelTab::Layers,
                Rect::from_min_max(egui::pos2(826.0, 180.0), egui::pos2(1000.0, 600.0)),
            ),
        ];
        for (panel, at) in rects {
            let node = main_leaf(&dock, panel).unwrap();
            if let Node::Leaf { rect, .. } = &mut dock.main_surface_mut()[node] {
                *rect = at;
            }
        }
        dock
    }

    fn drawing() -> Rect {
        Rect::from_min_max(egui::pos2(130.0, 0.0), egui::pos2(826.0, 600.0))
    }

    #[test]
    fn a_panel_not_in_the_dock_docks_on_a_target_or_joins_a_tab_bar() {
        let mut dock = laid_out();
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        dock.remove_tab(found);
        let size = egui::vec2(174.0, 180.0);
        let at = drawing().center();
        let all = targets(&dock, PanelTab::Layers, size, at);
        let right = all.iter().find(|t| t.side == Side::Right).unwrap();
        insert(&mut dock, PanelTab::Layers, *right);
        assert!(dock.find_tab(&PanelTab::Layers).is_some());
        // A tab bar the user can see takes it as another tab.
        let bar = egui::pos2(60.0, 5.0);
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        dock.remove_tab(found);
        assert!(join_bar(&mut dock, PanelTab::Layers, bar));
        let (_, node, _) = dock.find_tab(&PanelTab::Layers).unwrap();
        assert_eq!(node, main_leaf(&dock, PanelTab::Properties).unwrap());
        assert!(!join_bar(
            &mut dock,
            PanelTab::Assistant,
            drawing().center()
        ));
    }

    #[test]
    fn the_drawing_offers_left_right_and_below() {
        let dock = laid_out();
        let size = egui::vec2(174.0, 180.0);
        let found = targets(&dock, PanelTab::Layers, size, drawing().center());
        let sides: Vec<_> = found.iter().map(|t| (t.anchor, t.side)).collect();
        assert_eq!(
            sides,
            [
                (PanelTab::Canvas, Side::Left),
                (PanelTab::Canvas, Side::Right),
                (PanelTab::Canvas, Side::Below),
            ]
        );
        // Each button sits inside the drawing, by the edge it docks to.
        for t in &found {
            assert!(drawing().contains_rect(t.button), "{:?}", t.side);
            assert!(drawing().contains_rect(t.preview), "{:?}", t.side);
        }
        // The panel keeps the width it had, as far as the limits allow.
        assert!((found[0].preview.width() - 174.0).abs() < 1e-3);
        assert_eq!(found[1].preview.right(), drawing().right());
        assert_eq!(found[2].preview.bottom(), drawing().bottom());
        // Below, no taller than the command line strip is made.
        let tall = targets(
            &dock,
            PanelTab::Layers,
            Vec2::splat(900.0),
            drawing().center(),
        );
        let most = 600.0 * (1.0 - crate::dock::BELOW_SPLIT);
        assert!((tall[2].preview.height() - most).abs() < 1e-3);
    }

    #[test]
    fn a_side_panel_offers_above_and_below_only_under_the_pointer() {
        let dock = laid_out();
        let over_layers = egui::pos2(900.0, 400.0);
        let size = egui::vec2(130.0, 600.0);
        let found = targets(&dock, PanelTab::Properties, size, over_layers);
        let sides: Vec<_> = found.iter().map(|t| (t.anchor, t.side)).collect();
        assert!(sides.contains(&(PanelTab::Layers, Side::Above)));
        assert!(sides.contains(&(PanelTab::Layers, Side::Below)));
        assert!(!sides.iter().any(|(a, _)| *a == PanelTab::CommandLine));
        // The upper button clears the tab bar.
        let above = found.iter().find(|t| t.side == Side::Above).unwrap();
        assert!(above.button.top() > 180.0 + TAB_BAR_HEIGHT);
    }

    #[test]
    fn a_panel_alone_in_its_place_gets_no_targets_on_itself() {
        let dock = laid_out();
        let found = targets(
            &dock,
            PanelTab::Layers,
            Vec2::splat(200.0),
            egui::pos2(900.0, 400.0),
        );
        assert!(found.iter().all(|t| t.anchor != PanelTab::Layers));
        // Nor does the drawing, which cannot leave.
        let found = targets(
            &dock,
            PanelTab::Canvas,
            Vec2::splat(200.0),
            drawing().center(),
        );
        assert!(found.is_empty());
    }

    #[test]
    fn letting_go_on_a_panel_body_floats_and_on_a_tab_bar_leaves_it_to_the_dock() {
        let dock = laid_out();
        let found = targets(
            &dock,
            PanelTab::Layers,
            Vec2::splat(200.0),
            drawing().center(),
        );
        assert_eq!(drop_at(&dock, &found, drawing().center()), Drop::Float);
        assert_eq!(drop_at(&dock, &found, egui::pos2(60.0, 10.0)), Drop::TabBar);
        // The drawing alone has no tab bar to drop on.
        assert_eq!(drop_at(&dock, &found, egui::pos2(400.0, 10.0)), Drop::Float);
        let on = found[2].button.center();
        assert_eq!(drop_at(&dock, &found, on), Drop::Dock(found[2]));
    }

    #[test]
    fn a_target_docks_the_panel_on_that_side_of_its_anchor() {
        let mut dock = laid_out();
        let found = targets(
            &dock,
            PanelTab::Layers,
            egui::vec2(174.0, 120.0),
            drawing().center(),
        );
        let below = found[2];
        finish(
            &mut dock,
            PanelTab::Layers,
            Drop::Dock(below),
            Pos2::ZERO,
            Vec2::ZERO,
        );
        let layers = main_leaf(&dock, PanelTab::Layers).unwrap();
        let canvas = main_leaf(&dock, PanelTab::Canvas).unwrap();
        let parent = layers.parent().unwrap();
        assert_eq!(canvas.parent(), Some(parent), "split from the drawing");
        assert_eq!(layers, parent.right(), "below it");
        match dock.main_surface()[parent] {
            Node::Vertical { fraction, .. } => assert!((fraction - 0.8).abs() < 1e-6),
            _ => panic!("not a vertical split"),
        }
    }

    #[test]
    fn a_floated_drop_that_the_dock_kept_goes_into_a_window() {
        let mut dock = laid_out();
        // As if egui_dock had put Layers in the drawing's hidden tab bar.
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        dock.remove_tab(found);
        let canvas = main_leaf(&dock, PanelTab::Canvas).unwrap();
        dock.main_surface_mut()[canvas].append_tab(PanelTab::Layers);
        finish(
            &mut dock,
            PanelTab::Layers,
            Drop::Float,
            egui::pos2(300.0, 200.0),
            Vec2::splat(200.0),
        );
        let (surface, ..) = dock.find_tab(&PanelTab::Layers).unwrap();
        assert_ne!(surface, SurfaceIndex::main());
        assert_eq!(dock.main_surface()[canvas].tabs_count(), 1);
    }
}
