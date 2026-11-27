//! A block's name at the top of its details, with a pencil beside it to
//! rename the block there: the name turns into a field, Enter or OK keeps
//! the new name, Escape or Cancel leaves it.

use eframe::egui;
use opendraft_ui::{icons, Theme};

use super::chrome::icon_button;

/// What the name line asks for this frame.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum NameEdit {
    Nothing,
    /// The pencil was clicked.
    Start,
    Accept(String),
    Cancel,
}

/// The name as a heading, then the pencil and `facts` (unit, dynamic...);
/// or, while `typing` holds the name being typed, a field in its place.
pub(super) fn name_line(
    ui: &mut egui::Ui,
    theme: &Theme,
    name: &str,
    facts: &str,
    typing: Option<&mut String>,
) -> NameEdit {
    let mut asked = NameEdit::Nothing;
    ui.horizontal(|ui| {
        let Some(text) = typing else {
            ui.label(
                egui::RichText::new(name)
                    .size(theme.font_size_heading)
                    .color(theme.colors.text),
            );
            if icon_button(ui, theme, icons::CONTENT_RENAME, false, true, "Rename").clicked() {
                asked = NameEdit::Start;
            }
            ui.weak(facts);
            return;
        };
        // Read before the field: on Enter it gives up the focus it is given
        // again below, so it never reports having lost it.
        let id = egui::Id::new("content_manager_block_name");
        let typing_here = ui.memory(|m| m.has_focus(id));
        let (enter, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        ui.add(
            egui::TextEdit::singleline(text)
                .id(id)
                .font(egui::FontId::proportional(theme.font_size_heading))
                .desired_width(220.0),
        )
        .request_focus();
        if ui.button("OK").clicked() || (typing_here && enter) {
            asked = NameEdit::Accept(text.trim().to_owned());
        }
        if ui.button("Cancel").clicked() || escape {
            asked = NameEdit::Cancel;
        }
    });
    asked
}
