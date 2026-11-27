//! The middle of the window: the blocks of the chosen place, as thumbnails or
//! a list, with the bar for typing a name above them.

use eframe::egui;
use opendraft_content::blocks_payload::folder;
use opendraft_content::library::BlockInfo;
use opendraft_ui::{icons, Theme};

use super::actions::{Action, Taken};
use super::drawing::drawing_menu;
use super::list::{empty_state, list_header, list_row, list_widths};
use super::snapshot::Snapshot;
use super::{Editing, Env, Place, Selected, View, Window};

const TILE: egui::Vec2 = egui::vec2(128.0, 106.0);
const PICTURE: egui::Vec2 = egui::vec2(112.0, 70.0);

impl Window {
    pub(super) fn contents_pane(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        if let Some(problem) = &snap.problem {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(problem);
                ui.weak("Choose the folder on Settings > Files > Block library folder.");
            });
            return;
        }
        if self.import.is_some() {
            self.import_panel(ui, actions);
            return;
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(self.title(snap))
                    .size(theme.font_size_heading)
                    .color(theme.colors.text),
            );
        });
        self.edit_bar(ui, actions);
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if snap.searching || !matches!(self.place, Place::Drawing | Place::Trash) {
                    self.library_items(ui, theme, env, snap, actions);
                } else if self.place == Place::Drawing {
                    self.drawing_items(ui, theme, env, snap, actions);
                } else {
                    self.trash_items(ui, theme, snap, actions);
                }
            });
    }

    fn title(&self, snap: &Snapshot) -> String {
        if snap.searching {
            return match self.search.trim() {
                "" => "Blocks that match the filters".to_owned(),
                words => format!("Search results for \"{words}\""),
            };
        }
        match &self.place {
            Place::Mine(path) if path.is_empty() => "My blocks".to_owned(),
            Place::Mine(path) => format!("My blocks / {}", path.replace('/', " / ")),
            Place::Drawing => "Blocks in this drawing".to_owned(),
            Place::Favourites => "Favourites".to_owned(),
            Place::Recent => "Recent".to_owned(),
            Place::Trash => "Trash (kept for 30 days)".to_owned(),
        }
    }

    /// The row for typing a folder or block name.
    fn edit_bar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let Some(editing) = &mut self.editing else {
            return;
        };
        // A block is renamed in its details, where its name is.
        let (label, text) = match editing {
            Editing::NewFolder { text, .. } => ("New folder name", text),
            Editing::RenameFolder { text, .. } => ("Folder name", text),
            Editing::RenameBlock { .. } | Editing::RenameDrawingBlock { .. } => return,
        };
        let (mut accept, mut cancel) = (false, false);
        ui.horizontal(|ui| {
            ui.label(label);
            let field = ui.add(egui::TextEdit::singleline(text).desired_width(200.0));
            field.request_focus();
            accept = ui.button("OK").clicked()
                || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            cancel =
                ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        if cancel {
            actions.push(Action::Edit(None));
        } else if accept {
            let name = text.trim().to_owned();
            actions.push(match editing {
                Editing::NewFolder { parent, .. } => Action::NewFolder(folder::join(parent, &name)),
                Editing::RenameFolder { path, .. } => {
                    Action::RenameFolder(path.clone(), folder::join(folder::parent(path), &name))
                }
                Editing::RenameBlock { .. } | Editing::RenameDrawingBlock { .. } => return,
            });
        }
    }

    fn library_items(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        let folders: &[String] = if snap.searching {
            &[]
        } else {
            &snap.subfolders
        };
        if folders.is_empty() && snap.shown.is_empty() {
            let text = match &self.place {
                Place::Favourites => "No favourites yet: pick a block and choose Favourite.",
                Place::Recent => "Blocks you insert are listed here.",
                _ => "Nothing here yet. Add blocks with Add to library.",
            };
            empty_state(ui, theme, snap.narrowed, text);
            return;
        }
        match self.view {
            View::Grid => {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                    for path in folders {
                        let go = tile(ui, theme, folder::name(path), false, Picture::Folder);
                        if go.double_clicked() {
                            actions.push(Action::Go(Place::Mine(path.clone())));
                        }
                    }
                    for block in &snap.shown {
                        self.block_tile(ui, theme, env, block, actions);
                    }
                });
            }
            View::List => {
                let widths = list_widths(ui);
                list_header(ui, theme, widths, ["Name", "Folder", "Tags"]);
                for path in folders {
                    let row = list_row(
                        ui,
                        theme,
                        widths,
                        false,
                        [folder::name(path), "Folder", ""],
                        Some(icons::CONTENT_FOLDER),
                    );
                    if row.double_clicked() {
                        actions.push(Action::Go(Place::Mine(path.clone())));
                    }
                }
                for block in &snap.shown {
                    self.block_row(ui, theme, widths, block, actions);
                }
            }
        }
    }

    fn block_tile(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        block: &BlockInfo,
        actions: &mut Vec<Action>,
    ) {
        let selected = self.selected == Selected::Block(block.id);
        let texture = self.thumbs.library_block(ui.ctx(), env, block.id);
        let picture = match &texture {
            Ok(Some(t)) => Picture::Texture(t),
            Ok(None) => Picture::Empty,
            Err(()) => Picture::Pending,
        };
        let label = if block.favourite {
            format!("\u{2605} {}", block.name)
        } else {
            block.name.clone()
        };
        let response = tile(ui, theme, &label, selected, picture);
        self.block_interactions(&response, block, actions);
    }

    fn block_row(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        widths: [f32; 2],
        block: &BlockInfo,
        actions: &mut Vec<Action>,
    ) {
        let selected = self.selected == Selected::Block(block.id);
        let folder = if block.folder.is_empty() {
            "My blocks"
        } else {
            &block.folder
        };
        let icon = block.favourite.then_some(icons::CONTENT_FAVOURITE);
        let tags = block.tags.join(", ");
        let response = list_row(
            ui,
            theme,
            widths,
            selected,
            [&block.name, folder, &tags],
            icon,
        );
        self.block_interactions(&response, block, actions);
    }

    /// Click to choose, double-click to insert, drag to a folder, right-click
    /// for the menu.
    fn block_interactions(
        &self,
        response: &egui::Response,
        block: &BlockInfo,
        actions: &mut Vec<Action>,
    ) {
        if response.clicked() {
            actions.push(Action::Select(Selected::Block(block.id)));
        }
        if response.double_clicked() {
            actions.push(Action::Insert(block.id));
        }
        if response.drag_started() {
            response.dnd_set_drag_payload(block.id);
        }
        let (id, name, favourite) = (block.id, block.name.clone(), block.favourite);
        response.context_menu(|ui| {
            if ui.button("Insert").clicked() {
                actions.push(Action::Insert(id));
                ui.close_menu();
            }
            let star = if favourite {
                "Remove from Favourites"
            } else {
                "Add to Favourites"
            };
            if ui.button(star).clicked() {
                actions.push(Action::Favourite(id, !favourite));
                ui.close_menu();
            }
            if ui.button("Rename").clicked() {
                actions.push(Action::Select(Selected::Block(id)));
                actions.push(Action::Edit(Some(Editing::RenameBlock {
                    id,
                    text: name.clone(),
                })));
                ui.close_menu();
            }
            if ui.button("Delete").clicked() {
                actions.push(Action::Delete(id));
                ui.close_menu();
            }
        });
    }

    fn drawing_items(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        if snap.drawing.is_empty() {
            let plain = "This drawing has no blocks. Make one with BLOCK.";
            empty_state(ui, theme, snap.narrowed, plain);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for block in &snap.drawing {
                let selected = self.selected == Selected::Drawing(block.name.clone());
                let texture = self
                    .thumbs
                    .drawing_block(ui.ctx(), env, &block.name, block.id);
                let picture = match &texture {
                    Ok(Some(t)) => Picture::Texture(t),
                    Ok(None) => Picture::Empty,
                    Err(()) => Picture::Pending,
                };
                let response = tile(ui, theme, &block.name, selected, picture);
                if response.clicked() {
                    actions.push(Action::Select(Selected::Drawing(block.name.clone())));
                }
                let name = block.name.clone();
                if response.double_clicked() {
                    actions.push(Action::AddFromDrawing(name.clone(), Taken::Cancel));
                }
                response.context_menu(|ui| drawing_menu(ui, block.id, &name, actions));
            }
        });
    }

    fn trash_items(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        if snap.trash.is_empty() {
            empty_state(ui, theme, snap.narrowed, "The Trash is empty.");
            return;
        }
        if ui.button("Empty Trash").clicked() {
            actions.push(Action::EmptyTrash);
        }
        ui.add_space(4.0);
        let widths = list_widths(ui);
        list_header(ui, theme, widths, ["Name", "Deleted from", ""]);
        for item in &snap.trash {
            let selected = self.selected == Selected::Trashed(item.id);
            let from = if item.folder.is_empty() {
                "My blocks"
            } else {
                &item.folder
            };
            let response = list_row(ui, theme, widths, selected, [&item.name, from, ""], None);
            if response.clicked() {
                actions.push(Action::Select(Selected::Trashed(item.id)));
            }
            if response.double_clicked() {
                actions.push(Action::Restore(item.id));
            }
        }
    }
}

/// What a tile shows above its name.
enum Picture<'a> {
    Texture(&'a egui::TextureHandle),
    /// Nothing to draw (an empty block, or one too detailed).
    Empty,
    /// Not made yet this frame.
    Pending,
    Folder,
}

/// One block or folder: a picture over a name, highlighted when chosen.
fn tile(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    selected: bool,
    picture: Picture,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(TILE, egui::Sense::click_and_drag());
    let c = &theme.colors;
    let fill = if selected {
        c.accent.gamma_multiply(0.45)
    } else if response.hovered() {
        c.hover
    } else {
        c.panel_raised
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, fill);
    if selected {
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0_f32, c.accent),
            egui::StrokeKind::Inside,
        );
    }
    let area = egui::Rect::from_min_size(
        rect.min + egui::vec2((TILE.x - PICTURE.x) / 2.0, 6.0),
        PICTURE,
    );
    match picture {
        Picture::Texture(texture) => {
            painter.image(
                texture.id(),
                area,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        Picture::Folder => {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "\u{1F4C1}",
                egui::FontId::proportional(36.0),
                c.text_weak,
            );
        }
        Picture::Empty | Picture::Pending => {
            painter.rect_filled(area, 2.0, c.canvas_bg);
            let note = if matches!(picture, Picture::Empty) {
                "no preview"
            } else {
                ""
            };
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                note,
                egui::FontId::proportional(theme.font_size_small),
                c.text_weak,
            );
        }
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(theme.font_size_small + 1.0),
        c.text,
    );
    job.wrap = egui::text::TextWrapping {
        max_width: TILE.x - 10.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = painter.layout_job(job);
    let at = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.max.y - galley.size().y - 6.0,
    );
    painter.galley(at, galley, c.text);
    response.on_hover_text(label)
}
