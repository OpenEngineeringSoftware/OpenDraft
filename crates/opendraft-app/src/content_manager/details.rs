//! The details of the chosen block, below the contents: its facts, a
//! description and tags that can be edited, and what can be done with it.

use eframe::egui;
use opendraft_content::blocks_payload::BlockUuid;
use opendraft_content::library::BlockInfo;
use opendraft_ui::Theme;

use super::actions::{Action, Taken};
use super::name_edit::{name_line, NameEdit};
use super::snapshot::{DrawingBlock, Snapshot};
use super::unit_menu::{unit_text, unit_with_pencil};
use super::{Editing, Env, Selected, Window};

/// Width of the names beside the fields.
const LABEL: f32 = 74.0;

/// A name beside a field, on the left of its own width.
fn label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(LABEL, 18.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| ui.label(text),
    );
}

/// The text being edited for the chosen block.
#[derive(Default)]
pub struct Buffers {
    id: Option<BlockUuid>,
    description: String,
    tags: String,
}

impl Buffers {
    /// Starts again from `block` when another one is chosen or its text was
    /// changed from elsewhere while the field was not being typed in.
    fn follow(&mut self, block: &BlockInfo, typing: bool) {
        if self.id != Some(block.id) {
            self.id = Some(block.id);
            self.description.clone_from(&block.description);
            self.tags = block.tags.join(", ");
        } else if !typing {
            if self.description != block.description {
                self.description.clone_from(&block.description);
            }
            let tags = block.tags.join(", ");
            if self.tags != tags {
                self.tags = tags;
            }
        }
    }
}

/// `"a, b ,c"` as tags.
fn parse_tags(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

impl Window {
    pub(super) fn details_pane(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        snap: &Snapshot,
        actions: &mut Vec<Action>,
    ) {
        ui.add_space(4.0);
        match self.selected.clone() {
            Selected::Block(_) => match &snap.selected {
                Some(block) => self.block_details(ui, theme, env, block, actions),
                None => {
                    ui.weak("That block is no longer in the library.");
                }
            },
            Selected::Drawing(name) => match snap.drawing.iter().find(|b| b.name == name) {
                Some(block) => self.drawing_details(ui, theme, block, actions),
                None => {
                    ui.weak("That block is no longer in the drawing.");
                }
            },
            Selected::Trashed(id) => {
                if let Some(item) = snap.trash.iter().find(|t| t.id == id) {
                    ui.strong(&item.name);
                    let from = if item.folder.is_empty() {
                        "My blocks"
                    } else {
                        &item.folder
                    };
                    ui.label(format!("Deleted from {from}. Kept for 30 days."));
                    if ui.button("Restore").clicked() {
                        actions.push(Action::Restore(id));
                    }
                }
            }
            Selected::Nothing => {
                ui.weak("Choose a block to see its details. Double-click a block to insert it.");
            }
        }
    }

    fn drawing_details(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        block: &DrawingBlock,
        actions: &mut Vec<Action>,
    ) {
        let typing = match &mut self.editing {
            Some(Editing::RenameDrawingBlock { id, text }) if *id == block.id => Some(text),
            _ => None,
        };
        let mut chosen = None;
        let unit = |ui: &mut egui::Ui| {
            let (unit, open) = unit_with_pencil(ui, theme, block.unit);
            (chosen, self.unit_menu) = (unit, open);
            ui.weak(extras(block.dynamic, block.has_attributes));
        };
        let asked = name_line(ui, theme, &block.name, unit, typing);
        if let Some(unit) = chosen {
            actions.push(Action::SetDrawingBlockUnit(block.id, unit));
        }
        match asked {
            NameEdit::Nothing => {}
            NameEdit::Start => actions.extend(super::drawing::rename(block.id, &block.name)),
            NameEdit::Accept(name) => actions.push(Action::RenameDrawingBlock(block.id, name)),
            NameEdit::Cancel => actions.push(Action::Edit(None)),
        }
        ui.label(format!(
            "A block of this drawing, used {}.",
            crate::text::plural(block.references, "time")
        ));
        if block.in_library {
            ui.weak("The library has a block of this name already.");
        }
        if ui.button("Add to library").clicked() {
            actions.push(Action::AddFromDrawing(block.name.clone(), Taken::Cancel));
        }
    }

    fn block_details(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        env: &Env,
        block: &BlockInfo,
        actions: &mut Vec<Action>,
    ) {
        let typing = ui.memory(|m| m.focused().is_some());
        self.details.follow(block, typing);
        ui.horizontal_top(|ui| {
            self.preview(ui, theme, env, block);
            ui.vertical(|ui| self.block_facts(ui, theme, block, actions));
        });
    }

    /// The block's picture, at the left of its details.
    fn preview(&mut self, ui: &mut egui::Ui, theme: &Theme, env: &Env, block: &BlockInfo) {
        let size = egui::vec2(120.0, 90.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, theme.colors.panel_raised);
        match self.thumbs.library_block(ui.ctx(), env, block.id) {
            Ok(Some(texture)) => {
                let inner = rect.shrink(4.0);
                ui.painter().image(
                    texture.id(),
                    inner,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            Ok(None) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "no preview",
                    egui::FontId::proportional(theme.font_size_small),
                    theme.colors.text_weak,
                );
            }
            Err(()) => {}
        }
    }

    fn block_facts(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        block: &BlockInfo,
        actions: &mut Vec<Action>,
    ) {
        let typing = match &mut self.editing {
            Some(Editing::RenameBlock { id, text }) if *id == block.id => Some(text),
            _ => None,
        };
        let line = format!(
            "{}{}",
            unit_text(block.unit),
            match extras(block.dynamic, block.has_attributes) {
                e if e.is_empty() => e,
                e => format!(", {e}"),
            }
        );
        let facts = |ui: &mut egui::Ui| {
            ui.weak(line);
        };
        match name_line(ui, theme, &block.name, facts, typing) {
            NameEdit::Nothing => {}
            NameEdit::Start => actions.push(Action::Edit(Some(Editing::RenameBlock {
                id: block.id,
                text: block.name.clone(),
            }))),
            NameEdit::Accept(name) => actions.push(Action::RenameBlock(block.id, name)),
            NameEdit::Cancel => actions.push(Action::Edit(None)),
        }
        if !block.components.is_empty() {
            ui.weak(format!("Contains: {}", block.components.join(", ")));
        }
        // Plain rows with the field width worked out here, not a grid: on its
        // first frame a grid offers a field no width, the text wraps a letter
        // to a line, and the window grows to hold that.
        let field = (ui.available_width() - LABEL - ui.spacing().item_spacing.x).max(120.0);
        ui.horizontal_top(|ui| {
            label(ui, "Description");
            let edit = ui.add(
                egui::TextEdit::multiline(&mut self.details.description)
                    .desired_rows(2)
                    .desired_width(field),
            );
            if edit.lost_focus() && self.details.description != block.description {
                actions.push(Action::SetDescription(
                    block.id,
                    self.details.description.clone(),
                ));
            }
        });
        ui.horizontal(|ui| {
            label(ui, "Tags");
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.details.tags)
                    .hint_text("comma separated")
                    .desired_width(field),
            );
            if edit.lost_focus() && parse_tags(&self.details.tags) != block.tags {
                actions.push(Action::SetTags(block.id, parse_tags(&self.details.tags)));
            }
        });
        ui.horizontal(|ui| {
            if opendraft_ui::widgets::primary_button(ui, theme, "Insert").clicked() {
                actions.push(Action::Insert(block.id));
            }
            let star = if block.favourite {
                "\u{2605} Favourite"
            } else {
                "\u{2606} Favourite"
            };
            if ui.button(star).clicked() {
                actions.push(Action::Favourite(block.id, !block.favourite));
            }
            if ui.button("Delete").clicked() {
                actions.push(Action::Delete(block.id));
            }
        });
    }
}

/// What is shown after a block's unit: whether it is dynamic or has
/// attributes.
fn extras(dynamic: bool, has_attributes: bool) -> String {
    let words: Vec<&str> = [(dynamic, "dynamic"), (has_attributes, "has attributes")]
        .into_iter()
        .filter_map(|(on, word)| on.then_some(word))
        .collect();
    words.join(", ")
}
