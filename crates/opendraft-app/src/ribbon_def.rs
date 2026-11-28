//! The application's ribbon layout. Items name command ids; what they do is
//! decided elsewhere (the command registry arrives in Phase 3).
//!
//! The usual tabs are always there, and a contextual tab follows them while
//! the drawing on screen is in the Block Editor or a layout
//! ([`RibbonContext`]). The Block Editor's opens on its tab, as in AutoCAD:
//! its Save and Close are how one leaves it. The Layout tab never takes the
//! focus, unlike AutoCAD's (which does unless set to "Full without focus"):
//! it holds set-up tools used now and then, while the work in a layout is done
//! on the usual tabs, so being moved off them would only get in the way.

use opendraft_ui::icons;
use opendraft_ui::ribbon::{Column, Item, Panel, Ribbon, Tab};

use crate::state::AppState;

mod home;
mod layout;

pub use layout::LAYOUT_TAB;

/// The title of the contextual tab shown while the Block Editor is open.
pub const BLOCK_EDITOR_TAB: &str = "Block Editor";

/// What the drawing on screen is doing, which decides the contextual tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonContext {
    /// Model space, or no drawing on screen (the Home page).
    Normal,
    Layout,
    BlockEditor,
}

impl RibbonContext {
    pub fn of(state: &AppState) -> Self {
        if state.home {
            Self::Normal
        } else if state.in_block_editor() {
            Self::BlockEditor
        } else if state.space().layout().is_some() {
            Self::Layout
        } else {
            Self::Normal
        }
    }

    /// The context `ribbon` was made for, told by its contextual tab.
    pub fn shown(ribbon: &Ribbon) -> Self {
        match ribbon.tabs.last().filter(|t| t.contextual).map(|t| t.title) {
            Some(BLOCK_EDITOR_TAB) => Self::BlockEditor,
            Some(LAYOUT_TAB) => Self::Layout,
            _ => Self::Normal,
        }
    }
}

pub fn default_ribbon() -> Ribbon {
    ribbon_for(RibbonContext::Normal)
}

/// The usual tabs and, last, the contextual tab of `context`, if it has one.
pub fn ribbon_for(context: RibbonContext) -> Ribbon {
    let mut tabs = vec![home::home(), insert(), annotate(), view(), manage()];
    match context {
        RibbonContext::Normal => {}
        RibbonContext::Layout => tabs.push(layout::layout()),
        RibbonContext::BlockEditor => tabs.push(block_editor()),
    }
    Ribbon { tabs }
}

/// The tab to show once the ribbon changes from `from` to `to` (made for
/// `context`): the Block Editor's own tab; otherwise the tab that was showing,
/// if `to` still has it, else Home.
pub fn tab_after_change(
    active: usize,
    from: &Ribbon,
    to: &Ribbon,
    context: RibbonContext,
) -> usize {
    if context == RibbonContext::BlockEditor {
        return to.tabs.len().saturating_sub(1);
    }
    from.tabs
        .get(active)
        .and_then(|shown| to.tabs.iter().position(|t| t.title == shown.title))
        .unwrap_or(0)
}

fn block_editor() -> Tab {
    Tab {
        title: BLOCK_EDITOR_TAB,
        panels: vec![
            Panel {
                title: "Open/Save",
                columns: vec![Column::Large(Item::new(
                    "block.save",
                    "Save Block",
                    icons::BLOCK_SAVE,
                ))],
                ..Default::default()
            },
            Panel {
                title: "Dynamic",
                columns: vec![
                    Column::Large(Item::new(
                        "block.parameter",
                        "Parameter",
                        icons::BLOCK_PARAMETER,
                    )),
                    Column::Large(Item::new("block.action", "Action", icons::BLOCK_ACTION)),
                    Column::Large(Item::new(
                        "block.actionset",
                        "Action Objects",
                        icons::BLOCK_ACTIONSET,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Close",
                columns: vec![Column::Large(Item::new(
                    "block.close",
                    "Close Block Editor",
                    icons::BLOCK_CLOSE,
                ))],
                ..Default::default()
            },
        ],
        contextual: true,
    }
}

fn insert() -> Tab {
    Tab {
        title: "Insert",
        panels: vec![
            Panel {
                title: "Block",
                columns: vec![
                    Column::Large(Item::new("block.insert", "Insert", icons::BLOCK_INSERT)),
                    Column::Large(Item::new("block.create", "Create", icons::BLOCK_CREATE)),
                    Column::Large(Item::new("block.edit", "Edit", icons::BLOCK_EDIT)),
                    Column::Large(Item::new("block.purge", "Purge", icons::BLOCK_PURGE)),
                    Column::Large(Item::new(
                        "block.content_manager",
                        "Content Manager",
                        icons::BLOCK_LIBRARY,
                    )),
                    Column::Large(Item::new("block.reset", "Reset", icons::BLOCK_RESET)),
                ],
                ..Default::default()
            },
            Panel {
                title: "Reference",
                columns: vec![
                    Column::Large(Item::new("xref.attach", "Attach", icons::BLOCK_INSERT)),
                    Column::Large(Item::new("xref.clip", "Clip", icons::BLOCK_EDIT)),
                    Column::Large(Item::new(
                        "xref.palette",
                        "External References",
                        icons::BLOCK_LIBRARY,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Attributes",
                columns: vec![
                    Column::Large(Item::new(
                        "block.attdef",
                        "Define Attributes",
                        icons::BLOCK_ATTDEF,
                    )),
                    Column::Large(Item::new(
                        "block.attedit",
                        "Edit Attributes",
                        icons::BLOCK_ATTEDIT,
                    )),
                    Column::Large(Item::new(
                        "block.attsync",
                        "Synchronize",
                        icons::BLOCK_ATTSYNC,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Data",
                columns: vec![
                    Column::Large(Item::new("block.field", "Field", icons::INSERT_FIELD)),
                    Column::Large(Item::new(
                        "block.updatefield",
                        "Update Fields",
                        icons::INSERT_UPDATEFIELD,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Import",
                columns: vec![
                    Column::Large(Item::new(
                        "insert.import",
                        "Import File",
                        icons::INSERT_IMPORT,
                    )),
                    Column::Large(Item::new(
                        "insert.export",
                        "Export File",
                        icons::INSERT_EXPORT,
                    )),
                ],
                ..Default::default()
            },
        ],
        contextual: false,
    }
}

fn annotate() -> Tab {
    Tab {
        title: "Annotate",
        panels: vec![
            Panel {
                title: "Annotation",
                columns: vec![
                    Column::Large(Item::new(
                        "annotate.mtext",
                        "Multiline Text",
                        icons::ANNOTATE_MTEXT,
                    )),
                    Column::Large(Item::new(
                        "annotate.text",
                        "Single Line Text",
                        icons::ANNOTATE_TEXT,
                    )),
                    Column::Large(Item::new(
                        "annotate.mleader",
                        "Multileader",
                        icons::ANNOTATE_MLEADER,
                    )),
                    Column::Large(Item::new(
                        "annotate.leader",
                        "Leader",
                        icons::ANNOTATE_LEADER,
                    )),
                    Column::Large(Item::new(
                        "annotate.textstyle",
                        "Text Style",
                        icons::ANNOTATE_TEXTSTYLE,
                    )),
                    Column::Large(Item::new(
                        "annotate.mleaderstyle",
                        "Multileader Style",
                        icons::ANNOTATE_MLEADERSTYLE,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Dimensions",
                columns: vec![
                    Column::Large(Item::new("dim.linear", "Linear", icons::DIM_LINEAR)),
                    Column::Large(Item::new("dim.aligned", "Aligned", icons::DIM_ALIGNED)),
                    Column::Large(Item::new("dim.angular", "Angular", icons::DIM_ANGULAR)),
                    Column::Stack(vec![
                        Item::new("dim.radius", "Radius", icons::DIM_RADIUS),
                        Item::new("dim.diameter", "Diameter", icons::DIM_DIAMETER),
                        Item::new("dim.arc", "Arc Length", icons::DIM_ARC),
                    ]),
                    Column::Stack(vec![
                        Item::new("dim.jogged", "Jogged", icons::DIM_JOGGED),
                        Item::new("dim.ordinate", "Ordinate", icons::DIM_ORDINATE),
                        Item::new("dim.centre", "Center Mark", icons::DIM_CENTRE),
                    ]),
                    Column::Stack(vec![
                        Item::new("dim.baseline", "Baseline", icons::DIM_BASELINE),
                        Item::new("dim.continue", "Continue", icons::DIM_CONTINUE),
                        Item::new("dim.space", "Space", icons::DIM_SPACE),
                    ]),
                    Column::Stack(vec![
                        Item::new("dim.edit", "Edit", icons::DIM_EDIT),
                        Item::new("dim.textedit", "Text Edit", icons::DIM_TEXTEDIT),
                        Item::new("dim.jogline", "Jog Line", icons::DIM_JOGLINE),
                    ]),
                    Column::Stack(vec![
                        Item::new("dim.reassociate", "Reassociate", icons::DIM_REASSOCIATE),
                        Item::new("dim.disassociate", "Disassociate", icons::DIM_DISASSOCIATE),
                    ]),
                    Column::Large(Item::new("dim.style", "Dimension Style", icons::DIM_STYLE)),
                ],
                ..Default::default()
            },
        ],
        contextual: false,
    }
}

fn view() -> Tab {
    Tab {
        title: "View",
        panels: vec![
            Panel {
                title: "Navigate",
                columns: vec![
                    Column::Large(Item::new("view.pan", "Pan", icons::VIEW_PAN)),
                    Column::Large(Item::new(
                        "view.zoom_extents",
                        "Zoom Extents",
                        icons::VIEW_ZOOM_EXTENTS,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Coordinates",
                columns: vec![
                    Column::Large(Item::new("view.ccs", "Coordinate System", icons::VIEW_CCS)),
                    Column::Stack(vec![
                        Item::new("view.ccs_world", "World", icons::VIEW_CCS_WORLD),
                        Item::new("view.ccs_previous", "Previous", icons::VIEW_CCS_PREVIOUS),
                    ]),
                ],
                ..Default::default()
            },
            Panel {
                title: "Drafting Aids",
                columns: vec![Column::Stack(vec![
                    Item::new("view.grid", "Grid", icons::VIEW_GRID),
                    Item::new("view.snap", "Snap", icons::VIEW_SNAP),
                    Item::new("view.nav_gizmo", "Gizmo", icons::VIEW_NAV_GIZMO),
                ])],
                ..Default::default()
            },
            Panel {
                title: "Palettes",
                columns: vec![
                    Column::Large(Item::new(
                        "view.panel_properties",
                        "Properties",
                        icons::VIEW_PANEL_PROPERTIES,
                    )),
                    Column::Large(Item::new(
                        "view.panel_layers",
                        "Layer Properties",
                        icons::LAYER_PROPERTIES,
                    )),
                    Column::Large(Item::new(
                        "view.command_line",
                        "Command Line",
                        icons::VIEW_COMMAND_LINE,
                    )),
                    Column::Large(Item::new(
                        "view.assistant",
                        "Assistant",
                        icons::VIEW_ASSISTANT,
                    )),
                    Column::Large(Item::new(
                        "view.reset_layout",
                        "Reset Layout",
                        icons::VIEW_RESET_LAYOUT,
                    )),
                    Column::Large(Item::new(
                        "view.bring_windows_back",
                        "Bring Windows Back",
                        icons::VIEW_BRING_WINDOWS_BACK,
                    )),
                ],
                ..Default::default()
            },
        ],
        contextual: false,
    }
}

fn manage() -> Tab {
    Tab {
        title: "Manage",
        panels: vec![
            Panel {
                title: "Drawing",
                columns: vec![
                    Column::Large(Item::new("format.units", "Units", icons::FORMAT_UNITS)),
                    Column::Large(Item::new(
                        "format.properties",
                        "Properties",
                        icons::FORMAT_PROPERTIES,
                    )),
                    Column::Large(Item::new(
                        "format.linetype",
                        "Linetypes",
                        icons::FORMAT_LINETYPE,
                    )),
                ],
                ..Default::default()
            },
            Panel {
                title: "Customization",
                columns: vec![Column::Large(Item::new(
                    "manage.settings",
                    "Settings",
                    icons::MANAGE_SETTINGS,
                ))],
                ..Default::default()
            },
        ],
        contextual: false,
    }
}

/// Every command id on `tab`, in its panels and their flyouts.
#[cfg(test)]
fn commands(tab: &Tab) -> Vec<&'static str> {
    use opendraft_ui::ribbon::Cell;
    let mut ids = Vec::new();
    for panel in &tab.panels {
        for column in &panel.columns {
            match column {
                Column::Large(item) => ids.push(item.command),
                Column::LargeSplit(split) => ids.extend(split.items.iter().map(|i| i.command)),
                Column::Stack(items) => ids.extend(items.iter().map(|i| i.command)),
                Column::Rows(rows) => {
                    for cell in rows.iter().flatten() {
                        match cell {
                            Cell::Button(item) | Cell::Icon(item) => ids.push(item.command),
                            Cell::Split(split) | Cell::IconSplit(split) => {
                                ids.extend(split.items.iter().map(|i| i.command))
                            }
                            Cell::Slot { .. } => {}
                        }
                    }
                }
            }
        }
        ids.extend(panel.extra.iter().map(|i| i.command));
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_cmd::define_block;
    use crate::commands::build_registry;
    use opendraft_core::{Entity, Point};
    use RibbonContext::{BlockEditor, Layout, Normal};

    fn titles(ribbon: &Ribbon) -> Vec<&'static str> {
        ribbon.tabs.iter().map(|t| t.title).collect()
    }

    #[test]
    fn a_contextual_tab_follows_the_usual_ones_and_only_it_is_marked() {
        let usual = titles(&default_ribbon());
        assert_eq!(usual, ["Home", "Insert", "Annotate", "View", "Manage"]);
        for (context, extra) in [
            (Normal, None),
            (Layout, Some(LAYOUT_TAB)),
            (BlockEditor, Some(BLOCK_EDITOR_TAB)),
        ] {
            let ribbon = ribbon_for(context);
            let mut expected = usual.clone();
            expected.extend(extra);
            assert_eq!(titles(&ribbon), expected);
            let marked: Vec<&str> = ribbon
                .tabs
                .iter()
                .filter(|t| t.contextual)
                .map(|t| t.title)
                .collect();
            assert_eq!(marked, Vec::from_iter(extra));
            assert_eq!(RibbonContext::shown(&ribbon), context);
        }
    }

    #[test]
    fn every_contextual_item_runs_a_command() {
        let registry = build_registry();
        for context in [Layout, BlockEditor] {
            let ribbon = ribbon_for(context);
            let tab = ribbon.tabs.last().expect("a contextual tab");
            let ids = commands(tab);
            assert!(!ids.is_empty());
            for id in ids {
                assert!(registry.get(id).is_some(), "{}: {id}", tab.title);
            }
        }
    }

    #[test]
    fn the_layout_tab_has_autocads_panels() {
        let ribbon = ribbon_for(Layout);
        let tab = ribbon.tabs.last().expect("the Layout tab");
        let panels: Vec<&str> = tab.panels.iter().map(|p| p.title).collect();
        assert_eq!(panels, ["Layout", "Layout Viewports", "Display"]);
    }

    #[test]
    fn the_layout_tab_comes_and_goes_without_moving_the_ribbon() {
        let (normal, layout) = (ribbon_for(Normal), ribbon_for(Layout));
        for active in 0..normal.tabs.len() {
            assert_eq!(tab_after_change(active, &normal, &layout, Layout), active);
            assert_eq!(tab_after_change(active, &layout, &normal, Normal), active);
        }
        // Leaving a layout while on its tab: that tab is gone, so Home.
        let on_layout = layout.tabs.len() - 1;
        assert_eq!(tab_after_change(on_layout, &layout, &normal, Normal), 0);
    }

    #[test]
    fn the_block_editor_opens_on_its_tab_and_leaves_the_others_be() {
        let (normal, editor) = (ribbon_for(Normal), ribbon_for(BlockEditor));
        let on_editor = editor.tabs.len() - 1;
        assert_eq!(
            tab_after_change(2, &normal, &editor, BlockEditor),
            on_editor
        );
        assert_eq!(tab_after_change(on_editor, &editor, &normal, Normal), 0);
        // Moved to another tab while editing: closing keeps it.
        assert_eq!(tab_after_change(3, &editor, &normal, Normal), 3);
    }

    #[test]
    fn the_context_is_the_one_of_the_drawing_on_screen() {
        let mut state = AppState::new();
        assert_eq!(RibbonContext::of(&state), Normal);
        state.new_layout();
        assert_eq!(RibbonContext::of(&state), Layout);
        state.mspace();
        assert!(state.space().viewport().is_some(), "inside the viewport");
        assert_eq!(RibbonContext::of(&state), Layout);
        state.home = true;
        assert_eq!(RibbonContext::of(&state), Normal, "the Home page");
        state.home = false;
        state.enter_model();
        assert_eq!(RibbonContext::of(&state), Normal);

        state.add_entity(Entity::Rectangle {
            min: Point::new(0.0, 0.0),
            max: Point::new(2.0, 1.0),
        });
        state.select_all();
        state.start_command("BLOCK", define_block);
        state.settle_command();
        for answer in ["Box", "0,0"] {
            state.cmd_submit(answer);
            state.settle_command();
        }
        state.select_all();
        state.bedit();
        assert!(state.in_block_editor());
        assert_eq!(RibbonContext::of(&state), BlockEditor);
    }

    #[test]
    fn the_lock_button_shows_the_lock_of_the_viewport_it_acts_on() {
        let registry = build_registry();
        let mut state = AppState::new();
        state.new_layout();
        assert!(!registry.is_checked("layout.vplock", &state), "none picked");
        state.mspace();
        assert!(!registry.is_checked("layout.vplock", &state));
        registry.execute("layout.vplock", &mut state).unwrap();
        assert!(registry.is_checked("layout.vplock", &state));
        registry.execute("layout.vplock", &mut state).unwrap();
        assert!(!registry.is_checked("layout.vplock", &state));
    }
}
