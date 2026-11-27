//! What the window offers on a block of the open drawing ("This drawing").

use eframe::egui;
use opendraft_core::BlockId;

use super::actions::{Action, Taken};
use super::{Editing, Selected};

/// The right-click menu of a block of the drawing.
pub(super) fn drawing_menu(ui: &mut egui::Ui, id: BlockId, name: &str, actions: &mut Vec<Action>) {
    if ui.button("Add to library").clicked() {
        actions.push(Action::AddFromDrawing(name.to_owned(), Taken::Cancel));
        ui.close_menu();
    }
    if ui.button("Rename").clicked() {
        actions.extend(rename(id, name));
        ui.close_menu();
    }
}

/// Choose the block and open the name bar on its name.
pub(super) fn rename(id: BlockId, name: &str) -> [Action; 2] {
    [
        Action::Select(Selected::Drawing(name.to_owned())),
        Action::Edit(Some(Editing::RenameDrawingBlock {
            id,
            text: name.to_owned(),
        })),
    ]
}
