//! The Content Manager window: the user's block library beside the drawing.
//! A tree on the left (My blocks and its folders, This drawing, Favourites,
//! Recent, Trash), the blocks of the chosen place as thumbnails or a list, and
//! the chosen block's details below. It is modeless: it stays open while the
//! user draws. The state of the library is `opendraft_content::library`; this
//! module only shows it and turns clicks into actions on it.
//!
//! Each frame the window takes a [`snapshot::Snapshot`] of what it shows, draws
//! from that, and collects [`actions::Action`]s, which are carried out after
//! drawing, so no drawing code changes the library halfway through a frame.

use eframe::egui;
use opendraft_content::blocks_payload::BlockUuid;
use opendraft_core::{BlockId, Document};
use opendraft_ui::{icons, Theme};

use crate::content_library::LibraryHandle;

mod actions;
mod chrome;
mod contents;
mod details;
mod drawing;
mod filter;
mod import;
mod list;
mod name_edit;
mod snapshot;
mod thumbnails;
mod tree;
mod unit_menu;

use actions::Action;
use snapshot::Snapshot;

/// What the window is showing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// My blocks, in this folder (`""` for the top).
    Mine(String),
    Drawing,
    Favourites,
    Recent,
    Trash,
}

impl Default for Place {
    fn default() -> Self {
        Place::Mine(String::new())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Grid,
    List,
}

/// What is chosen in the contents pane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selected {
    #[default]
    Nothing,
    Block(BlockUuid),
    /// A block of the open drawing, by name.
    Drawing(String),
    Trashed(u64),
}

/// A name being typed: a new folder's, or a folder's, a library block's or
/// a block of the open drawing's new name.
#[derive(Clone, Debug)]
pub enum Editing {
    NewFolder { parent: String, text: String },
    RenameFolder { path: String, text: String },
    RenameBlock { id: BlockUuid, text: String },
    RenameDrawingBlock { id: BlockId, text: String },
}

/// What the window asks of the app, which owns the commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Start LIBINSERT on this block.
    Insert(BlockUuid),
    /// Start LIBADD (on the selected objects, if any).
    AddObjects,
    /// Ask for a drawing file and offer its blocks for import.
    PickImport,
    /// Rename a block of the open drawing (one undo step); the answer comes
    /// back through [`Window::drawing_block_renamed`].
    RenameDrawingBlock(BlockId, String),
    /// Put a block of the open drawing in another unit (one undo step); the
    /// answer comes back through [`Window::drawing_block_changed`].
    SetDrawingBlockUnit(BlockId, opendraft_core::InsertUnits),
}

/// What the window reads of the app.
pub struct Env<'a> {
    pub library: &'a LibraryHandle,
    pub document: &'a Document,
    /// Changes whenever the drawing does, so pictures of its blocks are redrawn.
    pub revision: u64,
}

#[derive(Default)]
pub struct Window {
    pub open: bool,
    place: Place,
    back: Vec<Place>,
    forward: Vec<Place>,
    search: String,
    view: View,
    selected: Selected,
    editing: Option<Editing>,
    status: String,
    /// A block of the drawing whose name the library already has, waiting
    /// for the user to say what to do.
    taken: Option<String>,
    thumbs: thumbnails::Thumbnails,
    details: details::Buffers,
    filter: filter::Filter,
    import: Option<import::ImportDraft>,
    /// The list of units of a drawing block's details is open.
    unit_menu: bool,
}

impl Window {
    /// Puts a line in the status bar.
    pub fn say(&mut self, text: String) {
        self.status = text;
        self.open = true;
    }

    /// Takes the app's answer to [`Request::RenameDrawingBlock`]: the old
    /// name (`None` when it was unchanged) and the drawing's revisions before
    /// and after. The name bar closes and the block stays chosen under its
    /// new name, with its picture; or the bar stays open with the reason in
    /// the status line.
    pub fn drawing_block_renamed(
        &mut self,
        result: Result<Option<String>, String>,
        name: &str,
        revisions: (u64, u64),
    ) {
        let name = name.trim();
        match result {
            Ok(renamed) => {
                self.editing = None;
                self.selected = Selected::Drawing(name.to_owned());
                if let Some(old) = renamed {
                    self.thumbs.drawing_block_renamed(&old, name, revisions);
                    self.status = format!("Block \"{old}\" renamed to \"{name}\".");
                }
            }
            Err(why) => self.status = why,
        }
    }

    /// Opens the window on This drawing with block `id` being renamed, as
    /// its pencil does.
    #[cfg(test)]
    pub(crate) fn start_drawing_rename(&mut self, id: BlockId, name: &str) {
        self.open_on(Place::Drawing);
        self.selected = Selected::Drawing(name.to_owned());
        self.editing = Some(Editing::RenameDrawingBlock {
            id,
            text: name.to_owned(),
        });
    }

    /// Whether the window has the keys: a name is being typed (a block's or
    /// a folder's), or the list of units is open.
    pub fn typing_a_name(&self) -> bool {
        self.open && (self.editing.is_some() || self.unit_menu)
    }

    /// Takes the app's answer to [`Request::SetDrawingBlockUnit`]: what to
    /// say (`None` when nothing changed) and the drawing's revisions before
    /// and after. No picture shows a unit, so the pictures are kept.
    pub fn drawing_block_changed(
        &mut self,
        result: Result<Option<String>, String>,
        revisions: (u64, u64),
    ) {
        match result {
            Ok(Some(said)) => {
                self.thumbs.drawing_unchanged(revisions);
                self.status = said;
            }
            Ok(None) => {}
            Err(why) => self.status = why,
        }
    }

    /// Opens the window on This drawing with block `name` chosen.
    #[cfg(test)]
    pub(crate) fn show_drawing_block(&mut self, name: &str) {
        self.open_on(Place::Drawing);
        self.selected = Selected::Drawing(name.to_owned());
    }

    /// Whether a block's name field is open.
    #[cfg(test)]
    pub(crate) fn renaming(&self) -> bool {
        matches!(
            self.editing,
            Some(Editing::RenameBlock { .. } | Editing::RenameDrawingBlock { .. })
        )
    }

    /// Opens the window on `place`.
    pub fn open_on(&mut self, place: Place) {
        self.open = true;
        self.go(place);
    }

    fn go(&mut self, place: Place) {
        if self.place != place {
            let from = std::mem::replace(&mut self.place, place);
            self.back.push(from);
            self.forward.clear();
        }
        self.selected = Selected::Nothing;
        self.editing = None;
    }

    /// Draws the window if it is open and says what the user asked the app to do.
    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme, env: &Env) -> Vec<Request> {
        if !self.open {
            return Vec::new();
        }
        let snapshot = Snapshot::take(self, env);
        self.unit_menu = false;
        self.thumbs.keep_current(snapshot.stamp, env.revision);
        let mut actions = Vec::new();
        let mut open = true;
        let c = &theme.colors;
        let frame = egui::Frame::new()
            .fill(c.panel_bg)
            .stroke(egui::Stroke::new(1.0_f32, c.border))
            .corner_radius(egui::CornerRadius::same(theme.radius + 3))
            .shadow(ctx.style().visuals.window_shadow);
        egui::Window::new("Content Manager")
            .id(egui::Id::new("content_manager_window"))
            .open(&mut open)
            .frame(frame)
            .default_size([840.0, 560.0])
            .min_size([560.0, 340.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| self.ui(ui, theme, env, &snapshot, &mut actions));
        self.open = open;
        self.apply(actions, env)
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        egui::TopBottomPanel::top("cm_toolbar").show_inside(ui, |ui| {
            self.toolbar(ui, theme, &snap.units, actions);
        });
        egui::TopBottomPanel::bottom("cm_status").show_inside(ui, |ui| {
            self.status_bar(ui, snap, actions);
        });
        egui::SidePanel::left("cm_tree")
            .default_width(190.0)
            .width_range(130.0..=360.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    tree::show(ui, theme, &self.place, snap, actions);
                });
            });
        egui::TopBottomPanel::bottom("cm_details")
            .exact_height(150.0)
            .resizable(false)
            .frame(room(ui, 4))
            .show_inside(ui, |ui| {
                self.details_pane(ui, theme, env, snap, actions);
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show_inside(ui, |ui| {
                self.contents_pane(ui, theme, env, snap, actions);
            });
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        units: &[opendraft_core::InsertUnits],
        actions: &mut Vec<Action>,
    ) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let up = matches!(&self.place, Place::Mine(p) if !p.is_empty());
            let nav = [
                (
                    icons::CONTENT_BACK,
                    !self.back.is_empty(),
                    "Back",
                    Action::Back,
                ),
                (
                    icons::CONTENT_FORWARD,
                    !self.forward.is_empty(),
                    "Forward",
                    Action::Forward,
                ),
                (icons::CONTENT_UP, up, "Up one level", Action::Up),
            ];
            for (icon, enabled, tip, action) in nav {
                if chrome::icon_button(ui, theme, icon, false, enabled, tip).clicked() {
                    actions.push(action);
                }
            }
            ui.add_space(6.0);
            ui.allocate_ui(egui::vec2(240.0, 24.0), |ui| {
                opendraft_ui::widgets::text_field(ui, theme, &mut self.search, "Search blocks");
            });
            self.filter.menu(ui, theme, units);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Import")
                    .on_hover_text("Add blocks from a drawing file: .odraft, DXF or DWG")
                    .clicked()
                {
                    actions.push(Action::ImportFile);
                }
                if ui
                    .button("Add to library")
                    .on_hover_text("Add the selected objects, or a block of this drawing (LIBADD)")
                    .clicked()
                {
                    actions.push(Action::AddObjects);
                }
                ui.add_space(6.0);
                chrome::segmented(
                    ui,
                    theme,
                    &mut self.view,
                    &[
                        (View::List, icons::HOME_VIEW_LIST, "List"),
                        (View::Grid, icons::HOME_VIEW_GRID, "Thumbnails"),
                    ],
                );
            });
        });
        ui.add_space(2.0);
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, snap: &Snapshot, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            if let Some(name) = self.taken.clone() {
                ui.label(format!("The library already has \"{name}\":"));
                if ui.button("Keep both").clicked() {
                    actions.push(Action::AddFromDrawing(
                        name.clone(),
                        actions::Taken::KeepBoth,
                    ));
                }
                if ui.button("Replace it").clicked() {
                    actions.push(Action::AddFromDrawing(
                        name.clone(),
                        actions::Taken::Replace,
                    ));
                }
                if ui.button("Cancel").clicked() {
                    actions.push(Action::DropOffer);
                }
            } else if self.status.is_empty() {
                ui.weak(snap.count_text(self));
            } else {
                ui.label(&self.status);
            }
        });
    }
}

/// A panel's usual frame with a little room round what is in it.
fn room(ui: &egui::Ui, vertical: i8) -> egui::Frame {
    egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, vertical))
}

#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod tests;
