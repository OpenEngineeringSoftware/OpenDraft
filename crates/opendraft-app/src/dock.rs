//! Dockable panel layout: which panels exist, where they sit, and how the
//! layout is saved. The panels themselves are drawn by `app.rs`.

use eframe::egui;
use egui_dock::{DockState, NodeIndex};
use opendraft_commands::Command;
use opendraft_ui::{key, Theme};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

mod drag;
mod places;
mod sizes;
mod targets;

pub use drag::{offer_detached, show_area};
pub use places::Places;
pub use sizes::{keep_minimum_sizes, split_shares};

// Bumped when a new default panel is added, so saved layouts pick it up.
const LAYOUT_KEY: &str = "dock_layout_v4";

/// One dockable panel. The canvas is a panel too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PanelTab {
    Canvas,
    Properties,
    Layers,
    CommandLine,
    Assistant,
}

impl PanelTab {
    /// The tab's title in English; drawn translated.
    pub fn title(self) -> &'static str {
        match self {
            PanelTab::Canvas => key!("Drawing"),
            PanelTab::Properties => key!("Properties"),
            PanelTab::Layers => key!("Layers"),
            PanelTab::CommandLine => key!("Command Line"),
            PanelTab::Assistant => key!("Assistant"),
        }
    }

    /// The drawing view cannot be closed or floated; everything else can.
    pub fn closeable(self) -> bool {
        self != PanelTab::Canvas
    }
}

/// A change to the layout asked for by a command, applied by the app next
/// frame (commands cannot reach the dock state directly).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutRequest {
    Reset,
    /// Show the panel if hidden, hide it if shown.
    Toggle(PanelTab),
    /// Take a docked panel out into a window of its own.
    Float(PanelTab),
}

/// Share of the window width kept by the Properties column on the left.
const PROPERTIES_SPLIT: f32 = 0.13;

/// Share of the rest kept by the drawing, beside the right-hand column.
const CANVAS_SPLIT: f32 = 0.8;

/// Share of the right-hand column kept by the command line above Layers.
const SIDE_SPLIT: f32 = 0.3;

/// Share of a column kept by whatever sits above a command line docked
/// under it (where the user may have moved it).
const BELOW_SPLIT: f32 = 0.74;

/// Properties on the left, the drawing in the middle, and the command line
/// above Layers on the right.
pub fn default_layout() -> DockState<PanelTab> {
    let mut dock = DockState::new(vec![PanelTab::Canvas]);
    let surface = dock.main_surface_mut();
    let [canvas, _] = surface.split_left(
        NodeIndex::root(),
        PROPERTIES_SPLIT,
        vec![PanelTab::Properties],
    );
    let [_, side] = surface.split_right(canvas, CANVAS_SPLIT, vec![PanelTab::CommandLine]);
    surface.split_below(side, SIDE_SPLIT, vec![PanelTab::Layers]);
    dock
}

/// Height in points of the command line strip when only the input shows
/// (tab bar plus one input row).
pub const COLLAPSED_HEIGHT: f32 = 68.0;

/// Height of the tab bar above every panel.
pub const TAB_BAR_HEIGHT: f32 = 24.0;

/// The least width or height a dragged separator leaves a panel.
const PANEL_MIN: f32 = 175.0;

/// The least height it leaves the command line with its tab bar. As a
/// strip the app measures it: two lines of history above the input row.
pub const COMMAND_LINE_MIN_HEIGHT: f32 = 74.0;

/// The command line docked above or below the drawing with no other tab
/// beside it. It is then drawn like AutoCAD's: no tab bar, and a narrow grip
/// down its left edge instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strip {
    Above,
    Below,
}

/// The other half of the split `node` is in.
fn sibling(node: NodeIndex) -> Option<NodeIndex> {
    let parent = node.parent()?;
    Some(if node == parent.left() {
        parent.right()
    } else {
        parent.left()
    })
}

/// Whether the command line shows as a strip, and on which side of the
/// drawing.
pub fn command_line_strip(dock: &DockState<PanelTab>) -> Option<Strip> {
    let (surface, node, _) = dock.find_tab(&PanelTab::CommandLine)?;
    let (_, canvas, _) = dock.find_tab(&PanelTab::Canvas)?;
    let (parent, other) = (node.parent()?, sibling(node)?);
    let tree = dock.main_surface();
    if surface != egui_dock::SurfaceIndex::main()
        || tree[node].tabs_count() != 1
        || !matches!(tree[parent], egui_dock::Node::Vertical { .. })
    {
        return None;
    }
    // The other half of the split holds the drawing.
    std::iter::successors(Some(canvas), |n| n.parent())
        .any(|n| n == other)
        .then_some(match node == parent.left() {
            true => Strip::Above,
            false => Strip::Below,
        })
}

/// A strip dropped along the whole window moves to the drawing's own column,
/// so the panels beside the drawing keep their full height, as in AutoCAD.
pub fn tuck_command_line(dock: &mut DockState<PanelTab>) {
    let Some(strip) = command_line_strip(dock) else {
        return;
    };
    let (Some(found), Some((_, canvas, _))) = (
        dock.find_tab(&PanelTab::CommandLine),
        dock.find_tab(&PanelTab::Canvas),
    ) else {
        return;
    };
    if sibling(found.1) == Some(canvas) {
        return;
    }
    let tree = dock.main_surface();
    let height = |node: NodeIndex| tree[node].rect().map_or(0.0, |r| r.height());
    // The drawing gets back the height the strip leaves, then shares it.
    let (own, drawing) = (height(found.1), height(canvas));
    let share = match own > 0.0 && drawing > 0.0 {
        true => own / (own + drawing),
        false => 1.0 - BELOW_SPLIT,
    };
    dock.remove_tab(found);
    let Some((_, canvas, _)) = dock.find_tab(&PanelTab::Canvas) else {
        return;
    };
    // The fraction is the top half's share.
    let (tree, tabs) = (dock.main_surface_mut(), vec![PanelTab::CommandLine]);
    match strip {
        Strip::Above => tree.split_above(canvas, share, tabs),
        Strip::Below => tree.split_below(canvas, 1.0 - share, tabs),
    };
}

/// Collapse the command line to its input row (`collapsed` points high), or
/// restore it. `saved` remembers the expanded share between the two. Does
/// nothing if the panel is not one half of a vertical split.
pub fn resize_command_line(
    dock: &mut DockState<PanelTab>,
    collapse: bool,
    saved: &mut Option<f32>,
    collapsed: f32,
) {
    let Some((surface, node, _)) = dock.find_tab(&PanelTab::CommandLine) else {
        return;
    };
    let (true, Some(parent)) = (surface == egui_dock::SurfaceIndex::main(), node.parent()) else {
        return;
    };
    // The fraction is the top child's share; odd indices are top children.
    let on_top = node.0 % 2 == 1;
    if let egui_dock::Node::Vertical { rect, fraction, .. } = &mut dock.main_surface_mut()[parent] {
        if collapse {
            if rect.height() > collapsed {
                saved.get_or_insert(*fraction);
                let share = collapsed / rect.height();
                *fraction = (if on_top { share } else { 1.0 - share }).clamp(0.02, 0.98);
            }
        } else {
            let default = if on_top { SIDE_SPLIT } else { BELOW_SPLIT };
            *fraction = saved.take().unwrap_or(default);
        }
    }
}

/// Whether the drawing view has its place to itself, with no other tab to
/// switch to: it then needs no tab bar.
pub fn drawing_alone(dock: &DockState<PanelTab>) -> bool {
    dock.find_tab(&PanelTab::Canvas)
        .is_some_and(|(surface, node, _)| dock[surface][node].tabs_count() == 1)
}

pub fn is_open(dock: &DockState<PanelTab>, panel: PanelTab) -> bool {
    dock.find_tab(&panel).is_some()
}

pub fn apply(dock: &mut DockState<PanelTab>, places: &mut Places, request: LayoutRequest) {
    match request {
        LayoutRequest::Reset => {
            *dock = default_layout();
            places.clear();
        }
        LayoutRequest::Toggle(panel) => {
            if let Some(found) = dock.find_tab(&panel) {
                if panel.closeable() {
                    places.note(dock, None);
                    dock.remove_tab(found);
                }
            } else if !places.restore(dock, panel) {
                restore(dock, panel);
            }
        }
        LayoutRequest::Float(panel) => float(dock, panel),
    }
}

/// The panels in the dock's own floating windows, with each window's inside
/// in the main window's coordinates when egui knows it.
pub fn floating(
    dock: &DockState<PanelTab>,
    ctx: &egui::Context,
) -> Vec<(PanelTab, Option<egui::Rect>)> {
    dock.iter_all_tabs()
        .filter(|((surface, _), tab)| {
            *surface != egui_dock::SurfaceIndex::main() && tab.closeable()
        })
        .map(|((surface, _), tab)| (*tab, places::window_rect(ctx, surface)))
        .collect()
}

/// Show a hidden panel docked in the main area, where it was or where the
/// default layout keeps it, never in a floating window.
pub fn dock_back(dock: &mut DockState<PanelTab>, places: &mut Places, panel: PanelTab) {
    if dock.find_tab(&panel).is_some() {
        return;
    }
    if places.restore(dock, panel) {
        match dock.find_tab(&panel) {
            Some(found) if found.0 != egui_dock::SurfaceIndex::main() => {
                dock.remove_tab(found);
            }
            _ => return,
        }
    }
    restore(dock, panel);
}

/// Move a panel docked in the main area to a window just above its place.
fn float(dock: &mut DockState<PanelTab>, panel: PanelTab) {
    let Some(found) = dock.find_tab(&panel) else {
        return;
    };
    if !panel.closeable() || found.0 != egui_dock::SurfaceIndex::main() {
        return;
    }
    let from = dock.main_surface()[found.1]
        .rect()
        .unwrap_or(egui::Rect::ZERO);
    // A docked panel's window opens at 0.8 of the size asked for.
    let size = egui::vec2(from.width().clamp(400.0, 800.0), 300.0);
    let at = egui::pos2(from.left() + 40.0, (from.top() - size.y).max(40.0));
    dock.detach_tab(found, egui::Rect::from_min_size(at, size));
}

/// Put a hidden panel that has no place to return to (see [`Places`]) where
/// the default layout keeps it: Properties down the left; the command line
/// and Layers sharing the right-hand column, the command line on top; the
/// assistant beside Layers.
fn restore(dock: &mut DockState<PanelTab>, panel: PanelTab) {
    let partner = match panel {
        PanelTab::CommandLine => Some((PanelTab::Layers, egui_dock::Split::Above)),
        PanelTab::Layers => Some((PanelTab::CommandLine, egui_dock::Split::Below)),
        _ => None,
    };
    if let Some((tab, split)) = partner {
        if let Some((surface, node, _)) = dock.find_tab(&tab) {
            // SIDE_SPLIT is the top child's share either way.
            dock.split(
                (surface, node),
                split,
                SIDE_SPLIT,
                egui_dock::Node::leaf(panel),
            );
            return;
        }
    }
    // The assistant is a tab beside Layers, the tallest place in the column.
    if panel == PanelTab::Assistant {
        if let Some((surface, node, _)) = dock.find_tab(&PanelTab::Layers) {
            dock.set_focused_node_and_surface((surface, node));
            dock.push_to_focused_leaf(panel);
            return;
        }
    }
    let surface = dock.main_surface_mut();
    if panel == PanelTab::Properties {
        surface.split_left(NodeIndex::root(), PROPERTIES_SPLIT, vec![panel]);
    } else {
        surface.split_right(NodeIndex::root(), CANVAS_SPLIT, vec![panel]);
    }
}

/// A saved layout is only usable if the drawing view is in it exactly once.
fn is_valid(dock: &DockState<PanelTab>) -> bool {
    dock.iter_all_tabs()
        .filter(|(_, tab)| **tab == PanelTab::Canvas)
        .count()
        == 1
}

/// The saved layout and the places of its hidden panels. Without a usable
/// layout both start afresh.
pub fn load(storage: Option<&dyn eframe::Storage>) -> (DockState<PanelTab>, Places) {
    let saved = storage
        .and_then(|s| s.get_string(LAYOUT_KEY))
        .and_then(|text| ron::from_str::<DockState<PanelTab>>(&text).ok())
        .filter(is_valid);
    match saved {
        Some(dock) => (dock, Places::load(storage)),
        None => (default_layout(), Places::default()),
    }
}

pub fn save(dock: &DockState<PanelTab>, places: &Places, storage: &mut dyn eframe::Storage) {
    if let Ok(text) = ron::to_string(dock) {
        storage.set_string(LAYOUT_KEY, text);
    }
    places.save(storage);
}

/// Dock chrome in the app's flat dark style. A dragged separator leaves a
/// panel at least `least` points.
pub fn style(theme: &Theme, egui_style: &egui::Style, least: f32) -> egui_dock::Style {
    let c = &theme.colors;
    let mut style = egui_dock::Style::from_egui(egui_style);
    style.main_surface_border_stroke = egui::Stroke::NONE;
    style.separator.color_idle = c.divider;
    style.separator.color_hovered = c.accent;
    style.separator.color_dragged = c.accent;
    style.separator.width = 1.0;
    // The dock has one limit for every separator; `keep_minimum_sizes`
    // raises it again for all panels but the command line.
    style.separator.extra = least.min(PANEL_MIN);
    style.tab_bar.bg_fill = c.tab_strip();
    style.tab_bar.hline_color = c.divider;
    style.tab_bar.height = TAB_BAR_HEIGHT;
    // egui_dock only marks drops on tab bars; a drop on a panel's body
    // floats it, and `drag` draws the targets that dock it and the window
    // outline itself.
    style.overlay.overlay_type = egui_dock::OverlayType::HighlightedAreas;
    style.overlay.feel.center_drop_coverage = 0.0;
    style.overlay.selection_stroke_width = 0.0;
    style.overlay.selection_color = c.accent.gamma_multiply(0.45);
    let tab = &mut style.tab;
    tab.tab_body.bg_fill = c.sidebar_bg;
    tab.tab_body.stroke = egui::Stroke::NONE;
    tab.tab_body.inner_margin = egui::Margin::ZERO;
    for state in [
        &mut tab.active,
        &mut tab.inactive,
        &mut tab.focused,
        &mut tab.hovered,
        &mut tab.active_with_kb_focus,
        &mut tab.inactive_with_kb_focus,
        &mut tab.focused_with_kb_focus,
    ] {
        state.outline_color = egui::Color32::TRANSPARENT;
        state.corner_radius = egui::CornerRadius::ZERO;
    }
    tab.active.bg_fill = c.sidebar_bg;
    tab.active.text_color = c.text;
    tab.focused.bg_fill = c.sidebar_bg;
    tab.focused.text_color = c.text;
    // Tabs not showing merge into the strip, as the ribbon's do.
    tab.inactive.bg_fill = c.tab_strip();
    tab.inactive_with_kb_focus.bg_fill = c.tab_strip();
    tab.inactive.text_color = c.text_weak;
    tab.hovered.bg_fill = c.hover;
    tab.hovered.text_color = c.text;
    style
}

/// The panel a command shows or hides, for its button to look pressed.
pub fn panel_of(command: &str) -> Option<PanelTab> {
    match command {
        "view.panel_properties" => Some(PanelTab::Properties),
        "view.panel_layers" | "layer.properties" => Some(PanelTab::Layers),
        "view.command_line" => Some(PanelTab::CommandLine),
        "view.assistant" => Some(PanelTab::Assistant),
        _ => None,
    }
}

/// The commands that show, hide and reset the panels.
pub fn commands() -> Vec<Command<AppState>> {
    let toggle =
        |id, label, run: fn(&mut AppState)| Command::new(id, label, run).transparent().global();
    vec![
        toggle("view.panel_properties", "Properties Panel", |s| {
            s.layout_request = Some(LayoutRequest::Toggle(PanelTab::Properties))
        })
        .key("Ctrl+1"),
        toggle("view.panel_layers", "Layers Panel", |s| {
            s.layout_request = Some(LayoutRequest::Toggle(PanelTab::Layers))
        })
        .key("Ctrl+2"),
        toggle("view.command_line", "Command Line", |s| {
            s.layout_request = Some(LayoutRequest::Toggle(PanelTab::CommandLine))
        })
        .key("Ctrl+9"),
        toggle("view.assistant", "Assistant", |s| {
            s.layout_request = Some(LayoutRequest::Toggle(PanelTab::Assistant))
        }),
        toggle("view.reset_layout", "Reset Layout", |s| {
            s.layout_request = Some(LayoutRequest::Reset)
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_bars_and_hidden_tabs_take_the_tab_strip_tone() {
        let theme = Theme::dark();
        let style = style(&theme, &egui::Style::default(), 80.0);
        assert_eq!(style.tab_bar.bg_fill, theme.colors.tab_strip());
        assert_eq!(style.tab.inactive.bg_fill, theme.colors.tab_strip());
        assert_eq!(style.tab.active.bg_fill, theme.colors.sidebar_bg);
    }

    #[test]
    fn a_panel_docked_back_leaves_no_floating_window_behind() {
        let mut dock = default_layout();
        let mut places = Places::default();
        apply(
            &mut dock,
            &mut places,
            LayoutRequest::Float(PanelTab::Layers),
        );
        assert!(dock
            .find_tab(&PanelTab::Layers)
            .is_some_and(|f| f.0 != egui_dock::SurfaceIndex::main()));
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        dock.remove_tab(found);
        dock_back(&mut dock, &mut places, PanelTab::Layers);
        let found = dock.find_tab(&PanelTab::Layers).unwrap();
        assert_eq!(found.0, egui_dock::SurfaceIndex::main());
        assert!(is_valid(&dock));
    }

    #[test]
    fn default_layout_has_all_panels() {
        let dock = default_layout();
        assert!(is_open(&dock, PanelTab::CommandLine));
        assert!(is_open(&dock, PanelTab::Canvas));
        assert!(is_open(&dock, PanelTab::Properties));
        assert!(is_valid(&dock));
    }

    fn node_of(dock: &DockState<PanelTab>, panel: PanelTab) -> NodeIndex {
        dock.find_tab(&panel).unwrap().1
    }

    /// Properties left of everything, the command line above Layers in a
    /// column right of the drawing.
    fn assert_default_arrangement(dock: &DockState<PanelTab>) {
        let properties = node_of(dock, PanelTab::Properties);
        assert_eq!(properties, NodeIndex::root().left());
        let rest = NodeIndex::root().right();
        assert_eq!(node_of(dock, PanelTab::Canvas), rest.left());
        let side = rest.right();
        assert_eq!(node_of(dock, PanelTab::CommandLine), side.left());
        assert_eq!(node_of(dock, PanelTab::Layers), side.right());
        let surface = dock.main_surface();
        assert!(matches!(surface[side], egui_dock::Node::Vertical { .. }));
    }

    #[test]
    fn default_layout_puts_properties_left_and_the_command_line_above_layers() {
        assert_default_arrangement(&default_layout());
    }

    #[test]
    fn hidden_side_panels_come_back_where_they_were() {
        let mut dock = default_layout();
        for panel in [
            PanelTab::CommandLine,
            PanelTab::Layers,
            PanelTab::Properties,
        ] {
            apply(
                &mut dock,
                &mut Places::default(),
                LayoutRequest::Toggle(panel),
            );
            apply(
                &mut dock,
                &mut Places::default(),
                LayoutRequest::Toggle(panel),
            );
            assert_default_arrangement(&dock);
        }
    }

    #[test]
    fn the_command_line_collapses_from_the_top_of_its_column() {
        let mut dock = default_layout();
        let side = node_of(&dock, PanelTab::CommandLine).parent().unwrap();
        if let egui_dock::Node::Vertical { rect, .. } = &mut dock.main_surface_mut()[side] {
            *rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 680.0));
        }
        let fraction = |dock: &DockState<PanelTab>| match dock.main_surface()[side] {
            egui_dock::Node::Vertical { fraction, .. } => fraction,
            _ => panic!("not a vertical split"),
        };
        let mut saved = None;
        resize_command_line(&mut dock, true, &mut saved, COLLAPSED_HEIGHT);
        assert!((fraction(&dock) - 0.1).abs() < 1e-6);
        resize_command_line(&mut dock, false, &mut saved, COLLAPSED_HEIGHT);
        assert_eq!(fraction(&dock), SIDE_SPLIT);
    }

    /// The default layout with the command line moved under everything.
    fn docked_below() -> DockState<PanelTab> {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::CommandLine),
        );
        dock.main_surface_mut().split_below(
            NodeIndex::root(),
            BELOW_SPLIT,
            vec![PanelTab::CommandLine],
        );
        dock
    }

    fn set_rect(dock: &mut DockState<PanelTab>, node: NodeIndex, size: egui::Vec2) {
        match &mut dock.main_surface_mut()[node] {
            egui_dock::Node::Vertical { rect, .. }
            | egui_dock::Node::Horizontal { rect, .. }
            | egui_dock::Node::Leaf { rect, .. } => {
                *rect = egui::Rect::from_min_size(egui::pos2(0.0, 500.0), size);
            }
            egui_dock::Node::Empty => panic!("an empty node"),
        }
    }

    fn set_share(dock: &mut DockState<PanelTab>, node: NodeIndex, share: f32) {
        match &mut dock.main_surface_mut()[node] {
            egui_dock::Node::Vertical { fraction, .. }
            | egui_dock::Node::Horizontal { fraction, .. } => *fraction = share,
            _ => panic!("not a split"),
        }
    }

    #[test]
    fn the_command_line_is_a_strip_only_above_or_below_the_drawing() {
        // Above Layers in the side column: an ordinary panel.
        assert_eq!(command_line_strip(&default_layout()), None);
        let mut dock = docked_below();
        assert_eq!(command_line_strip(&dock), Some(Strip::Below));
        // Sharing its place with another tab, it needs its tab bar.
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Layers),
        );
        let (surface, node, _) = dock.find_tab(&PanelTab::CommandLine).unwrap();
        dock.set_focused_node_and_surface((surface, node));
        dock.push_to_focused_leaf(PanelTab::Layers);
        assert_eq!(command_line_strip(&dock), None);
        // Nor in a window of its own.
        let mut dock = docked_below();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Float(PanelTab::CommandLine),
        );
        assert_eq!(command_line_strip(&dock), None);
    }

    #[test]
    fn the_command_line_is_a_strip_right_under_the_drawing_too() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::CommandLine),
        );
        let canvas = node_of(&dock, PanelTab::Canvas);
        dock.main_surface_mut()
            .split_below(canvas, BELOW_SPLIT, vec![PanelTab::CommandLine]);
        assert_eq!(command_line_strip(&dock), Some(Strip::Below));
        let before = ron::to_string(&dock).unwrap();
        tuck_command_line(&mut dock);
        assert_eq!(ron::to_string(&dock).unwrap(), before, "already there");
    }

    #[test]
    fn a_strip_along_the_whole_window_moves_under_the_drawing_alone() {
        for (side, share) in [(Strip::Below, 0.8), (Strip::Above, 0.2)] {
            let mut dock = default_layout();
            apply(
                &mut dock,
                &mut Places::default(),
                LayoutRequest::Toggle(PanelTab::CommandLine),
            );
            let (tree, tabs) = (dock.main_surface_mut(), vec![PanelTab::CommandLine]);
            match side {
                Strip::Below => tree.split_below(NodeIndex::root(), BELOW_SPLIT, tabs),
                Strip::Above => tree.split_above(NodeIndex::root(), BELOW_SPLIT, tabs),
            };
            // As laid out: the strip 150 points high, the drawing 600.
            let size = egui::vec2(1200.0, 150.0);
            let node = node_of(&dock, PanelTab::CommandLine);
            set_rect(&mut dock, node, size);
            let size = egui::vec2(900.0, 600.0);
            let node = node_of(&dock, PanelTab::Canvas);
            set_rect(&mut dock, node, size);

            tuck_command_line(&mut dock);
            assert_eq!(command_line_strip(&dock), Some(side));
            let (line, canvas) = (
                node_of(&dock, PanelTab::CommandLine),
                node_of(&dock, PanelTab::Canvas),
            );
            assert_eq!(sibling(line), Some(canvas));
            // Properties and Layers are no longer above or below it.
            let column = line.parent().unwrap();
            for panel in [PanelTab::Properties, PanelTab::Layers] {
                let node = node_of(&dock, panel);
                let mut up = std::iter::successors(Some(node), |n| n.parent());
                assert!(!up.any(|n| n == column), "{panel:?}");
            }
            // It keeps its height, which the drawing gives up.
            let top_share = split_shares(&dock)[column.0].unwrap();
            assert!((top_share - share).abs() < 1e-6, "{side:?}");
            assert!(is_valid(&dock));
        }
    }

    #[test]
    fn a_separator_may_shrink_the_command_line_to_three_lines_but_no_other_panel() {
        let mut dock = docked_below();
        let root = NodeIndex::root();
        set_rect(&mut dock, root, egui::vec2(1200.0, 800.0));
        let before = split_shares(&dock);
        // Dragged right down: the command line keeps its least height.
        set_share(&mut dock, root, 0.99);
        keep_minimum_sizes(&mut dock, &before, 80.0);
        let share = split_shares(&dock)[0].unwrap();
        assert!((share - (1.0 - 80.0 / 800.0)).abs() < 1e-6);
        // Dragged right up: what is above keeps the usual least height.
        set_share(&mut dock, root, 0.01);
        keep_minimum_sizes(&mut dock, &before, 80.0);
        let share = split_shares(&dock)[0].unwrap();
        assert!((share - PANEL_MIN / 800.0).abs() < 1e-6);

        // Properties beside the rest: the usual least width on both sides.
        let side = root.left();
        set_rect(&mut dock, side, egui::vec2(1200.0, 600.0));
        let before = split_shares(&dock);
        set_share(&mut dock, side, 0.01);
        keep_minimum_sizes(&mut dock, &before, 80.0);
        let share = split_shares(&dock)[side.0].unwrap();
        assert!((share - PANEL_MIN / 1200.0).abs() < 1e-6);
    }

    #[test]
    fn separators_that_did_not_move_are_left_alone() {
        let mut dock = docked_below();
        set_rect(&mut dock, NodeIndex::root(), egui::vec2(1200.0, 800.0));
        set_share(&mut dock, NodeIndex::root(), 0.99);
        let before = split_shares(&dock);
        keep_minimum_sizes(&mut dock, &before, 80.0);
        assert_eq!(split_shares(&dock), before);
    }

    #[test]
    fn the_strip_collapses_to_its_input_row_without_a_tab_bar() {
        let mut dock = docked_below();
        set_rect(&mut dock, NodeIndex::root(), egui::vec2(1200.0, 800.0));
        let mut saved = None;
        resize_command_line(&mut dock, true, &mut saved, 40.0);
        let share = split_shares(&dock)[0].unwrap();
        assert!((share - (1.0 - 40.0 / 800.0)).abs() < 1e-6);
        resize_command_line(&mut dock, false, &mut saved, 40.0);
        assert_eq!(split_shares(&dock)[0], Some(BELOW_SPLIT));
    }

    #[test]
    fn a_docked_panel_floats_into_a_window_and_stays_open() {
        let mut dock = docked_below();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Float(PanelTab::CommandLine),
        );
        let (surface, ..) = dock.find_tab(&PanelTab::CommandLine).unwrap();
        assert_ne!(surface, egui_dock::SurfaceIndex::main());
        assert!(is_valid(&dock));
        // Asking again, or for the drawing, changes nothing.
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Float(PanelTab::CommandLine),
        );
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Float(PanelTab::Canvas),
        );
        let (canvas, ..) = dock.find_tab(&PanelTab::Canvas).unwrap();
        assert_eq!(canvas, egui_dock::SurfaceIndex::main());
        assert_eq!(dock.iter_surfaces().count(), 2);
    }

    #[test]
    fn toggle_hides_and_restores_a_panel() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Properties),
        );
        assert!(!is_open(&dock, PanelTab::Properties));
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Properties),
        );
        assert!(is_open(&dock, PanelTab::Properties));
    }

    #[test]
    fn command_line_can_be_hidden_and_restored() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::CommandLine),
        );
        assert!(!is_open(&dock, PanelTab::CommandLine));
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::CommandLine),
        );
        assert!(is_open(&dock, PanelTab::CommandLine));
        assert!(is_valid(&dock));
    }

    #[test]
    fn the_assistant_opens_beside_layers_and_can_be_closed() {
        let mut dock = default_layout();
        assert!(
            !is_open(&dock, PanelTab::Assistant),
            "not there until asked for"
        );
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Assistant),
        );
        let layers = dock.find_tab(&PanelTab::Layers).map(|(s, n, _)| (s, n));
        let assistant = dock.find_tab(&PanelTab::Assistant).map(|(s, n, _)| (s, n));
        assert!(assistant.is_some());
        assert_eq!(layers, assistant, "the same tab strip as Layers");
        assert!(is_valid(&dock));
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Assistant),
        );
        assert!(!is_open(&dock, PanelTab::Assistant));
    }

    #[test]
    fn the_assistant_opens_even_when_layers_is_closed() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Layers),
        );
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Assistant),
        );
        assert!(is_open(&dock, PanelTab::Assistant));
        assert!(is_valid(&dock));
    }

    #[test]
    fn a_layout_with_the_assistant_survives_saving() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Assistant),
        );
        let text = ron::to_string(&dock).unwrap();
        let back: DockState<PanelTab> = ron::from_str(&text).unwrap();
        assert!(is_open(&back, PanelTab::Assistant));
    }

    #[test]
    fn the_drawing_has_no_tab_bar_unless_a_panel_shares_its_place() {
        let mut dock = default_layout();
        assert!(drawing_alone(&dock));
        let (surface, node, _) = dock.find_tab(&PanelTab::Canvas).unwrap();
        dock.set_focused_node_and_surface((surface, node));
        dock.push_to_focused_leaf(PanelTab::Assistant);
        assert!(!drawing_alone(&dock));
    }

    #[test]
    fn the_drawing_view_cannot_be_toggled_away() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Canvas),
        );
        assert!(is_open(&dock, PanelTab::Canvas));
    }

    #[test]
    fn reset_restores_the_default() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Properties),
        );
        apply(&mut dock, &mut Places::default(), LayoutRequest::Reset);
        assert!(is_open(&dock, PanelTab::Properties));
    }

    #[test]
    fn layout_round_trips_through_text() {
        let mut dock = default_layout();
        apply(
            &mut dock,
            &mut Places::default(),
            LayoutRequest::Toggle(PanelTab::Properties),
        );
        let text = ron::to_string(&dock).unwrap();
        let back: DockState<PanelTab> = ron::from_str(&text).unwrap();
        assert!(is_valid(&back));
        assert!(!is_open(&back, PanelTab::Properties));
    }

    #[test]
    fn a_layout_without_the_drawing_view_is_rejected() {
        let mut dock = DockState::new(vec![PanelTab::Properties]);
        assert!(!is_valid(&dock));
        dock.push_to_first_leaf(PanelTab::Canvas);
        assert!(is_valid(&dock));
    }
}
